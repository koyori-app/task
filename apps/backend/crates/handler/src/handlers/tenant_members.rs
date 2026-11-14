use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use axum_valid::Valid;
use sea_orm::prelude::Uuid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
};

use crate::AppState;
use crate::auth_helpers::{explicit_member_project_ids, is_tenant_owner, require_tenant_admin};
use crate::error::{AppError, ServerError};
use crate::extractors::AuthUser;
use crate::handlers::project_members;
use crate::openapi::CrudErrors;
use entity::{
    project_members as project_members_entity, projects, scopes::Scope, tenant_members, tenants,
    users,
};
use payload::tenant_members::*;
use payload::tenants::TenantMembershipKind;
use payload::users::UserProfileResponse;
use service::access::visible_project_ids;
use service::db::is_postgres_unique_violation;

/// メンバー行に表示用のユーザー情報を同梱する。FK があるため利用者は必ず居るはずで、
/// 居なければ握り潰さず 500 にする。
async fn attach_users(
    state: &AppState,
    members: Vec<tenant_members::Model>,
) -> Result<Vec<TenantMemberResponse>, AppError> {
    let user_ids: Vec<Uuid> = members.iter().map(|m| m.user_id).collect();
    let mut users_by_id: std::collections::HashMap<Uuid, users::Model> = users::Entity::find()
        .filter(users::Column::Id.is_in(user_ids))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|u| (u.id, u))
        .collect();
    members
        .into_iter()
        .map(|m| {
            let user = users_by_id
                .remove(&m.user_id)
                .ok_or_else(|| anyhow::anyhow!("tenant member {} has no user row", m.user_id))?;
            Ok(TenantMemberResponse::from_parts(m, user))
        })
        .collect()
}

/// 部署・役職・チームの入力を保存する値にする。前後の空白を除き、空なら NULL
fn org_profile_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

async fn find_member_row(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Option<tenant_members::Model>, AppError> {
    Ok(tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(tenant_id))
        .filter(tenant_members::Column::UserId.eq(user_id))
        .one(&state.db)
        .await?)
}

