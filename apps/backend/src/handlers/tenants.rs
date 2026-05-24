use axum::{Json, extract::{Path, State}, http::StatusCode};
use axum_valid::Valid;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, QueryFilter, ColumnTrait};
use sea_orm::prelude::Uuid;
use serde::Deserialize;
use validator::Validate;

use crate::entities::tenants;
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use crate::AppState;

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateTenantRequest {
    #[validate(length(min = 1))]
    pub display_id: String,
    #[validate(length(min = 1))]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon_url: String,
}

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateTenantRequest {
    #[validate(length(min = 1))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    summary = "テナントを作成",
    request_body = CreateTenantRequest,
    responses(
        (status = 201, description = "作成されたテナント", body = tenants::Model),
        CrudErrors,
    )
)]
pub async fn create_tenant(
    State(state): State<AppState>,
    auth: AuthUser,
    Valid(Json(payload)): Valid<Json<CreateTenantRequest>>,
) -> Result<(StatusCode, Json<tenants::Model>), AppError> {
    let id = Uuid::new_v4();
    let tenant = tenants::ActiveModel {
        id: Set(id),
        display_id: Set(payload.display_id),
        name: Set(payload.name),
        description: Set(payload.description),
        icon_url: Set(payload.icon_url),
        owner_id: Set(auth.user_id),
    };
    let model = tenant.insert(&state.db).await?;
    Ok((StatusCode::CREATED, Json(model)))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    summary = "自分のテナント一覧",
    responses(
        (status = 200, description = "テナント一覧", body = [tenants::Model]),
        CrudErrors,
    )
)]
pub async fn list_tenants(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<tenants::Model>>, AppError> {
    let tenants = tenants::Entity::find()
        .filter(tenants::Column::OwnerId.eq(auth.user_id))
        .all(&state.db)
        .await?;
    Ok(Json(tenants))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}",
    summary = "テナントを取得",
    params(("id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 200, description = "テナント情報", body = tenants::Model),
        CrudErrors,
    )
)]
pub async fn get_tenant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<tenants::Model>, AppError> {
    let tenant = tenants::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if tenant.owner_id != auth.user_id {
        return Err(AppError::Forbidden);
    }
    Ok(Json(tenant))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}",
    summary = "テナントを更新",
    params(("id" = Uuid, Path, description = "テナントID")),
    request_body = UpdateTenantRequest,
    responses(
        (status = 200, description = "更新後のテナント", body = tenants::Model),
        CrudErrors,
    )
)]
pub async fn update_tenant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Valid(Json(payload)): Valid<Json<UpdateTenantRequest>>,
) -> Result<Json<tenants::Model>, AppError> {
    let tenant = tenants::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if tenant.owner_id != auth.user_id {
        return Err(AppError::Forbidden);
    }

    let mut active: tenants::ActiveModel = tenant.into();
    if let Some(name) = payload.name {
        active.name = Set(name);
    }
    if let Some(description) = payload.description {
        active.description = Set(description);
    }
    if let Some(icon_url) = payload.icon_url {
        active.icon_url = Set(icon_url);
    }
    let updated = active.update(&state.db).await?;
    Ok(Json(updated))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    summary = "テナントを削除",
    params(("id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 204, description = "削除しました"),
        CrudErrors,
    )
)]
pub async fn delete_tenant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let tenant = tenants::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if tenant.owner_id != auth.user_id {
        return Err(AppError::Forbidden);
    }
    tenants::Entity::delete_by_id(id).exec(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}
