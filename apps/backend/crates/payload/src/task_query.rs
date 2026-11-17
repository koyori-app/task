use crate::or_unknown::OrUnknown;
use crate::users::UserSummary;
use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const INVALID_TASK_QUERY: &str = "invalid-task-query";
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskQueryErrorResponse {
    pub message: String,
    pub errors: Vec<TaskQueryError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskQueryError {
    #[schema(inline)]
    pub code: OrUnknown<TaskQueryErrorCode>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub term: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TaskQueryErrorCode {
    UnknownValue,
    InvalidValue,
    EmptyValue,
    NotNegatable,
    DuplicateSort,
    ConflictingSort,
    TooLong,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskSearchMatch {
    pub title: Option<Vec<MatchSegment>>,
    pub snippets: Vec<MatchSnippet>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MatchSegment {
    pub text: String,
    pub hit: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MatchSource {
    Description,
    Comment,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MatchSnippet {
    #[schema(inline)]
    pub source: OrUnknown<MatchSource>,
    pub segments: Vec<MatchSegment>,
    pub truncated_start: bool,
    pub truncated_end: bool,
    pub comment: Option<MatchComment>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MatchComment {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub author: Option<UserSummary>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskFacetsResponse {
    pub statuses: Vec<FacetCount>,
    pub labels: Vec<FacetCount>,
    pub assignees: Vec<FacetCount>,
    pub authors: Vec<FacetCount>,
    pub priorities: Vec<PriorityFacetCount>,
    pub unassigned: u64,
    pub unlabeled: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FacetCount {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub count: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PriorityFacetCount {
    #[schema(inline)]
    pub priority: OrUnknown<entity::tasks::TaskPriority>,
    pub count: u64,
}
