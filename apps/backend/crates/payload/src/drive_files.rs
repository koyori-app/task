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

/// ファイル本文の ETag（ダブルクォート込み）。応答の `etag` と `ETag` ヘッダーの両方に使う。
///
/// `content_hash` を持たない既存行は `updated_at`（Unix ミリ秒）とサイズで代替する。
/// 本文更新で `updated_at` が変わるので、`If-Match` の楽観ロックにはこれで足りる。
pub fn file_etag(model: &drive_files::Model) -> String {
    match &model.content_hash {
        Some(hash) => format!("\"sha256:{hash}\""),
        None => format!(
            "\"ts:{}-{}\"",
            model.updated_at.timestamp_millis(),
            model.size
        ),
    }
}

/// `If-Match` / `If-None-Match` の 1 行が現在の ETag に一致するか。
///
/// `*` は常に一致。カンマ区切りの複数指定はどれか 1 つの完全一致で一致とする。
/// 弱い ETag（`W/"..."`）は完全一致しないので一致しない。
pub fn etag_matches(header_value: &str, current: &str) -> bool {
    let value = header_value.trim();
    value == "*" || value.split(',').any(|tag| tag.trim() == current)
}

/// ファイル名の部分一致検索（`q`）の最大文字数。
pub const MAX_NAME_QUERY_LEN: u64 = 200;

#[derive(Debug, Deserialize, Validate, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListFilesQuery {
    /// 省略時はドライブ直下（フォルダ無し）のファイル
    pub folder_id: Option<Uuid>,
    /// MIME タイプの前方一致。`text/markdown` は `text/markdown` で始まる MIME（`text/markdown; charset=…` など）にも一致し、`text/` は text 全体
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
    /// 本文の ETag（ダブルクォート込み）。`PUT .../content` の `If-Match` にそのまま渡す
    #[schema(
        example = "\"sha256:2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae\""
    )]
    pub etag: String,
}

impl From<drive_files::Model> for DriveFileResponse {
    fn from(model: drive_files::Model) -> Self {
        let etag = file_etag(&model);
        Self {
            id: model.id,
            name: model.name,
            size: model.size,
            mime_type: model.mime_type,
            url: content_url(model.id),
            folder_id: model.folder_id,
            created_at: model.created_at.with_timezone(&Utc),
            updated_at: model.updated_at.with_timezone(&Utc),
            etag,
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

#[cfg(test)]
mod tests {
    use super::etag_matches;

    const CURRENT: &str = "\"sha256:abc\"";

    #[test]
    fn exact_match_only() {
        assert!(etag_matches("\"sha256:abc\"", CURRENT));
        assert!(!etag_matches("\"sha256:abd\"", CURRENT));
        // クォート無しは別の値として扱う
        assert!(!etag_matches("sha256:abc", CURRENT));
    }

    #[test]
    fn wildcard_matches_any() {
        assert!(etag_matches("*", CURRENT));
        assert!(etag_matches("  *  ", CURRENT));
    }

    #[test]
    fn list_with_whitespace() {
        assert!(etag_matches("\"x\", \"sha256:abc\"", CURRENT));
        assert!(etag_matches("\"x\",\"sha256:abc\" ", CURRENT));
        assert!(etag_matches("  \"sha256:abc\"  ,\"y\"", CURRENT));
        assert!(!etag_matches("\"x\", \"y\"", CURRENT));
        assert!(!etag_matches("", CURRENT));
    }

    #[test]
    fn weak_etag_is_rejected() {
        assert!(!etag_matches("W/\"sha256:abc\"", CURRENT));
        assert!(!etag_matches("\"x\", W/\"sha256:abc\"", CURRENT));
    }
}
