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
    handler::server::{common::Extension, wrapper::Parameters},
    model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::inner::{InnerApi, ToolFailure, ok};
use super::resolve;

const INSTRUCTIONS: &str = "Tools for the task tracker. Refer to a task as KEY-N (project key and number, e.g. TASK-42) or by UUID together with `project`. \
`tenant` is only needed when the personal access token is bound to more than one tenant; `whoami` shows the tenants and scopes of the token. \
Labels, assignees, task types, milestones and sprints are passed by ID: use `list_project_options` to look up the IDs by name. \
Referring to a project by key or to a task as KEY-N needs the `read:project` scope; without it, pass UUIDs. \
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
}

#[tool_handler]
impl ServerHandler for TaskMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("task", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// 失敗も `isError: true` の結果として返す。プロトコルのエラー（`Err`）にすると、
/// クライアントが文面を利用者に見せない
fn finish(result: Result<Value, ToolFailure>) -> Result<CallToolResult, ErrorData> {
    Ok(match result {
        Ok(value) => ok(value),
        Err(failure) => failure.into(),
    })
}

fn parse_uuid(field: &str, value: &str) -> Result<Uuid, ToolFailure> {
    Uuid::parse_str(value).map_err(|_| ToolFailure::new(format!("{field} must be a UUID")))
}

fn parse_uuids(field: &str, values: &[String]) -> Result<Vec<Uuid>, ToolFailure> {
    values
        .iter()
        .map(|value| parse_uuid(field, value))
        .collect()
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

fn to_body<T: serde::Serialize>(request: &T) -> Result<Value, ToolFailure> {
    serde_json::to_value(request)
        .map_err(|error| ToolFailure::new(format!("Could not build the request body: {error}")))
}

/// `priority` は文字列のまま渡し、検証は API に任せる
fn set_priority(body: &mut Value, priority: Option<String>) {
    if let Some(priority) = priority {
        body["priority"] = Value::String(priority);
    }
}

#[derive(Deserialize, JsonSchema)]
struct TenantArgs {
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
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
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct ListTasksArgs {
    /// Project key or UUID
    project: String,
    /// Filter expression of the task list (same syntax as the filter box of the list screen). Omit for no filter
    q: Option<String>,
    /// List archived tasks instead of active ones
    is_archived: Option<bool>,
    /// created_at_desc (default), or title / assignee / priority / deadline followed by _asc or _desc
    sort: Option<String>,
    /// Tasks per page (1-200)
    limit: Option<u64>,
    /// Number of tasks to skip
    offset: Option<u64>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
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
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct TaskArgs {
    /// Task ref (KEY-N or UUID)
    task: String,
    /// Project key or UUID. Required when `task` is a UUID
    project: Option<String>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct CreateTaskArgs {
    /// Project key or UUID
    project: String,
    /// Task title
    title: String,
    /// Task description (Markdown)
    description: Option<String>,
    /// Status name or UUID (default: the project's default status)
    status: Option<String>,
    /// CriticalFire, Critical, High, Medium, Low or Trivial
    priority: Option<String>,
    /// Parent task UUID. A new task takes the parent's deadlines and planned start unless set here
    parent_task_id: Option<String>,
    /// Label UUIDs
    label_ids: Option<Vec<String>>,
    /// User UUIDs to assign
    assignee_ids: Option<Vec<String>>,
    /// Task type UUID
    task_type_id: Option<String>,
    /// Milestone UUID
    milestone_id: Option<String>,
    /// Sprint UUID
    sprint_id: Option<String>,
    /// Soft deadline (RFC 3339, or YYYY-MM-DD for the start of that day in UTC)
    soft_deadline: Option<String>,
    /// Hard deadline (RFC 3339, or YYYY-MM-DD for the start of that day in UTC)
    hard_deadline: Option<String>,
    /// Planned start date (YYYY-MM-DD)
    planned_start_on: Option<String>,
    /// Estimate in minutes (1 or more)
    estimated_minutes: Option<i32>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct UpdateTaskArgs {
    /// Task ref (KEY-N or UUID)
    task: String,
    /// Project key or UUID. Required when `task` is a UUID
    project: Option<String>,
    /// New title
    title: Option<String>,
    /// New description (use clear_description to remove it)
    description: Option<String>,
    /// Remove the description
    clear_description: Option<bool>,
    /// Status name or UUID
    status: Option<String>,
    /// CriticalFire, Critical, High, Medium, Low or Trivial
    priority: Option<String>,
    /// Progress percentage (0-100)
    progress_pct: Option<i16>,
    /// Label UUIDs to add, keeping the others
    add_label_ids: Option<Vec<String>>,
    /// Label UUIDs to remove, keeping the others
    remove_label_ids: Option<Vec<String>>,
    /// true moves the task into the archive, false brings it back
    is_archived: Option<bool>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct AddTaskCommentArgs {
    /// Task ref (KEY-N or UUID)
    task: String,
    /// Project key or UUID. Required when `task` is a UUID
    project: Option<String>,
    /// Comment body (Markdown)
    body: String,
    /// Comment UUID to reply to
    parent_comment_id: Option<String>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct LinkPullRequestArgs {
    /// Task ref (KEY-N or UUID)
    task: String,
    /// Project key or UUID. Required when `task` is a UUID
    project: Option<String>,
    /// Pull request number
    number: i32,
    /// Complete the task when the pull request is merged
    closes: Option<bool>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
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
    fn inner(&self, parts: &Parts) -> Result<InnerApi, ToolFailure> {
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
        let parent_task_id = parse_optional_uuid("parent_task_id", args.parent_task_id.as_deref())?;
        let milestone_id = parse_optional_uuid("milestone_id", args.milestone_id.as_deref())?;
        let task_type_id = parse_optional_uuid("task_type_id", args.task_type_id.as_deref())?;
        let sprint_id = parse_optional_uuid("sprint_id", args.sprint_id.as_deref())?;
        let label_ids = parse_uuids("label_ids", &args.label_ids.unwrap_or_default())?;
        let assignees = parse_uuids("assignee_ids", &args.assignee_ids.unwrap_or_default())?
            .into_iter()
            .map(|user_id| AssigneeInput {
                user_id,
                role: ASSIGNEE_ROLE.to_string(),
            })
            .collect();
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

        let api = self.inner(parts)?;
        let tenant_id = resolve::tenant_id(&api, args.tenant.as_deref()).await?;
        let project_id = resolve::project_id(&api, &tenant_id, &args.project).await?;
        // 作成 API は `status_id` が必須。省略時は既定の状態を引く
        let status_id =
            match resolve_status(&api, &tenant_id, &project_id, args.status.as_deref()).await? {
                Some(status_id) => status_id,
                None => {
                    let statuses = resolve::list_statuses(&api, &tenant_id, &project_id).await?;
                    parse_uuid("status", &resolve::pick_default_status(&statuses)?)?
                }
            };

        let mut body = to_body(&CreateTaskRequest {
            title: args.title,
            description: args.description,
            status_id,
            priority: None,
            progress_pct: None,
            parent_task_id,
            milestone_id,
            task_type_id,
            sprint_id,
            soft_deadline,
            hard_deadline,
            planned_start_on,
            estimated_minutes: args.estimated_minutes,
            require_all_assignees_done: false,
            assignees,
            label_ids,
            custom_field_values: Vec::new(),
        })?;
        set_priority(&mut body, args.priority);
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
        let add_label_ids = parse_uuids("add_label_ids", &args.add_label_ids.unwrap_or_default())?;
        let remove_label_ids = parse_uuids(
            "remove_label_ids",
            &args.remove_label_ids.unwrap_or_default(),
        )?;

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
        description = "List the statuses, labels, task types, milestones, sprints or assignable users of a project. Use it to look up the IDs that create_task and update_task take. Scopes: read:task for statuses, labels and task_types; read:milestone for milestones; read:sprint for sprints; write:task for assignable_users.",
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
            parse_uuids("label_ids", &["bug".to_string()])
                .unwrap_err()
                .message,
            "label_ids must be a UUID"
        );
        assert_eq!(
            parse_date("planned_start_on", "12/01").unwrap_err().message,
            "planned_start_on must be YYYY-MM-DD"
        );
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