async fn find_member(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<tenant_members::Model, AppError> {
    find_member_row(state, tenant_id, user_id)
        .await?
        .ok_or(AppError::NotFound)
}

/// 所属行の無いオーナー本人が、自分の所属をテナントの行へ保存する。
///
/// `tenant_members` に行を足すと、通知の宛先や「最後の Admin」の数え方までオーナーを
/// メンバーとして扱い始めるので足さない（apps/backend/docs/tenant-project-authz.md）。
/// 開くのは本人の所属だけで、他の人がオーナーの所属を変える口にも、オーナーのロールを
/// 変える口にもしない。どちらも行の無い相手への PUT として従来どおり 404 にする
async fn update_owner_org_profile(
    state: &AppState,
    actor_id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    payload: UpdateTenantMemberRequest,
) -> Result<TenantMemberResponse, AppError> {
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if user_id != tenant.owner_id || actor_id != tenant.owner_id || payload.role.is_some() {
        return Err(AppError::NotFound);
    }
    let user = users::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("tenant {tenant_id} has no owner row"))?;
    let mut active: tenants::ActiveModel = tenant.into();
    if let Some(department) = payload.department {
        active.owner_department = Set(org_profile_value(&department));
    }
    if let Some(position) = payload.position {
        active.owner_position = Set(org_profile_value(&position));
    }
    if let Some(team) = payload.team {
        active.owner_team = Set(org_profile_value(&team));
    }
    let updated = active.update(&state.db).await?;
    Ok(TenantMemberResponse::from_owner(&updated, user))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Tenant Members",
    summary = "テナントメンバー一覧",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 200, description = "メンバー一覧", body = [TenantMemberResponse]),
        CrudErrors,
    )
)]
pub async fn list_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<TenantMemberResponse>>, AppError> {
    auth.require_scope(Scope::ReadTenant)?;
    // 追加・変更・削除と違い、一覧の閲覧はテナントに入れる人なら誰でも許す
    // （PAT は読み取りなので `read:tenant` を要求する）
    auth.ensure_tenant_access(&state, tenant_id, None).await?;

    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let members = tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(tenant_id))
        .all(&state.db)
        .await?;
    let mut response = attach_users(&state, members).await?;

    // owner は認可上 tenant_members に行を持たないが、メンバー管理画面では
    // 常に表示できる必要がある。実在するメンバー行とは区別して synthetic row を返す。
    // ロール変更と削除の対象にはせず、本人が所属を直す PUT だけを受ける（`update_owner_org_profile`）。
    if !response
        .iter()
        .any(|member| member.user_id == tenant.owner_id)
    {
        let owner = users::Entity::find_by_id(tenant.owner_id)
            .one(&state.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("tenant {} has no owner row", tenant_id))?;
        response.insert(0, TenantMemberResponse::from_owner(&tenant, owner));
    }

    Ok(Json(response))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{tenant_id}/users/{user_id}",
    tag = "Tenant Members",
    summary = "利用者のプロフィールを取得",
    description = "テナントのオーナー・メンバー・客分のプロフィールを返す。メールアドレス等の本人向け項目は含めない。",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("user_id" = Uuid, Path, description = "ユーザーID"),
    ),
    responses(
        (status = 200, description = "プロフィール", body = UserProfileResponse),
        CrudErrors,
    )
)]
pub async fn get_user_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserProfileResponse>, AppError> {
    auth.require_scope(Scope::ReadTenant)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;

    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let member = find_member_row(&state, tenant_id, user_id).await?;
    // users はテナント横断の表なので、テナントに関わらない人は 403 ではなく 404 にして存在を明かさない
    let visible = member.is_some()
        || user_id == tenant.owner_id
        || !explicit_member_project_ids(&state.db, tenant_id, user_id)
            .await?
            .is_empty();
    if !visible {
        return Err(AppError::NotFound);
    }
    let user = users::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    // 部署・役職・チームはメンバーなら所属行、オーナーならテナントの行。客分では null
    let org = OrgProfile::resolve(&tenant, user_id, member.as_ref());
    // ここに来るのはオーナー・所属行あり・明示のプロジェクトメンバーのどれかなので、残りは客分
    let membership = if user_id == tenant.owner_id {
        TenantMembershipKind::Owner
    } else if member.is_some() {
        TenantMembershipKind::Member
    } else {
        TenantMembershipKind::Guest
    };
    Ok(Json(UserProfileResponse::from_parts(user, org, membership)))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "Tenant Members",
    summary = "テナントメンバーを追加",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    request_body = AddTenantMemberRequest,
    responses(
        (status = 201, description = "追加されたメンバー", body = TenantMemberResponse),
        (status = 409, description = "既にメンバーとして登録済み", body = ServerError),
        CrudErrors,
    )
)]
pub async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Valid(Json(payload)): Valid<Json<AddTenantMemberRequest>>,
) -> Result<(StatusCode, Json<TenantMemberResponse>), AppError> {
    auth.require_scope(Scope::WriteTenant)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;
    require_tenant_admin(&state.db, tenant_id, auth.user_id).await?;

    let user = users::Entity::find_by_id(payload.user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if user.is_bot() && user.bot_tenant_id != Some(tenant_id) {
        return Err(AppError::Forbidden);
    }
    if user.is_bot() && !payload.role.allowed_for_bot() {
        return Err(AppError::BadRequestDetail("bot-role-too-strong".into()));
    }
    // 二重追加は UNIQUE (tenant_id, user_id) に任せる。
    // 事前に引いてから INSERT すると、同時に 2 回追加されたときに 500 になる
    let member = match (tenant_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        tenant_id: Set(tenant_id),
        user_id: Set(payload.user_id),
        role: Set(payload.role),
        department: Set(None),
        position: Set(None),
        team: Set(None),
    })
    .insert(&state.db)
    .await
    {
        Ok(member) => member,
        Err(e) if is_postgres_unique_violation(&e) => return Err(AppError::Conflict),
        Err(e) => return Err(e.into()),
    };

    Ok((
        StatusCode::CREATED,
        Json(TenantMemberResponse::from_parts(member, user)),
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{user_id}",
    tag = "Tenant Members",
    summary = "テナントメンバーの権限・所属を変更",
    description = "どの欄も省略すれば変更しない。部署・役職・チームは前後の空白を除き、空なら未設定に戻す。所属行の無いオーナーには、本人だけが部署・役職・チームを送れる（role を付けた PUT と他の人の PUT は 404）。",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("user_id" = Uuid, Path, description = "ユーザーID"),
    ),
    request_body = UpdateTenantMemberRequest,
    responses(
        (status = 200, description = "変更後のメンバー", body = TenantMemberResponse),
        CrudErrors,
    )
)]
pub async fn update_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, user_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateTenantMemberRequest>>,
) -> Result<Json<TenantMemberResponse>, AppError> {
    auth.require_scope(Scope::WriteTenant)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;
    require_tenant_admin(&state.db, tenant_id, auth.user_id).await?;

    let Some(member) = find_member_row(&state, tenant_id, user_id).await? else {
        let owner =
            update_owner_org_profile(&state, auth.user_id, tenant_id, user_id, payload).await?;
        return Ok(Json(owner));
    };
    let user = users::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("tenant member {user_id} has no user row"))?;
    if user.is_bot()
        && payload
            .role
            .as_ref()
            .is_some_and(|role| !role.allowed_for_bot())
    {
        return Err(AppError::BadRequestDetail("bot-role-too-strong".into()));
    }
    let mut active: tenant_members::ActiveModel = member.into();
    if let Some(role) = payload.role {
        active.role = Set(role);
    }
    if let Some(department) = payload.department {
        active.department = Set(org_profile_value(&department));
    }
    if let Some(position) = payload.position {
        active.position = Set(org_profile_value(&position));
    }
    if let Some(team) = payload.team {
        active.team = Set(org_profile_value(&team));
    }
    let updated = active.update(&state.db).await?;
    Ok(Json(TenantMemberResponse::from_parts(updated, user)))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{user_id}",
    tag = "Tenant Members",
    summary = "テナントメンバーを削除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("user_id" = Uuid, Path, description = "ユーザーID"),
    ),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(Scope::WriteTenant)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;
    require_tenant_admin(&state.db, tenant_id, auth.user_id).await?;

    let member = find_member(&state, tenant_id, user_id).await?;
    let user = users::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if user.is_bot() {
        return Err(AppError::Forbidden);
    }

    // プロジェクト側の「最後の Admin を残す」判定は、テナントに在籍している Admin だけを
    // 数える（`project_members::would_drop_last_admin`）。除名はその数え方の入力を変えるので、
    // 判定と同じロックの内側で行う。外すと、降格が「まだ B が居る」を読んだ後・書く前に
    // B を除名でき、降格が消えた相手を数えたまま通る。
    //
    // ここで Admin 数を数え直して 409 にはしない。除名は「この人はもう居ない」という
    // 宣言で、それをプロジェクトのロールで止めると、対象が単独 Admin のプロジェクトを
    // 全部直すまでオフボーディングできなくなる。Admin が全員抜けたプロジェクトは
    // テナントオーナー（開いていればテナント Admin も）が直せる（`project_members::require_project_admin` は
    // オーナーを無条件で通す）。詳細は `lock_membership_changes` の doc を見る
    let txn = state.db.begin().await?;
    project_members::lock_membership_changes(&txn, tenant_id).await?;

    // 除名が消すのはテナント所属（継承）だけ。`project_members` の行はプロジェクト単位の
    // 明示 ACE であり、NTFS / NFSv4 の ACL と同じく継承元と独立して残る
    // （docs/dev/design/tenant-project-authz.md の「継承と明示」）。残った行の持ち主は
    // project-only の客分になり、名指しされたプロジェクトの中だけ引き続き入れる。
    // 副次的に、その人しか指定されていなかったプロジェクトがメンバー 0 人になって
    // 「メンバー未指定＝テナント全体に開放」へ戻ることも防ぎ、再参加時は元の割り当てが戻る。
    // 残る明示 ACE は `list_explicit_projects` で除名の前後どちらでも確かめられる。
    // 通知の宛先はテナントに居る人だけに絞られる（`service::access`。客分には飛ばない）
    tenant_members::Entity::delete_by_id(member.id)
        .exec(&txn)
        .await?;
    txn.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{user_id}/explicit-projects",
    tag = "Tenant Members",
    summary = "テナントメンバーが持つプロジェクト単位の明示 ACE",
    description = "テナント除名の前に「除名しても、この人はこれらのプロジェクトに入り続ける」と確かめるための口。除名の後に呼んでも同じ内容を返す。",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("user_id" = Uuid, Path, description = "ユーザーID"),
    ),
    responses(
        (status = 200, description = "明示 ACE の一覧", body = ExplicitProjectsResponse),
        CrudErrors,
    )
)]
pub async fn list_explicit_projects(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ExplicitProjectsResponse>, AppError> {
    auth.require_scope(Scope::ReadTenant)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;
    // 除名と同じ相手（オーナーとテナント Admin）にだけ見せる。
    // 対象がテナントに居るかは問わない。除名した後に「まだ何が残っているか」を確かめる用途があるため
    require_tenant_admin(&state.db, tenant_id, auth.user_id).await?;

    let rows = project_members_entity::Entity::find()
        .filter(project_members_entity::Column::UserId.eq(user_id))
        .find_also_related(projects::Entity)
        .all(&state.db)
        .await?;
    // 個人プロジェクト（Inbox）の自動生成行は客分の口にならないので、残る ACE にも数えない
    // （`access::is_shared_project_explicit_member` の doc）
    let mut aces: Vec<(project_members_entity::Model, projects::Model)> = rows
        .into_iter()
        .filter_map(|(m, p)| {
            p.filter(|p| p.tenant_id == tenant_id && !p.is_personal)
                .map(|p| (m, p))
        })
        .collect();

    // 非公開プロジェクトの名前や key を、そのプロジェクトに入れない人へ出さない。
    // テナント Admin でもプロジェクトの閲覧は `list_projects` と同じ境界で絞られるので、
    // ここも同じ判定（`visible_project_ids`）を通す。オーナーは全プロジェクトに入れる
    let visible = if is_tenant_owner(&state.db, tenant_id, auth.user_id).await? {
        aces.iter().map(|(_, p)| p.id).collect()
    } else {
        visible_project_ids(
            &state.db,
            aces.iter().map(|(_, p)| p.id).collect(),
            auth.user_id,
        )
        .await?
    };
    let hidden_count = aces
        .iter()
        .filter(|(_, p)| !visible.contains(&p.id))
        .count() as u64;
    aces.retain(|(_, p)| visible.contains(&p.id));
    aces.sort_by(|a, b| a.1.key.cmp(&b.1.key));

    Ok(Json(ExplicitProjectsResponse {
        explicit_projects: aces
            .into_iter()
            .map(|(m, p)| ExplicitProjectAce {
                project_id: p.id,
                key: p.key,
                name: p.name,
                role: m.role,
            })
            .collect(),
        hidden_count,
    }))
}
