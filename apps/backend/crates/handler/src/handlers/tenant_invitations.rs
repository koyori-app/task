//! テナントへのメール招待（apps/backend/docs/tenant-project-authz.md の「招待」）。
//!
//! 発行・一覧・取り消し・再送はメンバー管理と同じくオーナーとテナント Admin に許す。
//! 承諾は招待先のアドレスでログインしている本人だけ（セッション必須）。

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_valid::Valid;
use sea_orm::prelude::Uuid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};

use crate::AppState;
use crate::error::{AppError, ServerError};
use crate::extractors::{AuthUser, CurrentUser};
use crate::handlers::tenant_members::require_tenant_admin;
use crate::openapi::CrudErrors;
use entity::tenant_members::TenantRole;
use entity::{scopes::Scope, tenant_invitations, tenant_members, tenants, users};
use job::tenant_invitation_email;
use payload::tenant_invitations::*;
use payload::tenant_members::TenantMemberResponse;
use service::db::is_postgres_unique_violation;
use service::email::normalize_email;
use service::tenant_invitations::{
    find_by_token, is_expired, release_send_slot, try_acquire_send_slot, try_consume_send_quota,
};

/// 発行・一覧・取り消し・再送に共通する前段。テナントが無ければ 404、管理者でなければ 403。
async fn require_invitation_admin(
    state: &AppState,
    auth: &AuthUser,
    tenant_id: Uuid,
) -> Result<(), AppError> {
    auth.require_scope(Scope::AdminTenant)?;
    auth.ensure_tenant_access(state, tenant_id, None).await?;
    require_tenant_admin(state, tenant_id, auth.user_id).await
}

/// このアドレスの利用者が既にテナントに居るか（オーナーを含む）。
async fn is_member_email(state: &AppState, tenant_id: Uuid, email: &str) -> Result<bool, AppError> {
    let Some(user) = users::Entity::find()
        .filter(users::Column::Email.eq(email))
        .one(&state.db)
        .await?
    else {
        return Ok(false);
    };
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if tenant.owner_id == user.id {
        return Ok(true);
    }
    Ok(tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(tenant_id))
        .filter(tenant_members::Column::UserId.eq(user.id))
        .one(&state.db)
        .await?
        .is_some())
}

/// 宛先ごとの送信の枠を取り、テナントごと・招待者ごとの上限を 1 通ぶん使う。
/// 返すときに使う宛先の枠の持ち主の印を返す。どちらかに届いていれば 429。
async fn acquire_send_slot(
    state: &AppState,
    tenant_id: Uuid,
    inviter_id: Uuid,
    email: &str,
) -> Result<String, AppError> {
    let slot = try_acquire_send_slot(&state.redis_client, tenant_id, email)
        .await?
        .ok_or(AppError::TooManyRequests)?;
    if !try_consume_send_quota(&state.redis_client, tenant_id, inviter_id).await? {
        release_send_slot(&state.redis_client, tenant_id, email, &slot).await?;
        return Err(AppError::TooManyRequests);
    }
    Ok(slot)
}

/// 送信の枠を取ったあとの書き込み（招待の更新とジョブの投入）が失敗したら、枠を返して
/// 失敗をそのまま伝える。招待の更新とジョブの投入は 1 トランザクションなので、前のリンクも
/// 生きたまま残る。枠を返さないと、直後のやり直しが 429 になる。
async fn release_on_error<T>(
    state: &AppState,
    tenant_id: Uuid,
    email: &str,
    slot: &str,
    result: Result<T, anyhow::Error>,
) -> Result<T, AppError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            release_send_slot(&state.redis_client, tenant_id, email, slot).await?;
            Err(error.into())
        }
    }
}

