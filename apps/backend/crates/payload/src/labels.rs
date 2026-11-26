use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use entity::labels;

#[derive(Debug, Clone, Serialize, ToSchema, serde::Deserialize)]
pub struct LabelResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub color: String,
    #[schema(nullable)]
    pub icon_url: Option<String>,
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub project_id: Option<Uuid>,
    /// 色の丸の代わりに出す絵文字。無ければ画面は色の丸を出す
    #[schema(nullable)]
    pub icon_emoji: Option<String>,
}

impl From<labels::Model> for LabelResponse {
    fn from(model: labels::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            description: model.description,
            color: model.color,
            icon_url: model.icon_url,
            project_id: model.project_id,
            icon_emoji: model.icon_emoji,
        }
    }
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateLabelRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[validate(regex(path = "common::validation::COLOR_REGEX"))]
    pub color: String,
    pub icon_url: Option<String>,
    #[validate(custom(function = "crate::reactions::validate_emoji"))]
    pub icon_emoji: Option<String>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateLabelRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    pub description: Option<String>,
    #[validate(regex(path = "common::validation::COLOR_REGEX"))]
    pub color: Option<String>,
    pub icon_url: Option<String>,
    #[serde(default)]
    pub clear_icon_url: bool,
    #[validate(custom(function = "crate::reactions::validate_emoji"))]
    pub icon_emoji: Option<String>,
    /// 絵文字を外して色の丸に戻す。`icon_emoji` より優先する
    #[serde(default)]
    pub clear_icon_emoji: bool,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct LabelExport {
    pub version: u32,
    pub labels: Vec<LabelExportItem>,
}

#[derive(Validate, Serialize, Deserialize, ToSchema)]
pub struct LabelExportItem {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(regex(path = "common::validation::COLOR_REGEX"))]
    pub color: String,
    pub description: String,
    /// 欄が無ければ（絵文字を足す前に書き出したファイル）、`overwrite` でも今の絵文字を残す。
    /// `null` なら絵文字を外す
    #[serde(default, deserialize_with = "crate::nullable::deserialize_some")]
    #[schema(value_type = Option<String>, nullable)]
    #[validate(custom(function = "crate::reactions::validate_emoji"))]
    pub icon_emoji: Option<Option<String>>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct ImportLabelRequest {
    pub version: u32,
    #[validate(length(max = 500))]
    pub labels: Vec<LabelExportItem>,
    #[serde(default)]
    pub on_conflict: ImportConflict,
}

#[derive(Deserialize, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImportConflict {
    #[default]
    Skip,
    Overwrite,
}
