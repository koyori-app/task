//! スプリントの表示・開始・完了。スプリントの一覧は `list_project_options` の `sprints` が返す。

use axum::http::request::Parts;
use payload::sprints::CompleteSprintRequest;
use rmcp::{
    ErrorData,
    handler::server::{common::Extension, wrapper::Parameters},
    model::CallToolResult,
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::super::inner::{InnerApi, ToolFailure};
use super::super::resolve;
use super::{TaskMcp, blank_as_none, finish, parse_uuid, to_body};

#[derive(Deserialize, JsonSchema)]
struct SprintArgs {
    /// Project key or UUID
    project: String,
    /// Sprint UUID
    sprint_id: String,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

/// `/v1/tenants/{tenant}/projects/{project}/sprints/{sprint}` のセグメント。
/// `sprint_id` の形の誤りは、何も呼ぶ前に返す
async fn sprint_segments(api: &InnerApi, args: &SprintArgs) -> Result<Vec<String>, ToolFailure> {
    let sprint_id = parse_uuid("sprint_id", &args.sprint_id)?;
    let tenant_id = resolve::tenant_id(api, args.tenant.as_deref()).await?;
    let project_id = resolve::project_id(api, &tenant_id, &args.project).await?;
    Ok(vec![
        "v1".into(),
        "tenants".into(),
        tenant_id,
        "projects".into(),
        project_id,
        "sprints".into(),
        sprint_id.to_string(),
    ])
}

impl TaskMcp {
    /// スプリント（`action` が空ならそのもの）を呼ぶ。`body` があれば POST、無ければ GET
    async fn run_sprint(
        &self,
        parts: &Parts,
        args: SprintArgs,
        action: Option<&str>,
        body: Option<Value>,
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let mut segments = sprint_segments(&api, &args).await?;
        segments.extend(action.map(str::to_string));
        let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
        match body {
            Some(body) => api.post(&segments, &body).await,
            None => api.get(&segments, &[]).await,
        }
    }
}

#[tool_router(router = sprints_router, vis = "pub(super)")]
impl TaskMcp {
    #[tool(
        description = "Show a sprint with its task counts and burndown. Needs the read:sprint scope.",
        annotations(read_only_hint = true)
    )]
    async fn get_sprint(
        &self,
        Parameters(args): Parameters<SprintArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_sprint(&parts, args, None, None).await)
    }

    #[tool(
        description = "Start a planning sprint. Cannot be undone: a started sprint cannot go back to planning. Fails with 409 conflict when the sprint is not in planning or another sprint of the project is active. Needs the write:sprint scope, which an OAuth connection cannot be granted: use a personal access token."
    )]
    async fn start_sprint(
        &self,
        Parameters(args): Parameters<SprintArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        // 開始の API は本文を読まない（CLI も空の本文を送る）
        finish(
            self.run_sprint(&parts, args, Some("start"), Some(Value::Null))
                .await,
        )
    }

    #[tool(
        description = "Complete the active sprint. Destructive: it cannot be undone, and the tasks that are not done are taken out of the sprint. Fails with 409 conflict when the sprint is not active. Needs the write:sprint and write:task scopes; an OAuth connection cannot be granted write:sprint: use a personal access token."
    )]
    async fn complete_sprint(
        &self,
        Parameters(args): Parameters<SprintArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        // 未完了のタスクは完了の API が必ずスプリントから外す（`move_incomplete_to_backlog`
        // の真偽で結果は変わらない）ので、CLI の `--backlog` に当たる引数は作らない
        finish(
            async {
                let body = to_body(&CompleteSprintRequest {
                    move_incomplete_to_sprint_id: None,
                    move_incomplete_to_backlog: false,
                })?;
                self.run_sprint(&parts, args, Some("complete"), Some(body))
                    .await
            }
            .await,
        )
    }
}
