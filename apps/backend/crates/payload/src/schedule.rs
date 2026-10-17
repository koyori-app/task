//! テナントのスケジュール（期間のタスク・スプリント・マイルストーン）。

use chrono::{DateTime, NaiveDate, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    milestones::MilestoneResponse,
    my_tasks::{MyTaskProjectInfo, MyTaskStatusInfo},
    sprints::SprintResponse,
    users::UserSummary,
};
use entity::tasks;

#[derive(Debug, Deserialize, Serialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ScheduleQuery {
    /// 期間の開始日（含む）
    #[param(value_type = String, format = "date")]
    pub from: NaiveDate,
    /// 期間の終了日（含む）。`to - from` は 92 日まで
    #[param(value_type = String, format = "date")]
    pub to: NaiveDate,
    /// 締切を日付に落とす IANA タイムゾーン名。既定 `UTC`
    pub timezone: Option<String>,
    /// プロジェクトで絞る（繰り返し指定可）。見えないプロジェクトは無視する
    #[serde(default)]
    #[param(value_type = Option<Vec<String>>, explode)]
    pub project_id: Vec<Uuid>,
    /// 担当者で絞る（繰り返し指定可）
    #[serde(default)]
    #[param(value_type = Option<Vec<String>>, explode)]
    pub user_id: Vec<Uuid>,
    /// 完了ステータスのタスクを含めるか。既定 true
    pub include_done: Option<bool>,
    /// 前ページの `next_cursor`
    pub cursor: Option<String>,
    /// タスクの件数。既定 200、上限 500
    pub limit: Option<u64>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ScheduleRange {
    #[schema(value_type = String, format = "date")]
    pub from: NaiveDate,
    #[schema(value_type = String, format = "date")]
    pub to: NaiveDate,
    pub timezone: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ScheduleAssignee {
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ScheduleTask {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub seq_id: i32,
    pub seq_key: String,
    pub title: String,
    pub project: MyTaskProjectInfo,
    pub status: MyTaskStatusInfo,
    pub priority: tasks::TaskPriority,
    pub progress_pct: i16,
    #[schema(value_type = Option<String>, format = "date", nullable)]
    pub planned_start_on: Option<NaiveDate>,
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub soft_deadline: Option<DateTime<Utc>>,
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub hard_deadline: Option<DateTime<Utc>>,
    #[schema(nullable)]
    pub estimated_minutes: Option<i32>,
    pub assignees: Vec<ScheduleAssignee>,
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub sprint_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub milestone_id: Option<Uuid>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ScheduleResponse {
    pub range: ScheduleRange,
    pub tasks: Vec<ScheduleTask>,
    /// 期間に重なるスプリント（ページングしない）
    pub sprints: Vec<SprintResponse>,
    /// 期日が期間内のマイルストーン（ページングしない）
    pub milestones: Vec<MilestoneResponse>,
    /// このページの `tasks` の担当者
    pub members: Vec<UserSummary>,
    #[schema(nullable)]
    pub next_cursor: Option<String>,
}
