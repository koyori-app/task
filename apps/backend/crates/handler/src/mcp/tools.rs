//! MCP のツール。どれも REST API を内向きに呼び直し、応答をそのまま返す。
//!
//! 引数の `///` はツールの説明としてクライアントへ渡るので英語で書く
//! （CLI のヘルプ `crates/cli/src/cli.rs` の文面に合わせる）。

use axum::{Router, http::request::Parts};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use payload::forge::LinkTaskPullRequestRequest;
use payload::tasks::{AssigneeInput, CreateTaskRequest, UpdateTaskRequest};
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::{common::Extension, router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::inner::{InnerApi, ToolFailure, ok};
use super::resolve;

mod notifications;
mod sprints;
mod task_ops;

const INSTRUCTIONS: &str = "Tools for the task tracker. Refer to a task as KEY-N (project key and number, e.g. TASK-42) or by UUID together with `project`. \
`tenant` is only needed when the personal access token is bound to more than one tenant; `whoami` shows the tenants and scopes of the token. \
The notification list and read tools and `list_tenants` take no `tenant`. \
Labels, assignees, task types, milestones and sprints are passed by name or UUID; a name that is not found or is ambiguous returns the candidates. Looking up a milestone or sprint by name needs the read:milestone or read:sprint scope. \
Referring to a project by key or to a task as KEY-N needs the `read:project` scope; without it, pass UUIDs. \
Review tools take the pull request number as `pr`; `repo` is only needed to read rounds recorded for another repository. \
A failed call returns the HTTP status and message of the API, e.g. `403 forbidden` or `404 not-found`.";

/// 担当者の役割。CLI（`crates/cli/src/commands/tasks.rs` の `ASSIGNEE_ROLE`）と同じ値
const ASSIGNEE_ROLE: &str = "assignee";

#[derive(Clone)]
pub(super) struct TaskMcp {
    api: Router,
}

impl TaskMcp {
    pub(super) fn new(api: Router) -> Self {
        Self { api }
    }

    /// ツールはモジュールごとのルーターに分け、ここで足し合わせる。和を `#[tool_handler]` の
    /// 属性に直接書くと、マクロが `式.call(..)` と展開して最後の項にだけ掛かる
    fn all_tools() -> ToolRouter<Self> {
        Self::tool_router()
            + Self::review_router()
            + Self::task_ops_router()
            + Self::sprints_router()
            + Self::notifications_router()
    }
}

#[tool_handler(router = Self::all_tools())]
impl ServerHandler for TaskMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("task", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// 失敗も `isError: true` の結果として返す。プロトコルのエラー（`Err`）にすると、
/// クライアントが文面を利用者に見せない
pub(super) fn finish(result: Result<Value, ToolFailure>) -> Result<CallToolResult, ErrorData> {
    Ok(match result {
        Ok(value) => ok(value),
        Err(failure) => failure.into(),
    })
}

pub(super) fn parse_uuid(field: &str, value: &str) -> Result<Uuid, ToolFailure> {
    Uuid::parse_str(value).map_err(|_| ToolFailure::new(format!("{field} must be a UUID")))
}

fn parse_optional_uuid(field: &str, value: Option<&str>) -> Result<Option<Uuid>, ToolFailure> {
    value.map(|value| parse_uuid(field, value)).transpose()
}

/// 期限。RFC 3339 か、その日の始まり（UTC）を指す `YYYY-MM-DD`（CLI と同じ）
fn parse_deadline(field: &str, value: &str) -> Result<DateTime<Utc>, ToolFailure> {
    if let Ok(instant) = DateTime::parse_from_rfc3339(value) {
        return Ok(instant.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| date.and_time(NaiveTime::MIN).and_utc())
        .map_err(|_| ToolFailure::new(format!("{field} must be RFC 3339 or YYYY-MM-DD")))
}

fn parse_date(field: &str, value: &str) -> Result<NaiveDate, ToolFailure> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| ToolFailure::new(format!("{field} must be YYYY-MM-DD")))
}

pub(super) fn to_body<T: serde::Serialize>(request: &T) -> Result<Value, ToolFailure> {
    serde_json::to_value(request)
        .map_err(|error| ToolFailure::new(format!("Could not build the request body: {error}")))
}

/// `priority` は文字列のまま渡し、検証は API に任せる
fn set_priority(body: &mut Value, priority: Option<String>) {
    if let Some(priority) = priority {
        body["priority"] = Value::String(priority);
    }
}

/// 任意の引数の空文字（空白だけを含む）を「省略」と読む。
///
/// 使わない引数に `""` を入れてくるクライアントがある。名前として解決すると、
/// 束縛が 1 件の PAT でも `tenant: ""` が「テナントが見つからない」になる。
/// 本文（`title`・`description`・`body`）には付けない
pub(super) fn blank_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.filter(|value| !value.trim().is_empty()))
}

