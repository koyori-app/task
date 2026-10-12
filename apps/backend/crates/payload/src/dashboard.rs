use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{my_tasks::MyTaskItem, projects::ProjectResponse, task_comments::ActivityItem};

#[derive(Default, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DashboardQuery {
    /// today / week (Monday–Sunday) / overdue / all. Only unfinished tasks are listed.
    pub filter: Option<String>,
    /// IANA timezone used for today's date and completion history. Defaults to UTC.
    pub timezone: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardCounts {
    pub today: i64,
    pub week: i64,
    pub overdue: i64,
    pub open: i64,
    pub completed_week: i64,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct DashboardDay {
    pub date: String,
    pub count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardTask {
    pub task: MyTaskItem,
    #[schema(value_type = Option<String>, format = "uuid", required, nullable)]
    pub done_status_id: Option<Uuid>,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardProject {
    pub project: ProjectResponse,
    pub total: i64,
    pub completed: i64,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardActivity {
    pub activity: ActivityItem,
    pub task_title: String,
    pub task_seq_id: i32,
    pub project_key: String,
    pub project_name: String,
}

#[derive(Serialize, ToSchema)]
pub struct DashboardResponse {
    pub counts: DashboardCounts,
    pub days: Vec<DashboardDay>,
    pub tasks: Vec<DashboardTask>,
    pub total: i64,
    pub projects: Vec<DashboardProject>,
    pub activities: Vec<DashboardActivity>,
}
