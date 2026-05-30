use axum::{
    Json,
    extract::State,
    http::HeaderMap,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::entities::system_settings;
use crate::error::AppError;
use crate::extractors::AdminUser;
use crate::handlers::admin_audit::record_audit;
use crate::openapi::SessionAuthErrors;
use crate::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct SystemSettingsResponse {
    pub user_registration_enabled: bool,
    pub drive_default_quota_mb: i64,
    pub drive_system_max_quota_mb: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateSystemSettingsRequest {
    pub user_registration_enabled: Option<bool>,
    pub drive_default_quota_mb: Option<i64>,
    pub drive_system_max_quota_mb: Option<i64>,
}

fn model_to_response(model: system_settings::Model) -> SystemSettingsResponse {
    SystemSettingsResponse {
        user_registration_enabled: model.user_registration_enabled,
        drive_default_quota_mb: model.drive_default_quota_mb,
        drive_system_max_quota_mb: model.drive_system_max_quota_mb,
    }
}

async fn load_singleton(db: &sea_orm::DatabaseConnection) -> Result<system_settings::Model, AppError> {
    system_settings::Entity::find()
        .filter(system_settings::Column::Singleton.eq(true))
        .one(db)
        .await?
        .ok_or(AppError::NotFound)
}

fn validate_quota_pair(default_mb: i64, system_max_mb: i64) -> Result<(), AppError> {
    if default_mb < 0 || system_max_mb < 0 {
        return Err(AppError::BadRequest);
    }
    if system_max_mb > 0 && default_mb > system_max_mb {
        return Err(AppError::BadRequest);
    }
    Ok(())
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Admin System Settings",
    summary = "システム設定を取得",
    responses(
        (status = 200, description = "システム設定", body = SystemSettingsResponse),
        SessionAuthErrors,
    )
)]
pub async fn get_system_settings(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<SystemSettingsResponse>, AppError> {
    let settings = load_singleton(&state.db).await?;
    Ok(Json(model_to_response(settings)))
}

#[axum::debug_handler]
#[utoipa::path(
    patch,
    path = "/",
    tag = "Admin System Settings",
    summary = "システム設定を更新",
    request_body = UpdateSystemSettingsRequest,
    responses(
        (status = 200, description = "更新後のシステム設定", body = SystemSettingsResponse),
        SessionAuthErrors,
    )
)]
pub async fn update_system_settings(
    admin: AdminUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateSystemSettingsRequest>,
) -> Result<Json<SystemSettingsResponse>, AppError> {
    let before = load_singleton(&state.db).await?;
    let mut active = before.clone().into_active_model();
    let mut changed_fields: Vec<&str> = Vec::new();

    let mut next_default = before.drive_default_quota_mb;
    let mut next_system_max = before.drive_system_max_quota_mb;

    if let Some(v) = payload.user_registration_enabled {
        if v != before.user_registration_enabled {
            changed_fields.push("user_registration_enabled");
        }
        active.user_registration_enabled = Set(v);
    }
    if let Some(v) = payload.drive_default_quota_mb {
        if v != before.drive_default_quota_mb {
            changed_fields.push("drive_default_quota_mb");
        }
        next_default = v;
        active.drive_default_quota_mb = Set(v);
    }
    if let Some(v) = payload.drive_system_max_quota_mb {
        if v != before.drive_system_max_quota_mb {
            changed_fields.push("drive_system_max_quota_mb");
        }
        next_system_max = v;
        active.drive_system_max_quota_mb = Set(v);
    }

    if changed_fields.is_empty() {
        return Ok(Json(model_to_response(before)));
    }

    validate_quota_pair(next_default, next_system_max)?;
    active.updated_at = Set(Utc::now());

    let updated = active.update(&state.db).await?;

    let metadata = serde_json::json!({
        "changed_fields": changed_fields,
        "before": {
            "user_registration_enabled": before.user_registration_enabled,
            "drive_default_quota_mb": before.drive_default_quota_mb,
            "drive_system_max_quota_mb": before.drive_system_max_quota_mb,
        },
        "after": {
            "user_registration_enabled": updated.user_registration_enabled,
            "drive_default_quota_mb": updated.drive_default_quota_mb,
            "drive_system_max_quota_mb": updated.drive_system_max_quota_mb,
        },
    });

    record_audit(
        &state.db,
        admin.user_id,
        "system.settings.update",
        "system",
        "settings",
        None,
        Some(metadata),
        &headers,
    )
    .await?;

    Ok(Json(model_to_response(updated)))
}
