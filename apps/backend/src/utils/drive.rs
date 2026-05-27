//! Drive クォータ・設定ヘルパー。

use std::env;

use sea_orm::{ColumnTrait, EntityTrait, ExprTrait, QueryFilter, QuerySelect};
use sea_orm::prelude::Uuid;

use crate::entities::{drive_files, tenants};
use crate::error::AppError;
use crate::AppState;

/// Drive 関連の環境変数設定。
#[derive(Clone, Debug)]
pub struct DriveConfig {
    pub upload_max_bytes: u64,
    /// `0` = 無制限（デフォルトクォータ未設定時）
    pub default_quota_bytes: i64,
    /// `0` = 天井なし
    pub system_max_quota_bytes: i64,
}

impl DriveConfig {
    pub fn from_env() -> Self {
        let upload_max_mb = env_u64("UPLOAD_MAX_SIZE_MB", 100);
        let default_quota_mb = env_i64("DRIVE_DEFAULT_QUOTA_MB", 10240);
        let system_max_mb = env_i64("DRIVE_SYSTEM_MAX_QUOTA_MB", 51200);

        Self {
            upload_max_bytes: upload_max_mb.saturating_mul(1024 * 1024),
            default_quota_bytes: mb_to_bytes(default_quota_mb),
            system_max_quota_bytes: mb_to_bytes(system_max_mb),
        }
    }

    pub fn system_max_bytes_opt(&self) -> Option<i64> {
        if self.system_max_quota_bytes == 0 {
            None
        } else {
            Some(self.system_max_quota_bytes)
        }
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_i64(key: &str, default: i64) -> i64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn mb_to_bytes(mb: i64) -> i64 {
    if mb == 0 {
        0
    } else {
        mb.saturating_mul(1024 * 1024)
    }
}

/// テナントの有効クォータ（バイト）。`None` = 無制限。
pub fn effective_quota(tenant: &tenants::Model, config: &DriveConfig) -> Option<i64> {
    match tenant.drive_quota_bytes {
        Some(q) => Some(q),
        None => {
            if config.default_quota_bytes == 0 {
                None
            } else {
                Some(config.default_quota_bytes)
            }
        }
    }
}

pub fn content_url(file_id: Uuid) -> String {
    format!("/v1/drive/files/{file_id}/content")
}

pub async fn tenant_used_bytes(
    db: &sea_orm::DatabaseConnection,
    tenant_id: Uuid,
) -> Result<i64, AppError> {
    let sum = drive_files::Entity::find()
        .filter(drive_files::Column::TenantId.eq(tenant_id))
        .select_only()
        .column_as(
            sea_orm::sea_query::Expr::col(drive_files::Column::Size).sum(),
            "total",
        )
        .into_tuple::<Option<i64>>()
        .one(db)
        .await?;

    Ok(sum.flatten().unwrap_or(0))
}

pub fn current_storage_type() -> crate::entities::drive_files::StorageType {
    match env::var("STORAGE_BACKEND")
        .unwrap_or_else(|_| "local".into())
        .as_str()
    {
        "s3" => crate::entities::drive_files::StorageType::S3,
        _ => crate::entities::drive_files::StorageType::Local,
    }
}

/// テナントオーナー判定（drive_files / drive_folders の共通ヘルパー）。
pub async fn is_tenant_owner(
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

pub fn guess_mime(filename: &str) -> String {
    mime_guess::from_path(filename)
        .first_raw()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/octet-stream".into())
}