#[derive(Deserialize, JsonSchema)]
struct TenantArgs {
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ProjectOptionKind {
    Statuses,
    Labels,
    TaskTypes,
    Milestones,
    Sprints,
    AssignableUsers,
}

impl ProjectOptionKind {
    /// パスの集合名（`crates/handler/src/routes/tenants.rs`）
    fn collection(self) -> &'static str {
        match self {
            Self::Statuses => "statuses",
            Self::Labels => "labels",
            Self::TaskTypes => "task-types",
            Self::Milestones => "milestones",
            Self::Sprints => "sprints",
            Self::AssignableUsers => "assignable-users",
        }
    }
}

#[derive(Deserialize, JsonSchema)]
struct ListProjectOptionsArgs {
    /// Project key or UUID
    project: String,
    /// Which list to return: statuses, labels, task_types, milestones, sprints or assignable_users
    kind: ProjectOptionKind,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct ListTasksArgs {
    /// Project key or UUID
    project: String,
    /// Filter expression of the task list (same syntax as the filter box of the list screen). Omit for no filter
    #[serde(default, deserialize_with = "blank_as_none")]
    q: Option<String>,
    /// List archived tasks instead of active ones
    is_archived: Option<bool>,
    /// created_at_desc (default), or title / assignee / priority / deadline followed by _asc or _desc
    #[serde(default, deserialize_with = "blank_as_none")]
    sort: Option<String>,
    /// Tasks per page (1-200)
    limit: Option<u64>,
    /// Number of tasks to skip
    offset: Option<u64>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct SearchTasksArgs {
    /// Project key or UUID
    project: String,
    /// Search query
    q: String,
    /// Hits per page (1-100)
    limit: Option<u64>,
    /// Number of hits to skip
    offset: Option<u64>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct TaskArgs {
    /// Task ref (KEY-N or UUID)
    task: String,
    /// Project key or UUID. Required when `task` is a UUID; ignored when `task` is KEY-N
    #[serde(default, deserialize_with = "blank_as_none")]
    project: Option<String>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct CreateTaskArgs {
    /// Project key or UUID
    project: String,
    // 項目は一括作成の要素と共有する。flatten なので入力スキーマは平らなまま
    #[serde(flatten)]
    task: NewTaskArgs,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

// 作るタスク 1 件の項目（`create_task` と `bulk_create_tasks` の要素）。
#[derive(Deserialize, JsonSchema)]
struct NewTaskArgs {
    /// Task title
    title: String,
    /// Task description (Markdown)
    description: Option<String>,
    /// Status name or UUID (default: the project's default status)
    #[serde(default, deserialize_with = "blank_as_none")]
    status: Option<String>,
    /// CriticalFire, Critical, High, Medium, Low or Trivial
    #[serde(default, deserialize_with = "blank_as_none")]
    priority: Option<String>,
    /// Parent task UUID. A new task takes the parent's deadlines and planned start unless set here
    #[serde(default, deserialize_with = "blank_as_none")]
    parent_task_id: Option<String>,
    /// Label names or UUIDs
    #[serde(alias = "label_ids")]
    labels: Option<Vec<String>>,
    /// Assignee usernames or user UUIDs
    #[serde(alias = "assignee_ids")]
    assignees: Option<Vec<String>>,
    /// Task type name or UUID
    #[serde(default, alias = "task_type_id", deserialize_with = "blank_as_none")]
    task_type: Option<String>,
    /// Milestone name or UUID
    #[serde(default, alias = "milestone_id", deserialize_with = "blank_as_none")]
    milestone: Option<String>,
    /// Sprint name or UUID
    #[serde(default, alias = "sprint_id", deserialize_with = "blank_as_none")]
    sprint: Option<String>,
    /// Soft deadline (RFC 3339, or YYYY-MM-DD for the start of that day in UTC)
    #[serde(default, deserialize_with = "blank_as_none")]
    soft_deadline: Option<String>,
    /// Hard deadline (RFC 3339, or YYYY-MM-DD for the start of that day in UTC)
    #[serde(default, deserialize_with = "blank_as_none")]
    hard_deadline: Option<String>,
    /// Planned start date (YYYY-MM-DD)
    #[serde(default, deserialize_with = "blank_as_none")]
    planned_start_on: Option<String>,
    /// Estimate in minutes (1 or more)
    estimated_minutes: Option<i32>,
}

/// 形を確かめた `NewTaskArgs`。状態と名前は文字列のまま持ち、API を呼ぶときに引く
struct ParsedNewTask {
    title: String,
    description: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    parent_task_id: Option<Uuid>,
    labels: Vec<String>,
    assignees: Vec<String>,
    task_type: Option<String>,
    milestone: Option<String>,
    sprint: Option<String>,
    soft_deadline: Option<DateTime<Utc>>,
    hard_deadline: Option<DateTime<Utc>>,
    planned_start_on: Option<NaiveDate>,
    estimated_minutes: Option<i32>,
}

/// 引数の形の誤りを、API を呼ぶ前に返す
fn parse_new_task(args: NewTaskArgs) -> Result<ParsedNewTask, ToolFailure> {
    let parent_task_id = parse_optional_uuid("parent_task_id", args.parent_task_id.as_deref())?;
    let soft_deadline = args
        .soft_deadline
        .as_deref()
        .map(|value| parse_deadline("soft_deadline", value))
        .transpose()?;
    let hard_deadline = args
        .hard_deadline
        .as_deref()
        .map(|value| parse_deadline("hard_deadline", value))
        .transpose()?;
    let planned_start_on = args
        .planned_start_on
        .as_deref()
        .map(|value| parse_date("planned_start_on", value))
        .transpose()?;
    Ok(ParsedNewTask {
        title: args.title,
        description: args.description,
        status: args.status,
        priority: args.priority,
        parent_task_id,
        labels: args.labels.unwrap_or_default(),
        assignees: args.assignees.unwrap_or_default(),
        task_type: args.task_type,
        milestone: args.milestone,
        sprint: args.sprint,
        soft_deadline,
        hard_deadline,
        planned_start_on,
        estimated_minutes: args.estimated_minutes,
    })
}

/// 作るタスクの状態。UUID はそのまま使い、名前と省略のときだけ状態の一覧を引く。
/// 一覧は `statuses` に取っておき、一括作成の要素の間で使い回す
async fn status_for_new_task(
    api: &InnerApi,
    tenant_id: &str,
    project_id: &str,
    status: Option<&str>,
    statuses: &mut Option<Vec<Value>>,
) -> Result<Uuid, ToolFailure> {
    if let Some(uuid) = status.and_then(|value| Uuid::parse_str(value).ok()) {
        return Ok(uuid);
    }
    if statuses.is_none() {
        *statuses = Some(resolve::list_statuses(api, tenant_id, project_id).await?);
    }
    let listed = statuses.as_deref().unwrap_or_default();
    // 作成 API は `status_id` が必須。省略時は既定の状態を使う
    let id = match status {
        Some(name) => resolve::pick_status(listed, name)?,
        None => resolve::pick_default_status(listed)?,
    };
    parse_uuid("status", &id)
}

/// 1 件分の作成の本文。状態・マイルストーン・スプリント・種別・ラベル・担当者の順に引き
/// （CLI の `crates/cli/src/commands/tasks.rs` の作成と同じ順）、1 つでも引けなければ失敗する
async fn new_task_body(
    api: &InnerApi,
    tenant_id: &str,
    project_id: &str,
    names: &mut resolve::ProjectNames<'_>,
    statuses: &mut Option<Vec<Value>>,
    task: ParsedNewTask,
) -> Result<Value, ToolFailure> {
    let status_id =
        status_for_new_task(api, tenant_id, project_id, task.status.as_deref(), statuses).await?;
    let milestone_id = match task.milestone.as_deref() {
        Some(value) => Some(names.milestone(value).await?),
        None => None,
    };
    let sprint_id = match task.sprint.as_deref() {
        Some(value) => Some(names.sprint(value).await?),
        None => None,
    };
    let task_type_id = match task.task_type.as_deref() {
        Some(value) => Some(names.task_type(value).await?),
        None => None,
    };
    let label_ids = names.labels(&task.labels).await?;
    // 担当者は引いた後の ID で重複を除く（初出の順を保つ）
    let mut assignees: Vec<AssigneeInput> = Vec::new();
    for value in &task.assignees {
        let user_id = names.user(value).await?;
        if assignees.iter().all(|assignee| assignee.user_id != user_id) {
            assignees.push(AssigneeInput {
                user_id,
                role: ASSIGNEE_ROLE.to_string(),
            });
        }
    }

    let mut body = to_body(&CreateTaskRequest {
        title: task.title,
        description: task.description,
        status_id,
        priority: None,
        progress_pct: None,
        parent_task_id: task.parent_task_id,
        milestone_id,
        task_type_id,
        sprint_id,
        soft_deadline: task.soft_deadline,
        hard_deadline: task.hard_deadline,
        planned_start_on: task.planned_start_on,
        estimated_minutes: task.estimated_minutes,
        require_all_assignees_done: false,
        assignees,
        label_ids,
        custom_field_values: Vec::new(),
    })?;
    set_priority(&mut body, task.priority);
    Ok(body)
}

#[derive(Deserialize, JsonSchema)]
struct UpdateTaskArgs {
    /// Task ref (KEY-N or UUID)
    task: String,
    /// Project key or UUID. Required when `task` is a UUID; ignored when `task` is KEY-N
    #[serde(default, deserialize_with = "blank_as_none")]
    project: Option<String>,
    /// New title
    title: Option<String>,
    /// New description (use clear_description to remove it)
    description: Option<String>,
    /// Remove the description
    clear_description: Option<bool>,
    /// Status name or UUID
    #[serde(default, deserialize_with = "blank_as_none")]
    status: Option<String>,
    /// CriticalFire, Critical, High, Medium, Low or Trivial
    #[serde(default, deserialize_with = "blank_as_none")]
    priority: Option<String>,
    /// Progress percentage (0-100)
    progress_pct: Option<i16>,
    /// Label names or UUIDs to add, keeping the others
    #[serde(alias = "add_label_ids")]
    add_labels: Option<Vec<String>>,
    /// Label names or UUIDs to remove, keeping the others
    #[serde(alias = "remove_label_ids")]
    remove_labels: Option<Vec<String>>,
    /// true moves the task into the archive, false brings it back
    is_archived: Option<bool>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct AddTaskCommentArgs {
    /// Task ref (KEY-N or UUID)
    task: String,
    /// Project key or UUID. Required when `task` is a UUID; ignored when `task` is KEY-N
    #[serde(default, deserialize_with = "blank_as_none")]
    project: Option<String>,
    /// Comment body (Markdown)
    body: String,
    /// Comment UUID to reply to
    #[serde(default, deserialize_with = "blank_as_none")]
    parent_comment_id: Option<String>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct LinkPullRequestArgs {
    /// Task ref (KEY-N or UUID)
    task: String,
    /// Project key or UUID. Required when `task` is a UUID; ignored when `task` is KEY-N
    #[serde(default, deserialize_with = "blank_as_none")]
    project: Option<String>,
    /// Pull request number
    number: i32,
    /// Complete the task when the pull request is merged
    closes: Option<bool>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

/// タスクの下の資源を指すパスの素。
struct TaskPath {
    tenant_id: String,
    project_id: String,
    task_id: String,
}

impl TaskPath {
    async fn resolve(
        api: &InnerApi,
        tenant: Option<&str>,
        task: &str,
        project: Option<&str>,
    ) -> Result<Self, ToolFailure> {
        let tenant_id = resolve::tenant_id(api, tenant).await?;
        let (project_id, task_id) = resolve::task_location(api, &tenant_id, task, project).await?;
        Ok(Self {
            tenant_id,
            project_id,
            task_id,
        })
    }

    fn segments<'a>(&'a self, tail: &[&'a str]) -> Vec<&'a str> {
        let mut segments = vec![
            "v1",
            "tenants",
            &self.tenant_id,
            "projects",
            &self.project_id,
            "tasks",
            &self.task_id,
        ];
        segments.extend_from_slice(tail);
        segments
    }
}

/// `status` を ID に直す。UUID はそのまま使い、名前と省略のときだけ状態の一覧を引く。
async fn resolve_status(
    api: &InnerApi,
    tenant_id: &str,
    project_id: &str,
    status: Option<&str>,
) -> Result<Option<Uuid>, ToolFailure> {
    let Some(name) = status else {
        return Ok(None);
    };
    if let Ok(uuid) = Uuid::parse_str(name) {
        return Ok(Some(uuid));
    }
    let statuses = resolve::list_statuses(api, tenant_id, project_id).await?;
    parse_uuid("status", &resolve::pick_status(&statuses, name)?).map(Some)
}

impl TaskMcp {
    pub(super) fn inner(&self, parts: &Parts) -> Result<InnerApi, ToolFailure> {
        InnerApi::from_parts(&self.api, parts)
    }

    async fn run_list_project_options(
        &self,
        parts: &Parts,
        args: ListProjectOptionsArgs,
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let tenant_id = resolve::tenant_id(&api, args.tenant.as_deref()).await?;
        let project_id = resolve::project_id(&api, &tenant_id, &args.project).await?;
        api.get(
            &[
                "v1",
                "tenants",
                &tenant_id,
                "projects",
                &project_id,
                args.kind.collection(),
            ],
            &[],
        )
        .await
    }

    async fn run_list_tasks(
        &self,
        parts: &Parts,
        args: ListTasksArgs,
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let tenant_id = resolve::tenant_id(&api, args.tenant.as_deref()).await?;
        let project_id = resolve::project_id(&api, &tenant_id, &args.project).await?;
        // 省略した項目はクエリに付けない（API の既定に任せる）
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(q) = args.q {
            query.push(("q", q));
        }
        if let Some(is_archived) = args.is_archived {
            query.push(("is_archived", is_archived.to_string()));
        }
        if let Some(sort) = args.sort {
            query.push(("sort", sort));
        }
        if let Some(limit) = args.limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(offset) = args.offset {
            query.push(("offset", offset.to_string()));
        }
        api.get(
            &[
                "v1",
                "tenants",
                &tenant_id,
                "projects",
                &project_id,
                "tasks",
            ],
            &query,
        )
        .await
    }

    async fn run_search_tasks(
        &self,
        parts: &Parts,
        args: SearchTasksArgs,
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let tenant_id = resolve::tenant_id(&api, args.tenant.as_deref()).await?;
        let project_id = resolve::project_id(&api, &tenant_id, &args.project).await?;
        let mut query: Vec<(&str, String)> = vec![("q", args.q)];
        if let Some(limit) = args.limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(offset) = args.offset {
            query.push(("offset", offset.to_string()));
        }
        api.get(
            &[
                "v1",
                "tenants",
                &tenant_id,
                "projects",
                &project_id,
                "tasks",
                "search",
            ],
            &query,
        )
        .await
    }

    /// タスクの下の読み取り（`tail` が空ならタスクそのもの）。
    async fn run_task_get(
        &self,
        parts: &Parts,
        args: TaskArgs,
        tail: &[&str],
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let path = TaskPath::resolve(
            &api,
            args.tenant.as_deref(),
            &args.task,
            args.project.as_deref(),
        )
        .await?;
        api.get(&path.segments(tail), &[]).await
    }

    async fn run_create_task(
        &self,
        parts: &Parts,
        args: CreateTaskArgs,
    ) -> Result<Value, ToolFailure> {
        // 引数の形の誤りは、何も呼ぶ前に返す
        let task = parse_new_task(args.task)?;
        let api = self.inner(parts)?;
        let tenant_id = resolve::tenant_id(&api, args.tenant.as_deref()).await?;
        let project_id = resolve::project_id(&api, &tenant_id, &args.project).await?;
        let mut names = resolve::ProjectNames::new(&api, &tenant_id, &project_id);
        let body =
            new_task_body(&api, &tenant_id, &project_id, &mut names, &mut None, task).await?;
        api.post(
            &[
                "v1",
                "tenants",
                &tenant_id,
                "projects",
                &project_id,
                "tasks",
            ],
            &body,
        )
        .await
    }

    async fn run_update_task(
        &self,
        parts: &Parts,
        args: UpdateTaskArgs,
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let path = TaskPath::resolve(
            &api,
            args.tenant.as_deref(),
            &args.task,
            args.project.as_deref(),
        )
        .await?;
        let status_id = resolve_status(
            &api,
            &path.tenant_id,
            &path.project_id,
            args.status.as_deref(),
        )
        .await?;
        let mut names = resolve::ProjectNames::new(&api, &path.tenant_id, &path.project_id);
        let add_label_ids = names.labels(&args.add_labels.unwrap_or_default()).await?;
        let remove_label_ids = names
            .labels(&args.remove_labels.unwrap_or_default())
            .await?;

        // 指定の無い項目は `None` / 既定値のまま送る。API は `None` を「変更なし」と読む
        // （CLI の `update_request` と同じ送り方）
        let mut body = to_body(&UpdateTaskRequest {
            title: args.title,
            description: args.description,
            clear_description: args.clear_description.unwrap_or(false),
            status_id,
            priority: None,
            progress_pct: args.progress_pct,
            parent_task_id: None,
            clear_parent_task_id: false,
            milestone_id: None,
            clear_milestone_id: false,
            task_type_id: None,
            clear_task_type_id: false,
            sprint_id: None,
            clear_sprint_id: false,
            soft_deadline: None,
            clear_soft_deadline: false,
            hard_deadline: None,
            clear_hard_deadline: false,
            planned_start_on: None,
            clear_planned_start_on: false,
            require_all_assignees_done: None,
            estimated_minutes: None,
            clear_estimated_minutes: false,
            is_archived: args.is_archived,
            label_ids: None,
            add_label_ids,
            remove_label_ids,
            assignees: None,
            custom_field_values: None,
        })?;
        set_priority(&mut body, args.priority);
        api.put(&path.segments(&[]), &body).await
    }

    async fn run_add_task_comment(
        &self,
        parts: &Parts,
        args: AddTaskCommentArgs,
    ) -> Result<Value, ToolFailure> {
        let parent_comment_id =
            parse_optional_uuid("parent_comment_id", args.parent_comment_id.as_deref())?;
        let api = self.inner(parts)?;
        let path = TaskPath::resolve(
            &api,
            args.tenant.as_deref(),
            &args.task,
            args.project.as_deref(),
        )
        .await?;
        // `CreateCommentRequest` は `Serialize` を持たない（CLI も同じ形を手で組む）
        let body = json!({ "body": args.body, "parent_comment_id": parent_comment_id });
        api.post(&path.segments(&["comments"]), &body).await
    }

    async fn run_link_pull_request(
        &self,
        parts: &Parts,
        args: LinkPullRequestArgs,
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let path = TaskPath::resolve(
            &api,
            args.tenant.as_deref(),
            &args.task,
            args.project.as_deref(),
        )
        .await?;
        let body = to_body(&LinkTaskPullRequestRequest {
            number: args.number,
            closes: args.closes.unwrap_or(false),
        })?;
        api.post(&path.segments(&["forge", "pull-requests"]), &body)
            .await
    }
}

#[tool_router]
impl TaskMcp {
    #[tool(
        description = "Show the personal access token in use: its owner, scopes, bound tenants and allowed projects.",
        annotations(read_only_hint = true)
    )]
    async fn whoami(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(
            async {
                self.inner(&parts)?
                    .get(&["v1", "personal_tokens", "me"], &[])
                    .await
            }
            .await,
        )
    }

    #[tool(
        description = "List the tenants this token can be used in. Needs the read:tenant scope.",
        annotations(read_only_hint = true)
    )]
    async fn list_tenants(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(async { self.inner(&parts)?.get(&["v1", "tenants"], &[]).await }.await)
    }

    #[tool(
        description = "List the projects of the tenant. Needs the read:project scope.",
        annotations(read_only_hint = true)
    )]
    async fn list_projects(
        &self,
        Parameters(args): Parameters<TenantArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(
            async {
                let api = self.inner(&parts)?;
                let tenant_id = resolve::tenant_id(&api, args.tenant.as_deref()).await?;
                api.get(&["v1", "tenants", &tenant_id, "projects"], &[])
                    .await
            }
            .await,
        )
    }

    #[tool(
        description = "List the statuses, labels, task types, milestones, sprints or assignable users of a project. Use it to see the names and IDs that create_task and update_task take. Scopes: read:task for statuses, labels and task_types; read:milestone for milestones; read:sprint for sprints; write:task for assignable_users.",
        annotations(read_only_hint = true)
    )]
    async fn list_project_options(
        &self,
        Parameters(args): Parameters<ListProjectOptionsArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_list_project_options(&parts, args).await)
    }