async fn load_invitation(
    state: &AppState,
    invitation_id: Uuid,
) -> Result<tenant_invitations::Model, AppError> {
    // 書き込みを確定した直後に読むので、無ければ（間で取り消された）404
    tenant_invitations::Entity::find_by_id(invitation_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Tenant Invitations",
    summary = "保留中の招待一覧",
    description = "期限切れの招待も返す（再送で期限を延ばせるため）。新しい順。",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 200, description = "保留中の招待", body = [TenantInvitationResponse]),
        CrudErrors,
    )
)]
pub async fn list_invitations(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<TenantInvitationResponse>>, AppError> {
    require_invitation_admin(&state, &auth, tenant_id).await?;
    let invitations = tenant_invitations::Entity::find()
        .filter(tenant_invitations::Column::TenantId.eq(tenant_id))
        .order_by_desc(tenant_invitations::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(invitations.into_iter().map(Into::into).collect()))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "Tenant Invitations",
    summary = "メールで招待する",
    description = "同じアドレスへの保留中の招待があれば、ロールと期限を更新して作り直す（前のリンクは使えなくなる）。",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    request_body = CreateTenantInvitationRequest,
    responses(
        (status = 201, description = "発行した招待", body = TenantInvitationResponse),
        (status = 400, description = "メールアドレスの形が正しくない", body = ServerError),
        (status = 409, description = "既にテナントのメンバー（already-member）", body = ServerError),
        (status = 429, description = "同じアドレスへ続けて送ろうとした", body = ServerError),
        CrudErrors,
    )
)]
pub async fn create_invitation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Valid(Json(payload)): Valid<Json<CreateTenantInvitationRequest>>,
) -> Result<(StatusCode, Json<TenantInvitationResponse>), AppError> {
    require_invitation_admin(&state, &auth, tenant_id).await?;
    let email = normalize_email(&payload.email);
    if is_member_email(&state, tenant_id, &email).await? {
        return Err(AppError::ConflictDetail("already-member".into()));
    }
    let slot = acquire_send_slot(&state, tenant_id, auth.user_id, &email).await?;

    // 二重招待は (tenant_id, email) の UNIQUE で同じ行を作り直す。世代が上がるので、
    // 前のメールのリンクはここで通らなくなる
    let issued = tenant_invitation_email::issue_and_enqueue(
        &state.pg_pool,
        tenant_id,
        &email,
        &payload.role,
        auth.user_id,
    )
    .await;
    let invitation_id = release_on_error(&state, tenant_id, &email, &slot, issued).await?;
    let invitation = load_invitation(&state, invitation_id).await?;
    Ok((StatusCode::CREATED, Json(invitation.into())))
}

async fn find_invitation(
    state: &AppState,
    tenant_id: Uuid,
    invitation_id: Uuid,
) -> Result<tenant_invitations::Model, AppError> {
    tenant_invitations::Entity::find_by_id(invitation_id)
        .filter(tenant_invitations::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{invitation_id}/resend",
    tag = "Tenant Invitations",
    summary = "招待メールを送り直す",
    description = "期限を今から延ばし、トークンを発行し直して送る（前のリンクは使えなくなる）。",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("invitation_id" = Uuid, Path, description = "招待ID"),
    ),
    responses(
        (status = 200, description = "送り直した招待", body = TenantInvitationResponse),
        (status = 429, description = "同じアドレスへ続けて送ろうとした", body = ServerError),
        CrudErrors,
    )
)]
pub async fn resend_invitation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, invitation_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<TenantInvitationResponse>, AppError> {
    require_invitation_admin(&state, &auth, tenant_id).await?;
    let invitation = find_invitation(&state, tenant_id, invitation_id).await?;
    let slot = acquire_send_slot(&state, tenant_id, auth.user_id, &invitation.email).await?;

    let resent =
        tenant_invitation_email::resend_and_enqueue(&state.pg_pool, tenant_id, invitation_id).await;
    if !release_on_error(&state, tenant_id, &invitation.email, &slot, resent).await? {
        // 確かめた後に取り消された
        release_send_slot(&state.redis_client, tenant_id, &invitation.email, &slot).await?;
        return Err(AppError::NotFound);
    }
    let invitation = load_invitation(&state, invitation_id).await?;
    Ok(Json(invitation.into()))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{invitation_id}",
    tag = "Tenant Invitations",
    summary = "招待を取り消す",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("invitation_id" = Uuid, Path, description = "招待ID"),
    ),
    responses(
        (status = 204, description = "取り消しました"),
        CrudErrors,
    )
)]
pub async fn delete_invitation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, invitation_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    require_invitation_admin(&state, &auth, tenant_id).await?;
    let deleted = tenant_invitations::Entity::delete_many()
        .filter(tenant_invitations::Column::Id.eq(invitation_id))
        .filter(tenant_invitations::Column::TenantId.eq(tenant_id))
        .exec(&state.db)
        .await?;
    if deleted.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// 招待者が今もテナントのオーナーか `Admin` か。
///
/// 発行時は `require_invitation_admin` で確かめるが、降格・除名は保留中の招待に手を付けない。
/// 承諾の側で確かめないと、外される前に出しておいた招待で（自分の別アドレスを `Admin` として
/// 招待しておけば）外された後に戻れてしまう。
async fn inviter_still_admin<C: ConnectionTrait>(
    db: &C,
    invitation: &tenant_invitations::Model,
) -> Result<bool, AppError> {
    let tenant = tenants::Entity::find_by_id(invitation.tenant_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    if tenant.owner_id == invitation.invited_by {
        return Ok(true);
    }
    Ok(tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(invitation.tenant_id))
        .filter(tenant_members::Column::UserId.eq(invitation.invited_by))
        .filter(tenant_members::Column::Role.eq(TenantRole::Admin))
        .one(db)
        .await?
        .is_some())
}

