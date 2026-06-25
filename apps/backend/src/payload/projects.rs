use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    #[validate(length(min = 1))]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[validate(length(max = 8))]
    pub icon_emoji: Option<String>,
    #[validate(url)]
    pub icon_url: Option<String>,
    /// プロジェクトキー（例: ENG, BACK）。省略時はプロジェクト名から自動生成。
    pub key: Option<String>,
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct UpdateProjectRequest {
    #[validate(length(min = 1))]
    pub name: Option<String>,
    pub description: Option<String>,
    #[validate(length(max = 8))]
    pub icon_emoji: Option<String>,
    #[validate(url)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub clear_icon_emoji: bool,
    #[serde(default)]
    pub clear_icon_url: bool,
}