    #[tool(
        description = "List tasks in a project. Needs the read:task scope.",
        annotations(read_only_hint = true)
    )]
    async fn list_tasks(
        &self,
        Parameters(args): Parameters<ListTasksArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_list_tasks(&parts, args).await)
    }

    #[tool(
        description = "Search tasks of a project by text. Needs the read:task scope.",
        annotations(read_only_hint = true)
    )]
    async fn search_tasks(
        &self,
        Parameters(args): Parameters<SearchTasksArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_search_tasks(&parts, args).await)
    }

    #[tool(
        description = "Show a task. Needs the read:task scope.",
        annotations(read_only_hint = true)
    )]
    async fn get_task(
        &self,
        Parameters(args): Parameters<TaskArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_task_get(&parts, args, &[]).await)
    }

    #[tool(description = "Create a task. Needs the write:task scope.")]
    async fn create_task(
        &self,
        Parameters(args): Parameters<CreateTaskArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_create_task(&parts, args).await)
    }

    #[tool(
        description = "Update a task. Only the given fields change. Needs the write:task scope."
    )]
    async fn update_task(
        &self,
        Parameters(args): Parameters<UpdateTaskArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_update_task(&parts, args).await)
    }

    #[tool(
        description = "List the comments of a task. Needs the read:task scope.",
        annotations(read_only_hint = true)
    )]
    async fn list_task_comments(
        &self,
        Parameters(args): Parameters<TaskArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_task_get(&parts, args, &["comments"]).await)
    }

    #[tool(description = "Add a comment to a task. Needs the write:task scope.")]
    async fn add_task_comment(
        &self,
        Parameters(args): Parameters<AddTaskCommentArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_add_task_comment(&parts, args).await)
    }

    #[tool(
        description = "List the pull requests linked to a task, and the project's repository. Needs the read:task scope.",
        annotations(read_only_hint = true)
    )]
    async fn list_task_pull_requests(
        &self,
        Parameters(args): Parameters<TaskArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_task_get(&parts, args, &["forge"]).await)
    }

    #[tool(
        description = "Link a pull request of the project's GitHub repository to a task. Needs the write:task scope."
    )]
    async fn link_pull_request(
        &self,
        Parameters(args): Parameters<LinkPullRequestArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_link_pull_request(&parts, args).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_takes_rfc3339_or_a_date() {
        assert_eq!(
            parse_deadline("soft_deadline", "2026-12-01T09:00:00+09:00")
                .unwrap()
                .to_rfc3339(),
            "2026-12-01T00:00:00+00:00"
        );
        assert_eq!(
            parse_deadline("soft_deadline", "2026-12-01")
                .unwrap()
                .to_rfc3339(),
            "2026-12-01T00:00:00+00:00"
        );
        assert_eq!(
            parse_deadline("soft_deadline", "tomorrow")
                .unwrap_err()
                .message,
            "soft_deadline must be RFC 3339 or YYYY-MM-DD"
        );
    }

    #[test]
    fn ids_must_be_uuids() {
        assert_eq!(
            parse_uuid("parent_task_id", "x").unwrap_err().message,
            "parent_task_id must be a UUID"
        );
        assert_eq!(
            parse_date("planned_start_on", "12/01").unwrap_err().message,
            "planned_start_on must be YYYY-MM-DD"
        );
    }

    #[test]
    fn blank_optional_arguments_are_read_as_omitted() {
        let args: TaskArgs =
            serde_json::from_value(json!({ "task": "KEY-1", "project": "", "tenant": "  " }))
                .unwrap();
        assert_eq!(args.project, None);
        assert_eq!(args.tenant, None);

        let args: TaskArgs =
            serde_json::from_value(json!({ "task": "KEY-1", "project": null })).unwrap();
        assert_eq!(args.project, None);
        assert_eq!(args.tenant, None);

        let args: TaskArgs =
            serde_json::from_value(json!({ "task": "KEY-1", "tenant": "acme" })).unwrap();
        assert_eq!(args.tenant.as_deref(), Some("acme"));

        // 本文は空でもそのまま渡し、検証を API に任せる
        let args: UpdateTaskArgs = serde_json::from_value(
            json!({ "task": "KEY-1", "description": "", "status": "", "priority": "" }),
        )
        .unwrap();
        assert_eq!(args.description.as_deref(), Some(""));
        assert_eq!(args.status, None);
        assert_eq!(args.priority, None);

        let args: CreateTaskArgs =
            serde_json::from_value(json!({ "project": "K", "title": "x", "milestone": "" }))
                .unwrap();
        assert_eq!(args.task.milestone, None);
    }

    /// `create_task` の項目を `NewTaskArgs` に切り出しても、引数とスキーマは平らなまま
    #[test]
    fn create_task_arguments_stay_flat() {
        let args: CreateTaskArgs = serde_json::from_value(json!({
            "project": "K",
            "title": "x",
            "estimated_minutes": 30,
            "tenant": "acme",
        }))
        .unwrap();
        assert_eq!(args.project, "K");
        assert_eq!(args.task.title, "x");
        assert_eq!(args.task.estimated_minutes, Some(30));
        assert_eq!(args.tenant.as_deref(), Some("acme"));

        let schema = serde_json::to_value(schemars::schema_for!(CreateTaskArgs)).unwrap();
        let properties = schema["properties"].as_object().expect("properties");
        for name in ["project", "title", "labels", "estimated_minutes", "tenant"] {
            assert!(properties.contains_key(name), "{name}: {schema}");
        }
        assert!(!properties.contains_key("task"), "{schema}");
        assert_eq!(schema["required"], json!(["project", "title"]));
    }

    /// `_id` の付いた旧名も受け続ける
    #[test]
    fn old_argument_names_are_still_accepted() {
        let args: CreateTaskArgs = serde_json::from_value(json!({
            "project": "K",
            "title": "x",
            "label_ids": ["a"],
            "assignee_ids": ["u"],
            "task_type_id": "t",
            "milestone_id": "m",
            "sprint_id": "s",
        }))
        .unwrap();
        let task = args.task;
        assert_eq!(task.labels, Some(vec!["a".to_string()]));
        assert_eq!(task.assignees, Some(vec!["u".to_string()]));
        assert_eq!(task.task_type.as_deref(), Some("t"));
        assert_eq!(task.milestone.as_deref(), Some("m"));
        assert_eq!(task.sprint.as_deref(), Some("s"));

        let args: UpdateTaskArgs = serde_json::from_value(json!({
            "task": "KEY-1",
            "add_label_ids": ["a"],
            "remove_label_ids": ["b"],
        }))
        .unwrap();
        assert_eq!(args.add_labels, Some(vec!["a".to_string()]));
        assert_eq!(args.remove_labels, Some(vec!["b".to_string()]));
    }

    #[test]
    fn priority_is_passed_through_as_a_string() {
        let mut body = json!({ "priority": null });
        set_priority(&mut body, None);
        assert_eq!(body["priority"], Value::Null);
        set_priority(&mut body, Some("High".into()));
        assert_eq!(body["priority"], "High");
    }
}