/// トークンから保留中の招待を引く。無ければ 404（取り消し・承諾済み・発行し直し・招待者が
/// `Admin` でなくなったものを含む）、期限切れなら 410。
async fn pending_by_token(
    state: &AppState,
    token: &str,
) -> Result<tenant_invitations::Model, AppError> {
    let invitation = find_by_token(&state.db, token, &state.settings.personal_token_secret)
        .await?
        .ok_or(AppError::NotFound)?;
    if is_expired(&invitation) {
        return Err(AppError::Gone);
    }
    if !inviter_still_admin(&state.db, &invitation).await? {
        return Err(AppError::NotFound);
    }
    Ok(invitation)
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/preview",
    tag = "Tenant Invitations",
    summary = "招待の中身を見る",
    description = "承諾画面の表示用。ログインは要らない（未登録の受け手にも見せる）。",
    request_body = InvitationTokenRequest,
    responses(
        (status = 200, description = "招待の中身", body = InvitationPreviewResponse),
        (status = 404, description = "招待が無い（取り消し・承諾済み・発行し直し・招待者が Admin でなくなった）", body = ServerError),
        (status = 410, description = "招待の期限が切れている", body = ServerError),
        (status = 500, description = "サーバー側で問題が発生しました", body = ServerError),
    )
)]
pub async fn preview_invitation(
    State(state): State<AppState>,
    Valid(Json(payload)): Valid<Json<InvitationTokenRequest>>,
) -> Result<Json<InvitationPreviewResponse>, AppError> {
    let invitation = pending_by_token(&state, &payload.token).await?;
    let tenant = tenants::Entity::find_by_id(invitation.tenant_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("invitation {} has no tenant row", invitation.id))?;
    let inviter = users::Entity::find_by_id(invitation.invited_by)
        .one(&state.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("invitation {} has no inviter row", invitation.id))?;
    Ok(Json(InvitationPreviewResponse {
        tenant_id: tenant.id,
        tenant_name: tenant.name,
        tenant_display_id: tenant.display_id,
        email: invitation.email,
        role: invitation.role,
        invited_by: inviter.username,
        expires_at: invitation.expires_at.into(),
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/accept",
    tag = "Tenant Invitations",
    summary = "招待を承諾してテナントに入る",
    description = "招待先のアドレスでログインしている本人だけが承諾できる（セッション必須）。承諾した招待は消える。",
    request_body = InvitationTokenRequest,
    responses(
        (status = 201, description = "加わったメンバー", body = TenantMemberResponse),
        (status = 401, description = "ログインが必要です", body = ServerError),
        (status = 403, description = "招待先とログイン中のアドレスが違う（invitation-email-mismatch）", body = ServerError),
        (status = 404, description = "招待が無い（取り消し・承諾済み・発行し直し・招待者が Admin でなくなった）", body = ServerError),
        (status = 409, description = "既にテナントのメンバー（already-member）", body = ServerError),
        (status = 410, description = "招待の期限が切れている", body = ServerError),
        (status = 500, description = "サーバー側で問題が発生しました", body = ServerError),
    )
)]
pub async fn accept_invitation(
    State(state): State<AppState>,
    user: CurrentUser,
    Valid(Json(payload)): Valid<Json<InvitationTokenRequest>>,
) -> Result<(StatusCode, Json<TenantMemberResponse>), AppError> {
    let invitation = pending_by_token(&state, &payload.token).await?;
    // リンクの転送だけで別人が入らないよう、宛先のアドレスを持つ本人に限る。
    // ログインはメール確認済みでないと通らないが、OAuth 経由の利用者もいるので確かめ直す
    if normalize_email(&user.email) != invitation.email || !user.email_verified {
        return Err(AppError::ForbiddenDetail(
            "invitation-email-mismatch".into(),
        ));
    }
    if is_member_email(&state, invitation.tenant_id, &invitation.email).await? {
        return Err(AppError::ConflictDetail("already-member".into()));
    }

    let txn = state.db.begin().await?;
    // 確かめた後に招待者が外されていないか、書き込みと同じトランザクションで見直す
    if !inviter_still_admin(&txn, &invitation).await? {
        return Err(AppError::NotFound);
    }
    // 同じトークンでの承諾が並んだとき、招待を消せた 1 つだけを通す。
    // 間に取り消し・発行し直しが入った場合も消せないので 404 になる
    let consumed = tenant_invitations::Entity::delete_many()
        .filter(tenant_invitations::Column::Id.eq(invitation.id))
        .filter(tenant_invitations::Column::Generation.eq(invitation.generation))
        .exec(&txn)
        .await?;
    if consumed.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    let member = match (tenant_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        tenant_id: Set(invitation.tenant_id),
        user_id: Set(user.id),
        role: Set(invitation.role),
    })
    .insert(&txn)
    .await
    {
        Ok(member) => member,
        // 確認の後に別経路（メンバー追加 API）で入っていた。招待は残す（ロールバック）
        Err(e) if is_postgres_unique_violation(&e) => {
            return Err(AppError::ConflictDetail("already-member".into()));
        }
        Err(e) => return Err(e.into()),
    };
    txn.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(TenantMemberResponse::from_parts(member, user.0)),
    ))
}
