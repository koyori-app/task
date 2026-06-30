use chrono::{DateTime, NaiveDate, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use entity::{task_timers, time_logs};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TimeLogResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub task_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub logged_minutes: i32,
    #[schema(value_type = String, format = "date")]
    pub logged_at: NaiveDate,
    #[schema(nullable)]
    pub note: Option<String>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTime<Utc>,
}

impl From<time_logs::Model> for TimeLogResponse {
    fn from(model: time_logs::Model) -> Self {
        Self {
            id: model.id,
            task_id: model.task_id,
            user_id: model.user_id,
            logged_minutes: model.logged_minutes,
            logged_at: model.logged_at,
            note: model.note,
            created_at: model.created_at.with_timezone(&Utc),
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaskTimerResponse {
    #[schema(value_type = String, format = "uuid")]
    pub task_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    #[schema(value_type = String, format = "date-time")]
    pub started_at: DateTime<Utc>,
}

impl From<task_timers::Model> for TaskTimerResponse {
    fn from(model: task_timers::Model) -> Self {
        Self {
            task_id: model.task_id,
            user_id: model.user_id,
            started_at: model.started_at.with_timezone(&Utc),
        }
    }
}

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
