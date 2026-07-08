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
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use crate::utils::db::is_postgres_unique_violation;
use entity::{drive_folders, project_members, projects, scopes::Scope, tenants};
use payload::projects::*;

fn generate_project_key(name: &str) -> String {
    let upper: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .take(10)
        .collect();
    if upper.is_empty() {
        "PROJ".to_string()
    } else {
        upper
    }
}

fn validate_project_key(key: &str) -> bool {
    let chars: Vec<char> = key.chars().collect();
    (chars.len() >= 2 && chars.len() <= 10)
        && chars[0].is_ascii_uppercase()
        && chars[1..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() && (c.is_ascii_uppercase() || c.is_ascii_digit()))
}

const INVALID_PROJECT_KEY_MESSAGE: &str = "key は 2〜10 文字で、先頭は大文字英字、残りは大文字英字または数字で入力してください（例: ENG, BACK）";

async fn is_tenant_owner(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(tenant.owner_id == user_id)
}

async fn require_tenant_owner(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    if is_tenant_owner(state, tenant_id, user_id).await? {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn require_project_readable(
    state: &AppState,
    tenant_id: Uuid,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    if is_tenant_owner(state, tenant_id, user_id).await? {
        return Ok(());
    }
    let is_member = project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .filter(project_members::Column::UserId.eq(user_id))
        .one(&state.db)
        .await?
        .is_some();
    if is_member {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "Projects",
    summary = "プロジェクトを作成",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    request_body = CreateProjectRequest,
    responses(
        (status = 201, description = "作成されたプロジェクト", body = ProjectResponse),
        CrudErrors,
    )
)]
pub async fn create_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Valid(Json(payload)): Valid<Json<CreateProjectRequest>>,
) -> Result<(StatusCode, Json<ProjectResponse>), AppError> {
    auth.require_scope(Scope::WriteProject)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;
    require_tenant_owner(&state, tenant_id, auth.user_id).await?;
    let explicit_key = payload.key;
    let mut key = match explicit_key.as_ref() {
        Some(k) if validate_project_key(k) => k.clone(),
        Some(_) => {
            return Err(AppError::BadRequestDetail(
                INVALID_PROJECT_KEY_MESSAGE.into(),
            ));
        }
        None => {
            let generated = generate_project_key(&payload.name);
            if validate_project_key(&generated) {
                generated
            } else {
                "PROJ".to_string()
            }
        }
    };
    let txn = state.db.begin().await?;
    let model = loop {
        let project = projects::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(payload.name.clone()),
            description: Set(payload.description.clone()),
            tenant_id: Set(tenant_id),
            icon_emoji: Set(payload.icon_emoji.clone()),
            icon_url: Set(payload.icon_url.clone()),
            key: Set(key.clone()),
            is_personal: Set(false),
            personal_owner_id: Set(None),
        };
        match project.insert(&txn).await {
            Ok(model) => break model,
            Err(e) if explicit_key.is_none() && is_postgres_unique_violation(&e) => {
                let suffix = Uuid::new_v4().simple().to_string().to_ascii_uppercase();
                let suffix = &suffix[..4];
                let max_base = 10usize.saturating_sub(suffix.len()).max(2);
                key = format!("{}{}", &key[..key.len().min(max_base)], suffix);
                if !validate_project_key(&key) {
                    return Err(AppError::Conflict);
                }
            }
            Err(e) => return Err(e.into()),
        }
    };

    let drive_folder = drive_folders::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(model.name.clone()),
        parent_id: Set(None),
        tenant_id: Set(tenant_id),
        project_id: Set(Some(model.id)),
        created_by: Set(auth.user_id),
        created_at: Set(chrono::Utc::now().into()),
    };
    drive_folder.insert(&txn).await?;
    txn.commit().await?;

    Ok((StatusCode::CREATED, Json(model.into())))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Projects",
    summary = "プロジェクト一覧",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 200, description = "プロジェクト一覧", body = [ProjectResponse]),
        CrudErrors,
    )
)]
pub async fn list_projects(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<ProjectResponse>>, AppError> {
    auth.require_scope(Scope::ReadProject)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;
    if is_tenant_owner(&state, tenant_id, auth.user_id).await? {
        let list = projects::Entity::find()
            .filter(projects::Column::TenantId.eq(tenant_id))
            .filter(projects::Column::IsPersonal.eq(false))
            .all(&state.db)
            .await?;
        return Ok(Json(list.into_iter().map(Into::into).collect()));
    }

    let member_project_ids: Vec<Uuid> = project_members::Entity::find()
        .filter(project_members::Column::UserId.eq(auth.user_id))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|m| m.project_id)
        .collect();

    if member_project_ids.is_empty() {
        return Err(AppError::Forbidden);
    }

    let list = projects::Entity::find()
        .filter(projects::Column::TenantId.eq(tenant_id))
        .filter(projects::Column::IsPersonal.eq(false))
        .filter(projects::Column::Id.is_in(member_project_ids))
        .all(&state.db)
        .await?;
    Ok(Json(list.into_iter().map(Into::into).collect()))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}",
    tag = "Projects",
    summary = "プロジェクトを取得",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, description = "プロジェクト情報", body = ProjectResponse),
        CrudErrors,
    )
)]
pub async fn get_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ProjectResponse>, AppError> {
    auth.require_scope(Scope::ReadProject)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(id))
        .await?;
    require_project_readable(&state, tenant_id, id, auth.user_id).await?;
    let project = projects::Entity::find_by_id(id)
        .filter(projects::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(project.into()))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}",
    tag = "Projects",
    summary = "プロジェクトを更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = UpdateProjectRequest,
    responses(
        (status = 200, description = "更新後のプロジェクト", body = ProjectResponse),
        CrudErrors,
    )
)]
pub async fn update_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateProjectRequest>>,
) -> Result<Json<ProjectResponse>, AppError> {
    auth.require_scope(Scope::WriteProject)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(id))
        .await?;
    require_tenant_owner(&state, tenant_id, auth.user_id).await?;
    let project = projects::Entity::find_by_id(id)
        .filter(projects::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let mut active: projects::ActiveModel = project.into();
    if let Some(name) = payload.name {
        active.name = Set(name);
    }
    if let Some(description) = payload.description {
        active.description = Set(description);
    }
    if payload.clear_icon_emoji {
        active.icon_emoji = Set(None);
    } else if let Some(icon_emoji) = payload.icon_emoji {
        active.icon_emoji = Set(Some(icon_emoji));
    }
    if payload.clear_icon_url {
        active.icon_url = Set(None);
    } else if let Some(icon_url) = payload.icon_url {
        active.icon_url = Set(Some(icon_url));
    }
    let updated = active.update(&state.db).await?;
    Ok(Json(updated.into()))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Projects",
    summary = "プロジェクトを削除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn delete_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(Scope::WriteProject)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(id))
        .await?;
    require_tenant_owner(&state, tenant_id, auth.user_id).await?;
    projects::Entity::find_by_id(id)
        .filter(projects::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    projects::Entity::delete_by_id(id).exec(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}
