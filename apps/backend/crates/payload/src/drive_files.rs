use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;

use entity::drive_files;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

pub const DEFAULT_LIST_LIMIT: u32 = 50;

/// ファイル本文取得エンドポイントの相対 URL。
/// レスポンス DTO（[`DriveFileResponse::from`]）が埋め込むためここに置く。
pub fn content_url(file_id: Uuid) -> String {
    format!("/v1/drive/files/{file_id}/content")
}

/// ファイル名の部分一致検索（`q`）の最大文字数。
pub const MAX_NAME_QUERY_LEN: u64 = 200;

#[derive(Debug, Deserialize, Validate, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListFilesQuery {
    /// 省略時はドライブ直下（フォルダ無し）のファイル
    pub folder_id: Option<Uuid>,
    /// MIME タイプの前方一致。`text/markdown` は完全一致として効き、`text/` は text 全体
    pub mime_type: Option<String>,
    /// ファイル名の部分一致（大文字小文字を区別しない）。200 文字まで
    #[validate(length(max = MAX_NAME_QUERY_LEN))]
    pub q: Option<String>,
    #[serde(default)]
    #[param(inline)]
    pub sort: DriveFileSort,
    #[param(minimum = 1, maximum = 200)]
    #[serde(default = "default_list_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_list_limit() -> u32 {
    DEFAULT_LIST_LIMIT
}

/// ファイル一覧の並び順。既定は作成日時の新しい順。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DriveFileSort {
    #[default]
    CreatedAtDesc,
    NameAsc,
    UpdatedAtDesc,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DriveFileResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub size: i64,
    pub mime_type: String,
    pub url: String,
    #[schema(value_type = String, format = "uuid", nullable)]
    pub folder_id: Option<Uuid>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTime<Utc>,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: DateTime<Utc>,
}

impl From<drive_files::Model> for DriveFileResponse {
    fn from(model: drive_files::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            size: model.size,
            mime_type: model.mime_type,
            url: content_url(model.id),
            folder_id: model.folder_id,
            created_at: model.created_at.with_timezone(&Utc),
            updated_at: model.updated_at.with_timezone(&Utc),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListFilesResponse {
    pub files: Vec<DriveFileResponse>,
    pub total: u64,
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct UpdateFileRequest {
    #[validate(length(min = 1))]
    pub name: Option<String>,
    /// `null` でルート（フォルダなし）へ移動。フィールド省略時は変更なし。
    #[serde(default, deserialize_with = "crate::nullable::deserialize_some")]
    #[schema(value_type = String, format = "uuid", nullable)]
    pub folder_id: Option<Option<Uuid>>,
}

/// ドライブファイルの本文差し替えリクエスト。
///
/// 空文字列は「中身を空にする編集」として許可する（アップロードが 0 バイトを
/// 拒否するのとは意図的に扱いを変えている）。
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateFileContentRequest {
    pub content: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DriveUsageResponse {
    pub used_bytes: i64,
    #[schema(nullable)]
    pub quota_bytes: Option<i64>,
    #[schema(nullable)]
    pub system_max_bytes: Option<i64>,
    pub unlimited: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateQuotaRequest {
    pub quota_bytes: Option<i64>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ContentQuery {
    pub token: Option<String>,
}
