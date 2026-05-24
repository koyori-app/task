use axum::{Json, extract::{Path, State}, http::StatusCode};
use axum_valid::Valid;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, QueryFilter, ColumnTrait};
use sea_orm::prelude::Uuid;
use serde::Deserialize;
use validator::Validate;

use crate::entities::{projects, tenants};
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use crate::AppState;

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateProjectRequest {
    #[validate(length(min = 1))]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[validate(length(max = 8))]
    pub icon_emoji: Option<String>,
    #[validate(url)]
    pub icon_url: Option<String>,
}

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    #[validate(length(max = 8))]
    pub icon_emoji: Option<String>,
    #[validate(url)]
    pub icon_url: Option<String>,
}

async fn require_tenant_owner(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if tenant.owner_id != user_id {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    summary = "プロジェクトを作成",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    request_body = CreateProjectRequest,
    responses(
        (status = 201, description = "作成されたプロジェクト", body = projects::Model),
        CrudErrors,
    )
)]
pub async fn create_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Valid(Json(payload)): Valid<Json<CreateProjectRequest>>,
) -> Result<(StatusCode, Json<projects::Model>), AppError> {
    require_tenant_owner(&state, tenant_id, auth.user_id).await?;
    let project = projects::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(payload.name),
        description: Set(payload.description),
        tenant_id: Set(tenant_id),
        icon_emoji: Set(payload.icon_emoji),
        icon_url: Set(payload.icon_url),
    };
    let model = project.insert(&state.db).await?;
    Ok((StatusCode::CREATED, Json(model)))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    summary = "プロジェクト一覧",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 200, description = "プロジェクト一覧", body = [projects::Model]),
        CrudErrors,
    )
)]
pub async fn list_projects(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Vec<projects::Model>>, AppError> {
    require_tenant_owner(&state, tenant_id, auth.user_id).await?;
    let list = projects::Entity::find()
        .filter(projects::Column::TenantId.eq(tenant_id))
        .all(&state.db)
        .await?;
    Ok(Json(list))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}",
    summary = "プロジェクトを取得",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, description = "プロジェクト情報", body = projects::Model),
        CrudErrors,
    )
)]
pub async fn get_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<projects::Model>, AppError> {
    require_tenant_owner(&state, tenant_id, auth.user_id).await?;
    let project = projects::Entity::find_by_id(id)
        .filter(projects::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(project))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}",
    summary = "プロジェクトを更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = UpdateProjectRequest,
    responses(
        (status = 200, description = "更新後のプロジェクト", body = projects::Model),
        CrudErrors,
    )
)]
pub async fn update_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateProjectRequest>,
) -> Result<Json<projects::Model>, AppError> {
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
    if let Some(icon_emoji) = payload.icon_emoji {
        active.icon_emoji = Set(Some(icon_emoji));
    }
    if let Some(icon_url) = payload.icon_url {
        active.icon_url = Set(Some(icon_url));
    }
    let updated = active.update(&state.db).await?;
    Ok(Json(updated))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
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
    require_tenant_owner(&state, tenant_id, auth.user_id).await?;
    projects::Entity::find_by_id(id)
        .filter(projects::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    projects::Entity::delete_by_id(id).exec(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}
