//! プロジェクトのドキュメント本文検索の DTO（docs/dev/features/drive/files.md の「本文の検索」）。

use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, Serialize, ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchDocumentsQuery {
    /// 検索語。空白で語に分け、全部の語を含む節に当たる（大文字小文字を無視した部分一致）
    pub q: String,
    #[serde(default = "default_search_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_search_limit() -> u64 {
    20
}

/// 当たった節 1 つ。1 ファイルから複数の節が返る
#[derive(Serialize, Deserialize, ToSchema)]
pub struct DocumentSearchHit {
    #[schema(value_type = String, format = "uuid")]
    pub file_id: Uuid,
    pub file_name: String,
    #[schema(value_type = String, format = "uuid", nullable)]
    pub folder_id: Option<Uuid>,
    /// 祖先から自分までの見出し。最初の見出しより前の節は空
    pub heading_path: Vec<String>,
    /// ファイルの中の節の順（0 始まり）
    pub position: i32,
    /// 語の前後を切り出した素の文字列（HTML ではない）。切った側に `…` が付く
    pub excerpt: String,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct DocumentSearchResponse {
    pub hits: Vec<DocumentSearchHit>,
    /// 当たった節の数（`limit` / `offset` を掛ける前）
    pub total: u64,
}
