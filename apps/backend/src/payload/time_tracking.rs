use chrono::{NaiveDate, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Validate, Deserialize, ToSchema)]
pub struct CreateTimeLogRequest {
    #[validate(range(min = 1))]
    pub logged_minutes: i32,
    #[schema(value_type = String, format = "date")]
    pub logged_at: NaiveDate,
    pub note: Option<String>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateTimeLogRequest {
    #[validate(range(min = 1))]
    pub logged_minutes: Option<i32>,
    #[schema(value_type = Option<String>, format = "date")]
    pub logged_at: Option<NaiveDate>,
    pub note: Option<String>,
    #[serde(default)]
    pub clear_note: bool,
}

#[derive(Serialize, ToSchema)]
pub struct TimeLogSummaryResponse {
    pub estimated_minutes: Option<i32>,
    pub actual_minutes: i32,
    pub remaining_minutes: Option<i32>,
    pub is_over: bool,
    pub by_user: Vec<UserTimeSummary>,
}

#[derive(Serialize, ToSchema)]
pub struct UserTimeSummary {
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub name: String,
    pub minutes: i32,
}

#[derive(Serialize, ToSchema)]
pub struct TimerStatusResponse {
    pub is_running: bool,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub started_at: Option<chrono::DateTime<Utc>>,
    pub elapsed_minutes: Option<i32>,
}
