use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_valid::Valid;
use sea_orm::prelude::Uuid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait,
};

use crate::AppState;
use crate::auth_helpers::require_tenant_admin;
use crate::auth_helpers::require_tenant_owner;
use crate::error::AppError;
use crate::extractors::{AuthMethod, AuthUser};
use crate::openapi::{CrudErrors, PersonalTokenAuthErrors, SessionAuthErrors};
use entity::scopes::ScopeList;
use entity::{
    personal_tokens::{self},
    projects,
};
use payload::personal_tokens::*;
use service::auth;
use service::auth::AuthError;

fn token_last_four(token: &str) -> String {
    token[token.len().saturating_sub(4)..].to_string()
}

/// プロジェクト制限は、選んだテナントのどれかに属するプロジェクトだけを指せる。
/// 各テナントに 1 つ以上要る（制限付きの PAT はテナント全体の操作ができないので、
/// 指すプロジェクトを持たないテナントでは何もできない束縛になる）。
async fn validate_project_ids(
    state: &AppState,
    tenant_ids: &[Uuid],
    project_ids: &[Uuid],
) -> Result<(), AppError> {
    if project_ids.is_empty() {
        return Err(AppError::BadRequest);
    }

    let unique_ids: std::collections::HashSet<Uuid> = project_ids.iter().copied().collect();

    let found: Vec<(Uuid, Uuid)> = projects::Entity::find()
        .select_only()
        .column(projects::Column::Id)
        .column(projects::Column::TenantId)
        .filter(projects::Column::TenantId.is_in(tenant_ids.iter().copied()))
        .filter(projects::Column::Id.is_in(unique_ids.iter().copied().collect::<Vec<_>>()))
        .into_tuple()
        .all(&state.db)
        .await?;

    if found.len() != unique_ids.len() {
        return Err(AppError::Forbidden);
    }
    let covered: std::collections::HashSet<Uuid> =
        found.into_iter().map(|(_, tenant_id)| tenant_id).collect();
    if tenant_ids.iter().any(|id| !covered.contains(id)) {
        return Err(AppError::BadRequest);
    }
    Ok(())
}

