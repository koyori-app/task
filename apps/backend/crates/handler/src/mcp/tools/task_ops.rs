//! タスクの一括作成・削除・PR のリンクの解除と、自分のタスクの一覧。

use axum::http::request::Parts;
use payload::tasks::BULK_CREATE_MAX_TASKS;
use rmcp::{
    ErrorData,
    handler::server::{common::Extension, wrapper::Parameters},
    model::CallToolResult,
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::super::inner::ToolFailure;
use super::super::resolve;
use super::{
    NewTaskArgs, TaskArgs, TaskMcp, TaskPath, blank_as_none, finish, new_task_body, parse_new_task,
    status_for_new_task,
};

#[derive(Deserialize, JsonSchema)]
struct BulkCreateTasksArgs {
    /// Project key or UUID
    project: String,
    /// Tasks to create (1-100). Each element takes the same fields as create_task except project and tenant
    tasks: Vec<NewTaskArgs>,
    /// Status name or UUID for the elements without status (default: the project's default status)
    #[serde(default, deserialize_with = "blank_as_none")]
    status: Option<String>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct UnlinkPullRequestArgs {
    /// Task ref (KEY-N or UUID)
    task: String,
    /// Project key or UUID. Required when `task` is a UUID; ignored when `task` is KEY-N
    #[serde(default, deserialize_with = "blank_as_none")]
    project: Option<String>,
    /// Pull request number
    number: i32,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct ListMyTasksArgs {
    /// today, week, no_due_date, overdue or all (default)
    #[serde(default, deserialize_with = "blank_as_none")]
    filter: Option<String>,
    /// false lists only the tasks that are not done, true only the done ones. Omit for both
    done: Option<bool>,
    /// Tasks per page (1-200)
    limit: Option<u64>,
    /// Number of tasks to skip
    offset: Option<u64>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

/// 要素の失敗に `tasks[i]: ` を付ける（API の要素の失敗と同じ形）。拒否の状態は保つ
fn element_failure(index: usize, failure: ToolFailure) -> ToolFailure {
    ToolFailure {
        status: failure.status,
        message: format!("tasks[{index}]: {}", failure.message),
    }
}

/// 解除する PR の ID を番号で選ぶ。規則と文面は CLI の
/// `crates/cli/src/commands/tasks.rs` の `pick_linked_pull_request` を写したもの。
///
/// 番号が合う PR が 1 件ならそれ。複数あれば、プロジェクトの現在の連携先のものが 1 件ならそれ。
/// 決まらなければ黙って片方を外さず、候補を並べて断る
fn pick_linked_pull_request(forge: &Value, number: i32, task: &str) -> Result<String, ToolFailure> {
    const REPOSITORY_FIELDS: [&str; 4] = ["host", "host_url", "repo_owner", "repo_name"];
    let matches: Vec<&Value> = forge["pull_requests"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|pull_request| pull_request["number"].as_i64() == Some(i64::from(number)))
        .collect();
    let repository = &forge["repository"];
    let in_current: Vec<&Value> = matches
        .iter()
        .copied()
        .filter(|pull_request| {
            repository.is_object()
                && REPOSITORY_FIELDS
                    .iter()
                    .all(|field| pull_request[field] == repository[field])
        })
        .collect();
    let picked = match (matches.as_slice(), in_current.as_slice()) {
        ([only], _) | (_, [only]) => *only,
        ([], _) => {
            return Err(ToolFailure::new(format!(
                "Pull request #{number} is not linked to {task}"
            )));
        }
        _ => {
            let names: Vec<String> = matches
                .iter()
                .map(|pull_request| {
                    format!(
                        "{}/{}#{}",
                        pull_request["repo_owner"].as_str().unwrap_or_default(),
                        pull_request["repo_name"].as_str().unwrap_or_default(),
                        number
                    )
                })
                .collect();
            return Err(ToolFailure::new(format!(
                "Several pull requests #{number} are linked to {task} ({}); unlink it from the web UI",
                names.join(", ")
            )));
        }
    };
    picked["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| ToolFailure::new("The linked pull request has no id"))
}

impl TaskMcp {
    async fn run_bulk_create_tasks(
        &self,
        parts: &Parts,
        args: BulkCreateTasksArgs,
    ) -> Result<Value, ToolFailure> {
        // 件数と各要素の形の誤りは、何も呼ぶ前に返す
        if args.tasks.is_empty() {
            return Err(ToolFailure::new("tasks must not be empty"));
        }
        if args.tasks.len() as u64 > BULK_CREATE_MAX_TASKS {
            return Err(ToolFailure::new(format!(
                "Too many tasks: {} (at most {BULK_CREATE_MAX_TASKS} at once)",
                args.tasks.len()
            )));
        }
        let parsed = args
            .tasks
            .into_iter()
            .enumerate()
            .map(|(index, task)| {
                parse_new_task(task).map_err(|failure| element_failure(index, failure))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let api = self.inner(parts)?;
        let tenant_id = resolve::tenant_id(&api, args.tenant.as_deref()).await?;
        let project_id = resolve::project_id(&api, &tenant_id, &args.project).await?;
        // 状態とラベルなどの一覧は要素の間で使い回し、種類ごとに 1 回だけ引く。
        // 担当者は引けた名前ごとに取っておく
        let mut names = resolve::ProjectNames::new(&api, &tenant_id, &project_id);
        let mut statuses = None;
        // 引数の `status` は要素より先に 1 回だけ引く。誤りを要素の失敗に見せず、
        // 全要素が `status` を持つときも確かめる
        let default_status = match args.status.as_deref() {
            Some(status) => Some(
                status_for_new_task(&api, &tenant_id, &project_id, Some(status), &mut statuses)
                    .await?
                    .to_string(),
            ),
            None => None,
        };
        let mut bodies = Vec::with_capacity(parsed.len());
        for (index, mut task) in parsed.into_iter().enumerate() {
            if task.status.is_none() {
                task.status.clone_from(&default_status);
            }
            let body = new_task_body(
                &api,
                &tenant_id,
                &project_id,
                &mut names,
                &mut statuses,
                task,
            )
            .await
            .map_err(|failure| element_failure(index, failure))?;
            bodies.push(body);
        }
        // 要素は `priority` を足した後の JSON なので、型付きの要求を通さずに並べる
        api.post(
            &[
                "v1",
                "tenants",
                &tenant_id,
                "projects",
                &project_id,
                "tasks",
                "bulk-create",
            ],
            &json!({ "tasks": bodies }),
        )
        .await
    }

    async fn run_unlink_pull_request(
        &self,
        parts: &Parts,
        args: UnlinkPullRequestArgs,
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let path = TaskPath::resolve(
            &api,
            args.tenant.as_deref(),
            &args.task,
            args.project.as_deref(),
        )
        .await?;
        let forge = api.get(&path.segments(&["forge"]), &[]).await?;
        let id = pick_linked_pull_request(&forge, args.number, &args.task)?;
        api.delete(&path.segments(&["forge", "pull-requests", &id]))
            .await
    }

    async fn run_list_my_tasks(
        &self,
        parts: &Parts,
        args: ListMyTasksArgs,
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let tenant_id = resolve::tenant_id(&api, args.tenant.as_deref()).await?;
        // 省略した項目はクエリに付けない（API の既定に任せる）
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(filter) = args.filter {
            query.push(("filter", filter));
        }
        if let Some(done) = args.done {
            query.push(("done", done.to_string()));
        }
        if let Some(limit) = args.limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(offset) = args.offset {
            query.push(("offset", offset.to_string()));
        }
        api.get(
            &["v1", "tenants", &tenant_id, "users", "me", "tasks"],
            &query,
        )
        .await
    }
}

#[tool_router(router = task_ops_router, vis = "pub(super)")]
impl TaskMcp {
    #[tool(
        description = "Create up to 100 tasks at once in one project (all or none). Each element takes the same fields as create_task. A failure names the element as tasks[i] (0-based). Needs the write:task scope."
    )]
    async fn bulk_create_tasks(
        &self,
        Parameters(args): Parameters<BulkCreateTasksArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_bulk_create_tasks(&parts, args).await)
    }

    #[tool(
        description = "Delete a task. Destructive: a deleted task cannot be restored through this server or the API. Only the creator of the task or the tenant owner can delete it. Needs the write:task scope."
    )]
    async fn delete_task(
        &self,
        Parameters(args): Parameters<TaskArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(
            async {
                let api = self.inner(&parts)?;
                let path = TaskPath::resolve(
                    &api,
                    args.tenant.as_deref(),
                    &args.task,
                    args.project.as_deref(),
                )
                .await?;
                api.delete(&path.segments(&[])).await
            }
            .await,
        )
    }

    #[tool(
        description = "Unlink a pull request from a task by its number (removes the link from every source). The pull request itself is kept and link_pull_request links it again (pass closes again if it was set); a KEY-N left in its title or body links it back on the next pull request event. Needs the write:task scope."
    )]
    async fn unlink_pull_request(
        &self,
        Parameters(args): Parameters<UnlinkPullRequestArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_unlink_pull_request(&parts, args).await)
    }

    #[tool(
        description = "List the tasks assigned to the token owner across the projects of the tenant, including the personal project (today and week are computed in UTC). To change one, pass its id and project.id to update_task. Needs the read:task scope.",
        annotations(read_only_hint = true)
    )]
    async fn list_my_tasks(
        &self,
        Parameters(args): Parameters<ListMyTasksArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_list_my_tasks(&parts, args).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CLI の試験（`crates/cli/src/commands/tasks.rs` の `linked_pull_request`）と同じ形の PR
    fn linked_pull_request(id: u128, repo: &str, number: i32) -> Value {
        let (owner, name) = repo.split_once('/').unwrap();
        json!({
            "id": uuid::Uuid::from_u128(id).to_string(),
            "host": "github",
            "host_url": "https://github.com",
            "repo_owner": owner,
            "repo_name": name,
            "number": number,
        })
    }

    fn forge_with(repository: Option<&str>, pull_requests: Vec<Value>) -> Value {
        let repository = repository.map(|repo| {
            let (owner, name) = repo.split_once('/').unwrap();
            json!({
                "host": "github",
                "host_url": "https://github.com",
                "repo_owner": owner,
                "repo_name": name,
                "html_url": format!("https://github.com/{repo}"),
            })
        });
        json!({ "pull_requests": pull_requests, "repository": repository })
    }

    #[test]
    fn unlink_picks_the_linked_pull_request_with_that_number() {
        let forge = forge_with(
            Some("acme/app"),
            vec![
                linked_pull_request(1, "acme/app", 87),
                linked_pull_request(2, "acme/app", 88),
            ],
        );
        assert_eq!(
            pick_linked_pull_request(&forge, 88, "APP-7").unwrap(),
            uuid::Uuid::from_u128(2).to_string()
        );
        assert_eq!(
            pick_linked_pull_request(&forge, 99, "APP-7")
                .unwrap_err()
                .message,
            "Pull request #99 is not linked to APP-7"
        );
    }

    /// 同じ番号が別リポジトリにもあれば現在の連携先を選び、選べなければ候補を並べて断る
    #[test]
    fn unlink_prefers_the_current_repository_and_refuses_to_guess_otherwise() {
        let both = || {
            vec![
                linked_pull_request(1, "acme/old", 12),
                linked_pull_request(2, "acme/app", 12),
            ]
        };
        assert_eq!(
            pick_linked_pull_request(&forge_with(Some("acme/app"), both()), 12, "APP-7").unwrap(),
            uuid::Uuid::from_u128(2).to_string()
        );
        for repository in [None, Some("acme/other")] {
            assert_eq!(
                pick_linked_pull_request(&forge_with(repository, both()), 12, "APP-7")
                    .unwrap_err()
                    .message,
                "Several pull requests #12 are linked to APP-7 (acme/old#12, acme/app#12); unlink it from the web UI",
                "{repository:?}"
            );
        }
    }
}
