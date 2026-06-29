//! Personal tokens entity — schema-first with hand-written JSONB helpers.
use sea_orm::entity::prelude::*;

/// JSONB `allowed_project_ids` を `Vec<Uuid>` に復元する。
/// NULL は「制限なし」を意味する。parse 失敗は DB 破損扱いで `Err` を返す。
pub fn parse_allowed_project_ids(
    value: &serde_json::Value,
) -> Result<Option<Vec<Uuid>>, serde_json::Error> {
    if value.is_null() {
        return Ok(None);
    }
    serde_json::from_value(value.clone()).map(Some)
}

pub use super::_generated::personal_tokens::*;