async fn get_owned_token(
    state: &AppState,
    token_id: Uuid,
    user_id: Uuid,
) -> Result<personal_tokens::Model, AppError> {
    personal_tokens::Entity::find_by_id(token_id)
        .filter(personal_tokens::Column::UserId.eq(user_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

#[axum::debug_handler(state = AppState)]
#[utoipa::path(
    get,
    path = "/me",
    tag = "Personal Tokens",
    summary = "使用中のパーソナルアクセストークンを表示",
    security(("bearerAuth" = [])),
    responses(
        (
            status = 200,
            description = "Bearer 認証に使ったトークンと持ち主の識別情報",
            body = PersonalTokenIdentityResponse
        ),
        PersonalTokenAuthErrors,
    )
)]
pub async fn get_current_personal_token(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<PersonalTokenIdentityResponse>, AuthError> {
    let user = entity::users::Entity::find_by_id(auth.user_id)
        .one(&state.db)
        .await?
        .ok_or(AuthError::Unauthorized)?;
    personal_token_identity(auth, user.account_kind).await
}

async fn personal_token_identity(
    auth: AuthUser,
    account_kind: String,
) -> Result<Json<PersonalTokenIdentityResponse>, AuthError> {
    let AuthMethod::PersonalToken {
        token_id,
        token_name,
        tenant_ids,
        allowed_project_ids,
        scopes,
        expires_at,
    } = auth.method
    else {
        return Err(AuthError::Unauthorized);
    };

    Ok(Json(PersonalTokenIdentityResponse {
        id: token_id,
        name: token_name,
        user_id: auth.user_id,
        username: auth.username,
        account_kind,
        // 旧 CLI 向け。複数のときに返すと、旧 CLI がその 1 つへ黙って書き込む
        tenant_id: match tenant_ids.as_slice() {
            [only] => Some(*only),
            _ => None,
        },
        tenant_ids,
        scopes: scopes.0.into_iter().map(Into::into).collect(),
        allowed_project_ids,
        expires_at: expires_at.map(|value| value.with_timezone(&chrono::Utc)),
    }))
}

#[cfg(test)]
mod identity_tests {
    use chrono::{Duration, Utc};
    use entity::scopes::{Scope, ScopeList};

    use super::*;

    fn personal_token_auth_in(
        tenant_ids: Vec<Uuid>,
        scopes: ScopeList,
        project_ids: Option<Vec<Uuid>>,
    ) -> AuthUser {
        AuthUser {
            user_id: Uuid::new_v4(),
            username: "automation".into(),
            method: AuthMethod::PersonalToken {
                token_id: Uuid::new_v4(),
                token_name: "review-bot".into(),
                tenant_ids,
                allowed_project_ids: project_ids,
                scopes,
                expires_at: Some((Utc::now() + Duration::days(30)).into()),
            },
        }
    }

    fn personal_token_auth(scopes: ScopeList, project_ids: Option<Vec<Uuid>>) -> AuthUser {
        personal_token_auth_in(vec![Uuid::new_v4()], scopes, project_ids)
    }

    #[tokio::test]
    async fn identity_requires_no_scope_and_reports_current_bounds() {
        let projects = vec![Uuid::new_v4()];
        let auth = personal_token_auth(ScopeList(vec![]), Some(projects.clone()));

        let Json(identity) = personal_token_identity(auth, "human".into())
            .await
            .expect("PAT identity");

        assert_eq!(identity.name, "review-bot");
        assert_eq!(identity.username, "automation");
        assert!(identity.scopes.is_empty());
        assert_eq!(identity.allowed_project_ids, Some(projects));
        assert!(identity.expires_at.is_some());
    }

    #[tokio::test]
    async fn identity_reports_the_bound_tenant() {
        let tenant_id = Uuid::new_v4();
        let auth = personal_token_auth_in(vec![tenant_id], ScopeList(vec![]), None);

        let Json(identity) = personal_token_identity(auth, "human".into())
            .await
            .expect("PAT identity");

        // 鍵だけ渡された者が行き先を知れるように、結び付いたテナントを返す
        assert_eq!(identity.tenant_ids, vec![tenant_id]);
        assert_eq!(identity.tenant_id, Some(tenant_id));
    }

    #[tokio::test]
    async fn identity_omits_the_legacy_tenant_id_for_several_tenants() {
        let tenants = vec![Uuid::new_v4(), Uuid::new_v4()];
        let auth = personal_token_auth_in(tenants.clone(), ScopeList(vec![]), None);

        let Json(identity) = personal_token_identity(auth, "human".into())
            .await
            .expect("PAT identity");

        assert_eq!(identity.tenant_ids, tenants);
        assert_eq!(identity.tenant_id, None);
    }

    #[tokio::test]
    async fn identity_accepts_a_narrow_scope() {
        let auth = personal_token_auth(ScopeList(vec![Scope::ReadTask]), None);

        let Json(identity) = personal_token_identity(auth, "human".into())
            .await
            .expect("PAT identity");

        assert_eq!(identity.scopes, vec![Scope::ReadTask]);
        assert_eq!(identity.allowed_project_ids, None);
    }

    #[tokio::test]
    async fn identity_rejects_a_session() {
        let auth = AuthUser {
            user_id: Uuid::new_v4(),
            username: "session-user".into(),
            method: AuthMethod::Session,
        };

        let error = personal_token_identity(auth, "human".into())
            .await
            .expect_err("session must not use PAT identity");

        assert!(matches!(error, AuthError::Unauthorized));
    }
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Personal Tokens",
    summary = "自分のパーソナルアクセストークンの一覧",
    responses(
        (
            status = 200,
            description = "取り消し済みを除く、自分が発行したトークンの一覧",
            body = Vec<PersonalTokenResponse>
        ),
        SessionAuthErrors,
    )
)]
pub async fn list_personal_tokens(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<PersonalTokenResponse>>, AppError> {
    auth.require_session()?;

    let tokens = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(auth.user_id))
        .filter(personal_tokens::Column::Revoked.eq(false))
        .order_by_asc(personal_tokens::Column::Name)
        .order_by_asc(personal_tokens::Column::Id)
        .all(&state.db)
        .await?;

    let token_ids: Vec<Uuid> = tokens.iter().map(|token| token.id).collect();
    let mut bindings = auth::personal_token_tenant_ids_by_token(&state.db, &token_ids).await?;

    // 束縛が尽きたトークン（テナントの削除・一括失効のあと）は認証でも通らないので出さない
    let resp = tokens
        .into_iter()
        .filter_map(|token| {
            let tenant_ids = bindings.remove(&token.id)?;
            Some(PersonalTokenResponse::from_parts(token, tenant_ids))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(resp))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "Personal Tokens",
    summary = "パーソナルアクセストークンを発行",
    request_body = CreatePersonalTokenRequest,
    responses(
        (
            status = 201,
            description = "発行したトークンの情報（平文トークンはこの応答でのみ返却）",
            body = CreatePersonalTokenResponse
        ),
        SessionAuthErrors,
    )
)]
pub async fn create_personal_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Valid(Json(payload)): Valid<Json<CreatePersonalTokenRequest>>,
) -> Result<(StatusCode, Json<CreatePersonalTokenResponse>), AppError> {
    auth.require_session()?;
    // 重複は落とす（順序は保つ）。1 件でも通らなければ全体を拒否する
    let mut seen = std::collections::HashSet::new();
    let tenant_ids: Vec<Uuid> = payload
        .tenant_ids
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .collect();
    for &tenant_id in &tenant_ids {
        // tenant_members と同じく、PAT を結び付けるテナントの所属と 2FA の強制を先に確かめる。
        // 強制の前に取った本認証のセッションでも、2FA を満たさなければ発行できない
        auth.ensure_tenant_access(&state, tenant_id, None).await?;
        // 発行はオーナーとテナント Admin に許す。PAT は発行者本人として動き
        // （user_id は発行者に固定）、得られる力は発行者の持つ力を超えぬため、
        // 主に閉じても守れる物が増えない（docs/personal-access-tokens-authz.md）。
        require_tenant_admin(&state.db, tenant_id, auth.user_id).await?;
    }

    if let Some(ref project_ids) = payload.project_ids {
        validate_project_ids(&state, &tenant_ids, project_ids).await?;
    }

    let secret = &state.settings.personal_token_secret;
    let (token_value, token_hash) =
        auth::generate_personal_token(secret).map_err(|e| AppError::Internal(e.into()))?;

    let allowed_project_ids = payload.project_ids.map(|ids| serde_json::json!(ids));

    let txn = state.db.begin().await?;
    let model = personal_tokens::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(payload.name),
        token_last_four: Set(token_last_four(&token_value)),
        token_hash: Set(token_hash),
        expires_at: Set(payload.expires_at.map(Into::into)),
        last_used_at: Set(None),
        revoked: Set(false),
        user_id: Set(auth.user_id),
        scopes: Set(ScopeList(payload.scopes)),
        allowed_project_ids: Set(allowed_project_ids),
    }
    .insert(&txn)
    .await?;
    auth::bind_personal_token_tenants(&txn, model.id, &tenant_ids).await?;
    txn.commit().await?;

    let mut tenant_ids = tenant_ids;
    tenant_ids.sort_unstable();
    let resp = CreatePersonalTokenResponse::new(token_value, model, tenant_ids)
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok((StatusCode::CREATED, Json(resp)))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}",
    tag = "Personal Tokens",
    summary = "指定したトークンを参照",
    params(("id" = Uuid, Path, description = "トークンの識別子")),
    responses(
        (
            status = 200,
            description = "トークンの状態",
            body = PersonalTokenResponse
        ),
        CrudErrors,
    )
)]
pub async fn get_personal_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<PersonalTokenResponse>, AppError> {
    auth.require_session()?;
    let token = get_owned_token(&state, id, auth.user_id).await?;
    let tenant_ids = auth::personal_token_tenant_ids(&state.db, token.id).await?;
    // 束縛が尽きたトークンは一覧にも出さず認証でも通らないので、単体取得も 404 にそろえる
    if tenant_ids.is_empty() {
        return Err(AppError::NotFound);
    }
    let resp = PersonalTokenResponse::from_parts(token, tenant_ids)
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(resp))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Personal Tokens",
    summary = "指定したトークンを取り消し",
    params(("id" = Uuid, Path, description = "トークンの識別子")),
    responses(
        (status = 204, description = "取り消しました"),
        CrudErrors,
    )
)]
pub async fn revoke_personal_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    auth.require_session()?;
    let token = get_owned_token(&state, id, auth.user_id).await?;

    if token.revoked {
        return Ok(StatusCode::NO_CONTENT);
    }

    let mut active: personal_tokens::ActiveModel = token.into();
    active.revoked = Set(true);
    active.update(&state.db).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/revoke-all",
    tag = "Personal Tokens",
    summary = "テナントの束縛をすべてのトークンから外す（束縛が無くなったトークンは取り消す）",
    request_body = RevokeAllPersonalTokensRequest,
    responses(
        (status = 204, description = "取り消しました"),
        CrudErrors,
    )
)]
pub async fn revoke_all_personal_tokens(
    State(state): State<AppState>,
    auth: AuthUser,
    Valid(Json(payload)): Valid<Json<RevokeAllPersonalTokensRequest>>,
) -> Result<StatusCode, AppError> {
    auth.require_session()?;
    // revoke-all は主に限ったまま（この境目はこの変更では広げない。
    // 広げるべきかは別途裁きを仰ぐ）。判定は共用の require_tenant_owner に寄せる
    // ——無いテナントは NotFound、主でなければ Forbidden で従前と同じ。
    require_tenant_owner(&state.db, payload.confirm_tenant_id, auth.user_id).await?;

    service::bots::revoke_all_tenant_tokens(&state.db, payload.confirm_tenant_id, auth.user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
