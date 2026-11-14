//! 自分のタスク。

use payload::my_tasks::{MyTaskItem, MyTasksListResponse};
use payload::tasks::TaskDetailResponse;

use crate::Context;
use crate::api::ApiClient;
use crate::cli::MyCommand;
use crate::commands::tasks::{check_limit, offset_for, page_summary};
use crate::error::{CliError, Result};
use crate::output::{OutputOptions, print};
use crate::resolve::{TaskRef, find_done_status_id, parse_task_ref, resolve_project};

/// UUID から自分のタスクを探すときの 1 回あたりの取得数。
const PAGE: u64 = 200;

/// サーバーの上限（handler の `list_my_tasks` が 200 に丸める）。
const MAX_LIST_LIMIT: u64 = 200;

pub async fn run(context: &Context, command: MyCommand, output: OutputOptions) -> Result<i32> {
    match command {
        MyCommand::List {
            filter,
            open,
            limit,
            page,
        } => {
            // 範囲外の値は送る前に断る（設定不足より先に、直せる誤りを出す）
            let limit = check_limit(limit, MAX_LIST_LIMIT)?;
            let offset = offset_for(page, limit)?;
            let api = &context.connect().await?;
            let mut query = vec![
                ("filter", filter),
                ("limit", limit.to_string()),
                ("offset", offset.to_string()),
            ];
            if open {
                query.push(("done", "false".into()));
            }
            let tasks: MyTasksListResponse = api.get(&my_tasks_path(api), &query).await?;
            if output.json {
                print(&tasks, output);
            } else {
                print_my_listing(&tasks, output, page, limit);
            }
        }
        MyCommand::Complete { task_ref } => {
            // 参照の形は送る前に確かめる（設定不足より先に、直せる誤りを出す）
            let parsed = parse_task_ref(&task_ref)?;
            let api = &context.connect().await?;
            let (project_id, task_id) = match parsed {
                TaskRef::Uuid(uuid) => {
                    let task = find_my_task(api, &uuid.to_string()).await?;
                    (task.project.id, task.id.to_string())
                }
                TaskRef::Seq {
                    project_key,
                    task_id,
                } => (resolve_project(api, &project_key).await?.id, task_id),
            };
            let status_id = find_done_status_id(api, project_id).await?;
            let project_id = project_id.to_string();
            let updated: TaskDetailResponse = api
                .put(
                    &[
                        "v1",
                        "tenants",
                        api.tenant_id(),
                        "projects",
                        &project_id,
                        "tasks",
                        &task_id,
                    ],
                    &crate::commands::tasks::done_request(status_id),
                )
                .await?;
            print(&updated, output);
        }
    }
    Ok(0)
}

fn my_tasks_path(api: &ApiClient) -> [&str; 6] {
    ["v1", "tenants", api.tenant_id(), "users", "me", "tasks"]
}

/// 人間向けの一覧。総件数を出さないと、既定の 50 件で切れていることに気付けない。
fn print_my_listing(tasks: &MyTasksListResponse, output: OutputOptions, page: u64, limit: u64) {
    if !tasks.tasks.is_empty() {
        print(tasks, output);
    }
    println!("{}", my_listing_summary(tasks, page, limit));
}

/// この API はカーソルを返さないので、続きの有無は「ここまでに見た件数 < 総数」で決める
/// （`tasks search` の `print_hits` と同じ）。
fn my_listing_summary(tasks: &MyTasksListResponse, page: u64, limit: u64) -> String {
    let seen = page
        .saturating_sub(1)
        .saturating_mul(limit)
        .saturating_add(tasks.tasks.len() as u64);
    page_summary(tasks.tasks.len(), tasks.total, page, seen < tasks.total)
}

/// UUID だけでは所属プロジェクトが分からないので、自分のタスクを辿って突き合わせる。
async fn find_my_task(api: &ApiClient, id: &str) -> Result<MyTaskItem> {
    let mut offset = 0u64;
    loop {
        let page: MyTasksListResponse = api
            .get(
                &my_tasks_path(api),
                &[
                    ("filter", "all".into()),
                    ("limit", PAGE.to_string()),
                    ("offset", offset.to_string()),
                ],
            )
            .await?;
        let count = page.tasks.len() as u64;
        if let Some(task) = page
            .tasks
            .into_iter()
            .find(|task| task.id.to_string() == id)
        {
            return Ok(task);
        }
        if count < PAGE {
            return Err(CliError::not_found(format!("Task not found: {id}")));
        }
        offset += PAGE;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn listing(shown: usize, total: u64) -> MyTasksListResponse {
        let task = |n: usize| {
            json!({
                "id": "00000000-0000-0000-0000-000000000001",
                "seq_id": n,
                "seq_key": format!("ME-{n}"),
                "title": format!("task {n}"),
                "status": {
                    "id": "00000000-0000-0000-0000-000000000003",
                    "name": "Todo",
                    "color": "#000000",
                    "is_done_state": false,
                },
                "priority": "medium",
                "soft_deadline": null,
                "hard_deadline": null,
                "planned_start_on": null,
                "project": {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "name": "me",
                    "key": "ME",
                    "is_personal": true,
                },
                "is_personal": true,
            })
        };
        let tasks: Vec<_> = (0..shown).map(task).collect();
        serde_json::from_value(json!({ "tasks": tasks, "total": total })).unwrap()
    }

    #[test]
    fn points_at_the_next_page_only_while_rows_remain() {
        // 担当 246 件。既定の 50 件で切れていることに気付けないと、残りを見落とす
        assert_eq!(
            my_listing_summary(&listing(50, 246), 1, 50),
            "50 件表示 / 全 246 件（--page 2 で続き）"
        );
        assert_eq!(
            my_listing_summary(&listing(46, 246), 5, 50),
            "46 件表示 / 全 246 件"
        );
        // 上限の 200 件で切って、その先のページがある
        assert_eq!(
            my_listing_summary(&listing(200, 246), 1, 200),
            "200 件表示 / 全 246 件（--page 2 で続き）"
        );
        assert_eq!(
            my_listing_summary(&listing(46, 246), 2, 200),
            "46 件表示 / 全 246 件"
        );
    }

    #[test]
    fn the_page_boundary_decides_whether_more_remain() {
        // 境界ちょうど（50 / 50）は続きなし、1 件越える（50 / 51）は続きあり
        assert_eq!(
            my_listing_summary(&listing(50, 50), 1, 50),
            "50 件表示 / 全 50 件"
        );
        assert_eq!(
            my_listing_summary(&listing(50, 51), 1, 50),
            "50 件表示 / 全 51 件（--page 2 で続き）"
        );
    }

    #[test]
    fn an_empty_listing_still_prints_its_total() {
        assert_eq!(
            my_listing_summary(&listing(0, 0), 1, 50),
            "0 件表示 / 全 0 件"
        );
    }
}
