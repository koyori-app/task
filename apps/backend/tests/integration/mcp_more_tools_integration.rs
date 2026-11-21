//! MCP の追加のツール（TASK-561）。規則は docs/dev/features/mcp.md。
//!
//! - `list_tenants`: 束縛したテナントだけ返る。`read:tenant` が無ければ 403
//! - `bulk_create_tasks`: 状態の決まり方と入力の順、100 件の境界と 0 件、要素の失敗は
//!   `tasks[i]` を付けて 1 件も作らない、`write:task` が無ければ 403
//! - `delete_task`: 作成者は消せて `get_task` が 404、作成者でもオーナーでもない者と
//!   `write:task` の無い PAT は 403
//! - `unlink_pull_request`: 番号で外れる、結んでいない番号の文面、`write:task` だけで外れ、
//!   `read:task` だけでは 403
//! - `list_my_tasks`: 担当のタスクだけ、`done`・`limit` の絞り込み、`read:task` が無ければ 403、
//!   束縛の外のテナントは 403
//! - スプリント: 表示・開始・二度目の開始の 409・完了で未完了のタスクが外れる、`sprint_id` の形、
//!   `write:sprint` が無ければ 403（OAuth の 403 は mcp_oauth_integration.rs）
//! - 通知: 一覧とカーソル、1 件と全件の既読、設定の表示と片側だけの更新、UUID の `project` は
//!   テナントを読まないが束縛の外は 403、PAT の scope に応じた種別だけが見える
//!
//! scope を絞った PAT の試験では、`project` と `task` を UUID で渡す（キーと `KEY-N` の解決は
//! `read:project` が要り、拒否の原因が混ざる）。

use crate::common::{TestApp, insert_extra_project, insert_tenant};
use crate::forge_commit_links_integration::{REPO_NAME, REPO_OWNER, insert_task, setup};
use crate::forge_manual_pull_request_links_integration::{forget_github_mock, github_mock};
use crate::mcp_integration::{
    Fixture, api_pat, call, create_label, create_sprint, create_task, fixture, task_count, username,
};
use entity::scopes::Scope;
use entity::tenant_members::{self, TenantRole};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

async fn pat(app: &TestApp, fx: &Fixture, scopes: Vec<Scope>) -> String {
    app.insert_pat(fx.user.id, fx.tp.tenant_id, scopes, None)
        .await
}

/// テナントの一般のメンバー（オーナーでない）を足す。
async fn insert_member(app: &TestApp, tenant_id: Uuid) -> crate::common::TestUser {
    let user = app.insert_user_default().await;
    tenant_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        tenant_id: Set(tenant_id),
        user_id: Set(user.id),
        role: Set(TenantRole::Member),
        department: Set(None),
        position: Set(None),
        team: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert tenant member");
    user
}

/// `n` 件の要素（題は `bulk {i}`）。
fn bulk_elements(n: usize) -> Vec<Value> {
    (0..n)
        .map(|i| json!({ "title": format!("bulk {i}") }))
        .collect()
}

// ---- list_tenants ----

#[tokio::test]
async fn list_tenants_returns_the_bound_tenant_and_needs_read_tenant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    // 持ち主は同じでも、束縛していないテナントは返らない
    insert_tenant(&app.state.db, fx.user.id).await;
    let token = pat(&app, &fx, vec![Scope::ReadTenant]).await;

    let tenants = call(&app, &token, "list_tenants", json!({})).await.json();
    let ids: Vec<&str> = tenants
        .as_array()
        .expect("tenants")
        .iter()
        .map(|tenant| tenant["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, [fx.tp.tenant_id.to_string()]);

    let token = pat(&app, &fx, vec![Scope::ReadTask]).await;
    let outcome = call(&app, &token, "list_tenants", json!({})).await;
    assert_eq!(outcome.error(), "403 forbidden");
}

// ---- bulk_create_tasks ----

#[tokio::test]
async fn bulk_create_tasks_uses_the_default_and_named_statuses_in_input_order() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let bug = create_label(&app, &fx, "bug").await;

    let created = call(
        &app,
        &token,
        "bulk_create_tasks",
        json!({
            "project": fx.project_key,
            "tasks": [
                { "title": "一件目", "priority": "High", "labels": ["bug"] },
                { "title": "二件目", "status": "Done" },
            ],
        }),
    )
    .await
    .json();
    let tasks = created["tasks"].as_array().expect("tasks");
    let titles: Vec<&str> = tasks
        .iter()
        .map(|task| task["title"].as_str().expect("title"))
        .collect();
    assert_eq!(titles, ["一件目", "二件目"], "入力の順に返る");
    assert_eq!(tasks[0]["status_id"], fx.todo.to_string(), "既定の状態");
    assert_eq!(tasks[0]["priority"], "High", "要素の priority が届く");
    assert_eq!(tasks[0]["labels"][0]["id"], bug.to_string(), "名前で引く");
    assert_eq!(tasks[1]["status_id"], fx.done.to_string(), "名前の状態");

    // 引数の `status` は `status` の無い要素にだけ使う
    let created = call(
        &app,
        &token,
        "bulk_create_tasks",
        json!({
            "project": fx.project_key,
            "status": "Done",
            "tasks": [
                { "title": "既定は Done" },
                { "title": "要素は Todo", "status": fx.todo.to_string() },
            ],
        }),
    )
    .await
    .json();
    assert_eq!(created["tasks"][0]["status_id"], fx.done.to_string());
    assert_eq!(created["tasks"][1]["status_id"], fx.todo.to_string());

    // 引数の `status` の誤りは、全要素が `status` を持っていても、要素の失敗でなく返す
    let before = task_count(&app, &fx).await;
    let outcome = call(
        &app,
        &token,
        "bulk_create_tasks",
        json!({
            "project": fx.project_key,
            "status": "Dnoe",
            "tasks": [{ "title": "要素は Todo", "status": "Todo" }],
        }),
    )
    .await;
    let message = outcome.error();
    assert!(message.starts_with("Status not found: Dnoe"), "{message}");
    assert_eq!(task_count(&app, &fx).await, before, "作らない");
}

#[tokio::test]
async fn bulk_create_tasks_takes_up_to_100_and_refuses_101_or_none() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let outcome = call(
        &app,
        &token,
        "bulk_create_tasks",
        json!({ "project": fx.project_key, "tasks": bulk_elements(101) }),
    )
    .await;
    assert_eq!(outcome.error(), "Too many tasks: 101 (at most 100 at once)");
    let outcome = call(
        &app,
        &token,
        "bulk_create_tasks",
        json!({ "project": fx.project_key, "tasks": [] }),
    )
    .await;
    assert_eq!(outcome.error(), "tasks must not be empty");
    assert_eq!(task_count(&app, &fx).await, 0, "1 件も作らない");

    let created = call(
        &app,
        &token,
        "bulk_create_tasks",
        json!({ "project": fx.project_key, "tasks": bulk_elements(100) }),
    )
    .await
    .json();
    assert_eq!(created["tasks"].as_array().map(Vec::len), Some(100));
    assert_eq!(task_count(&app, &fx).await, 100);
}

#[tokio::test]
async fn bulk_create_tasks_names_the_failed_element_and_creates_none() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    create_label(&app, &fx, "bug").await;

    // 形の誤り（API を呼ぶ前に返す）
    let outcome = call(
        &app,
        &token,
        "bulk_create_tasks",
        json!({
            "project": fx.project_key,
            "tasks": [{ "title": "ok" }, { "title": "ng", "parent_task_id": "x" }],
        }),
    )
    .await;
    assert_eq!(outcome.error(), "tasks[1]: parent_task_id must be a UUID");

    // 名前が引けない
    let outcome = call(
        &app,
        &token,
        "bulk_create_tasks",
        json!({
            "project": fx.project_key,
            "tasks": [{ "title": "ok", "labels": ["bug"] }, { "title": "ng", "labels": ["nope"] }],
        }),
    )
    .await;
    assert_eq!(
        outcome.error(),
        "tasks[1]: Label not found: nope (this project has bug)"
    );

    // API の要素の検証（期限が逆）。API の文面がそのまま `tasks[1]` を持つ
    let outcome = call(
        &app,
        &token,
        "bulk_create_tasks",
        json!({
            "project": fx.project_key,
            "tasks": [
                { "title": "ok" },
                { "title": "ng", "soft_deadline": "2026-12-10", "hard_deadline": "2026-12-01" },
            ],
        }),
    )
    .await;
    let message = outcome.error();
    assert!(message.starts_with("400 tasks[1]: "), "{message}");
    assert_eq!(task_count(&app, &fx).await, 0, "1 件も作らない");
}

#[tokio::test]
async fn bulk_create_tasks_needs_write_task() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = pat(&app, &fx, vec![Scope::ReadTask, Scope::ReadProject]).await;

    let outcome = call(
        &app,
        &token,
        "bulk_create_tasks",
        json!({ "project": fx.tp.project_id.to_string(), "tasks": bulk_elements(1) }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");
    assert_eq!(task_count(&app, &fx).await, 0);
}

// ---- delete_task ----

#[tokio::test]
async fn delete_task_is_for_the_creator_and_the_tenant_owner() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let (_, kept) = create_task(&app, &token, &fx, "残す").await;
    let (_, deleted) = create_task(&app, &token, &fx, "消す").await;
    let project = fx.tp.project_id.to_string();

    // 作成者でもオーナーでもないメンバーは、write:task があっても消せない
    let member = insert_member(&app, fx.tp.tenant_id).await;
    let member_token = app
        .insert_pat(
            member.id,
            fx.tp.tenant_id,
            vec![Scope::ReadTask, Scope::WriteTask],
            None,
        )
        .await;
    let outcome = call(
        &app,
        &member_token,
        "delete_task",
        json!({ "task": kept, "project": project }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");

    let read_only = pat(&app, &fx, vec![Scope::ReadTask]).await;
    let outcome = call(
        &app,
        &read_only,
        "delete_task",
        json!({ "task": kept, "project": project }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");
    assert_eq!(task_count(&app, &fx).await, 2, "どちらも残る");

    let outcome = call(
        &app,
        &token,
        "delete_task",
        json!({ "task": deleted, "project": project }),
    )
    .await;
    assert!(!outcome.is_error, "{}", outcome.text);
    assert_eq!(outcome.text, "ok");
    let fetched = call(
        &app,
        &token,
        "get_task",
        json!({ "task": deleted, "project": project }),
    )
    .await;
    assert_eq!(fetched.error(), "404 not-found");
    call(
        &app,
        &token,
        "get_task",
        json!({ "task": kept, "project": project }),
    )
    .await
    .json();

    // オーナーは、ほかの人が作ったタスクも消せる
    let by_member = call(
        &app,
        &member_token,
        "create_task",
        json!({ "project": project, "title": "メンバーが作る" }),
    )
    .await
    .json();
    let outcome = call(
        &app,
        &token,
        "delete_task",
        json!({ "task": by_member["id"], "project": project }),
    )
    .await;
    assert!(!outcome.is_error, "{}", outcome.text);
    let fetched = call(
        &app,
        &token,
        "get_task",
        json!({ "task": by_member["id"], "project": project }),
    )
    .await;
    assert_eq!(fetched.error(), "404 not-found");
}

// ---- unlink_pull_request ----

#[tokio::test]
async fn unlink_pull_request_by_number_needs_write_task() {
    let mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO_OWNER}/{REPO_NAME}/pulls/87")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 87,
            "state": "open",
            "title": "feat: OAuth",
            "body": "",
            "user": { "login": "octocat" },
            "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/87"),
            "updated_at": "2026-10-01T00:00:00Z",
            "merged_at": null,
            "head": { "ref": "feat/oauth", "sha": "a3f92c1e7b81d4000000000000000000000000aa" }
        })))
        .mount(&mock_server)
        .await;
    let token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;
    let args = |number: i32| {
        json!({
            "task": task.to_string(),
            "project": fx.tp.project_id.to_string(),
            "number": number,
        })
    };

    call(&app, &token, "link_pull_request", args(87))
        .await
        .json();

    // 解除の前の一覧は read:task で引けるが、解除そのものは write:task が要る
    let read_only = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let outcome = call(&app, &read_only, "unlink_pull_request", args(87)).await;
    assert_eq!(outcome.error(), "403 forbidden");

    let outcome = call(&app, &token, "unlink_pull_request", args(5)).await;
    assert_eq!(
        outcome.error(),
        format!("Pull request #5 is not linked to {task}")
    );

    let outcome = call(&app, &token, "unlink_pull_request", args(87)).await;
    assert!(!outcome.is_error, "{}", outcome.text);
    assert_eq!(outcome.text, "ok");
    let forge = call(
        &app,
        &token,
        "list_task_pull_requests",
        json!({ "task": task.to_string(), "project": fx.tp.project_id.to_string() }),
    )
    .await
    .json();
    assert_eq!(forge["pull_requests"], json!([]), "{forge}");
    forget_github_mock();
}

// ---- list_my_tasks ----

#[tokio::test]
async fn list_my_tasks_returns_the_assigned_tasks() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let me = username(&app, fx.user.id).await;
    let assigned = |title: &str, status: Uuid| {
        json!({
            "project": fx.project_key,
            "title": title,
            "status": status.to_string(),
            "assignees": [me.clone()],
        })
    };
    let open = call(&app, &token, "create_task", assigned("未完了", fx.todo))
        .await
        .json();
    call(&app, &token, "create_task", assigned("完了", fx.done))
        .await
        .json();
    create_task(&app, &token, &fx, "担当でない").await;

    let mine = call(&app, &token, "list_my_tasks", json!({})).await.json();
    let mut titles: Vec<&str> = mine["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|task| task["title"].as_str().expect("title"))
        .collect();
    titles.sort_unstable();
    assert_eq!(titles, ["完了", "未完了"], "{mine}");

    let open_only = call(&app, &token, "list_my_tasks", json!({ "done": false }))
        .await
        .json();
    assert_eq!(open_only["tasks"].as_array().map(Vec::len), Some(1));
    assert_eq!(open_only["tasks"][0]["id"], open["id"]);
    // 応答の id と project.id で update_task に渡せる
    assert_eq!(
        open_only["tasks"][0]["project"]["id"],
        fx.tp.project_id.to_string()
    );

    let page = call(&app, &token, "list_my_tasks", json!({ "limit": 1 }))
        .await
        .json();
    assert_eq!(page["tasks"].as_array().map(Vec::len), Some(1));
    assert_eq!(page["total"], 2);

    // write:task は read:task を含むので、タスクの scope を持たないトークンで確かめる
    let tenant_only = pat(&app, &fx, vec![Scope::ReadTenant]).await;
    let outcome = call(&app, &tenant_only, "list_my_tasks", json!({})).await;
    assert_eq!(outcome.error(), "403 forbidden");
}

/// 個人プロジェクトのタスクは `KEY-N` で指せないので、`list_my_tasks` の `id` と
/// `project.id` を `update_task` に渡して完了にする（`my complete` の代わり）
#[tokio::test]
async fn personal_task_is_completed_with_the_ids_from_list_my_tasks() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let res = app
        .get_with_session(&format!(
            "/v1/tenants/{}/users/me/personal-project",
            fx.tp.tenant_id
        ))
        .await;
    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let personal: Value = res.json().await.expect("personal project json");
    assert_eq!(personal["is_personal"], true);
    let personal_id = Uuid::parse_str(personal["id"].as_str().expect("id")).expect("uuid");
    let personal_tp = crate::common::TestTenantProject {
        tenant_id: fx.tp.tenant_id,
        project_id: personal_id,
    };
    let statuses = call(
        &app,
        &token,
        "list_project_options",
        json!({ "project": personal_id.to_string(), "kind": "statuses" }),
    )
    .await
    .json();
    let done = match statuses
        .as_array()
        .expect("statuses")
        .iter()
        .find(|status| status["is_done_state"] == true)
    {
        Some(status) => Uuid::parse_str(status["id"].as_str().expect("id")).expect("uuid"),
        None => {
            crate::common::create_status(&app, &personal_tp, "Todo", false).await;
            crate::common::create_status(&app, &personal_tp, "Done", true).await
        }
    };
    let me = username(&app, fx.user.id).await;
    call(
        &app,
        &token,
        "create_task",
        json!({ "project": personal_id.to_string(), "title": "自分用", "assignees": [me] }),
    )
    .await
    .json();

    let mine = call(&app, &token, "list_my_tasks", json!({ "done": false }))
        .await
        .json();
    let item = mine["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["title"] == "自分用")
        .unwrap_or_else(|| panic!("個人プロジェクトのタスクが一覧に出る: {mine}"));
    assert_eq!(item["project"]["is_personal"], true);

    let updated = call(
        &app,
        &token,
        "update_task",
        json!({
            "task": item["id"],
            "project": item["project"]["id"],
            "status": done.to_string(),
        }),
    )
    .await
    .json();
    assert_eq!(updated["status_id"], done.to_string());
    let mine = call(&app, &token, "list_my_tasks", json!({ "done": false }))
        .await
        .json();
    assert!(
        mine["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .all(|task| task["title"] != "自分用"),
        "{mine}"
    );
}

#[tokio::test]
async fn list_my_tasks_outside_the_token_binding_is_403() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let other_tenant = insert_tenant(&app.state.db, fx.user.id).await;

    let outcome = call(
        &app,
        &token,
        "list_my_tasks",
        json!({ "tenant": other_tenant.to_string() }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");

    let mine = call(
        &app,
        &token,
        "list_my_tasks",
        json!({ "tenant": fx.tp.tenant_id.to_string() }),
    )
    .await
    .json();
    assert_eq!(mine["total"], 0);
}

// ---- スプリント ----

#[tokio::test]
async fn sprint_is_shown_started_and_completed() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let sprint = create_sprint(&app, &fx, "week-1").await;
    let in_sprint = call(
        &app,
        &token,
        "create_task",
        json!({ "project": fx.project_key, "title": "未完了", "sprint": sprint.to_string() }),
    )
    .await
    .json();
    let args = json!({ "project": fx.project_key, "sprint_id": sprint.to_string() });

    let shown = call(&app, &token, "get_sprint", args.clone()).await.json();
    assert_eq!(shown["id"], sprint.to_string());
    assert_eq!(shown["status"], "planning");
    assert_eq!(shown["task_counts"]["total"], 1, "{shown}");
    assert!(shown["burndown"].is_array(), "{shown}");

    let started = call(&app, &token, "start_sprint", args.clone())
        .await
        .json();
    assert_eq!(started["status"], "active");
    let again = call(&app, &token, "start_sprint", args.clone()).await;
    assert_eq!(again.error(), "409 conflict");

    let completed = call(&app, &token, "complete_sprint", args.clone())
        .await
        .json();
    assert_eq!(completed["status"], "completed");
    let fetched = call(
        &app,
        &token,
        "get_task",
        json!({ "task": in_sprint["id"], "project": fx.project_key }),
    )
    .await
    .json();
    assert_eq!(fetched["sprint_id"], Value::Null, "未完了のタスクは外れる");

    let outcome = call(
        &app,
        &token,
        "get_sprint",
        json!({ "project": fx.project_key, "sprint_id": "week-1" }),
    )
    .await;
    assert_eq!(outcome.error(), "sprint_id must be a UUID");
}

#[tokio::test]
async fn start_sprint_needs_write_sprint() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let sprint = create_sprint(&app, &fx, "week-1").await;
    let token = pat(&app, &fx, vec![Scope::WriteTask, Scope::ReadSprint]).await;
    let args = json!({ "project": fx.tp.project_id.to_string(), "sprint_id": sprint.to_string() });

    let outcome = call(&app, &token, "start_sprint", args.clone()).await;
    assert_eq!(outcome.error(), "403 forbidden");
    let shown = call(&app, &token, "get_sprint", args).await.json();
    assert_eq!(shown["status"], "planning", "開始されていない");
}

/// 完了は write:sprint と write:task の両方が要る。片方だけでは断り、スプリントは進行中のまま
#[tokio::test]
async fn complete_sprint_needs_write_sprint_and_write_task() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let sprint = create_sprint(&app, &fx, "week-1").await;
    let args = json!({ "project": fx.tp.project_id.to_string(), "sprint_id": sprint.to_string() });
    let token = api_pat(&app, &fx).await;
    call(&app, &token, "start_sprint", args.clone())
        .await
        .json();

    for scopes in [
        vec![Scope::WriteSprint],
        vec![Scope::WriteTask, Scope::ReadSprint],
    ] {
        let narrow = pat(&app, &fx, scopes.clone()).await;
        let outcome = call(&app, &narrow, "complete_sprint", args.clone()).await;
        assert_eq!(outcome.error(), "403 forbidden", "{scopes:?}");
    }
    let shown = call(&app, &token, "get_sprint", args.clone()).await.json();
    assert_eq!(shown["status"], "active", "完了していない");

    let both = pat(&app, &fx, vec![Scope::WriteSprint, Scope::WriteTask]).await;
    let completed = call(&app, &both, "complete_sprint", args).await.json();
    assert_eq!(completed["status"], "completed");
}

// ---- 通知 ----

/// 担当の通知を 1 件直に挿す。`minutes_ago` で新旧の順を決める
async fn insert_notification(app: &TestApp, fx: &Fixture, minutes_ago: i64) -> Uuid {
    insert_notification_of(app, fx, minutes_ago, common::notifications::TYPE_ASSIGNED).await
}

async fn insert_notification_of(
    app: &TestApp,
    fx: &Fixture,
    minutes_ago: i64,
    notification_type: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    entity::notifications::ActiveModel {
        id: Set(id),
        user_id: Set(fx.user.id),
        task_id: Set(None),
        project_id: Set(Some(fx.tp.project_id)),
        notification_type: Set(notification_type.into()),
        payload: Set(json!({})),
        read_at: Set(None),
        created_at: Set((chrono::Utc::now() - chrono::Duration::minutes(minutes_ago)).into()),
        email_queued_at: Set(None),
        emailed_at: Set(None),
        email_attempts: Set(0),
    }
    .insert(&app.state.db)
    .await
    .expect("insert notification");
    id
}

fn notification_ids(list: &Value) -> Vec<String> {
    list["notifications"]
        .as_array()
        .expect("notifications")
        .iter()
        .map(|item| item["id"].as_str().expect("id").to_string())
        .collect()
}

#[tokio::test]
async fn notifications_are_listed_by_cursor_and_marked_read() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let oldest = insert_notification(&app, &fx, 3).await;
    let middle = insert_notification(&app, &fx, 2).await;
    let newest = insert_notification(&app, &fx, 1).await;

    let first = call(&app, &token, "list_notifications", json!({ "limit": 2 }))
        .await
        .json();
    assert_eq!(first["unread_count"], 3);
    assert_eq!(
        notification_ids(&first),
        [newest.to_string(), middle.to_string()]
    );
    let cursor = first["next_cursor"].as_str().expect("next_cursor");
    let rest = call(
        &app,
        &token,
        "list_notifications",
        json!({ "limit": 2, "cursor": cursor }),
    )
    .await
    .json();
    assert_eq!(notification_ids(&rest), [oldest.to_string()]);

    let read_only = pat(&app, &fx, vec![Scope::ReadTask]).await;
    let outcome = call(
        &app,
        &read_only,
        "mark_notification_read",
        json!({ "notification_id": newest.to_string() }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");
    let outcome = call(
        &app,
        &token,
        "mark_notification_read",
        json!({ "notification_id": "x" }),
    )
    .await;
    assert_eq!(outcome.error(), "notification_id must be a UUID");

    let read = call(
        &app,
        &token,
        "mark_notification_read",
        json!({ "notification_id": newest.to_string() }),
    )
    .await
    .json();
    assert!(read["read_at"].is_string(), "{read}");
    let unread = call(
        &app,
        &token,
        "list_notifications",
        json!({ "unread": true }),
    )
    .await
    .json();
    assert_eq!(
        notification_ids(&unread),
        [middle.to_string(), oldest.to_string()]
    );

    let outcome = call(&app, &token, "mark_all_notifications_read", json!({})).await;
    assert!(!outcome.is_error, "{}", outcome.text);
    assert_eq!(outcome.text, "ok");
    let after = call(&app, &token, "list_notifications", json!({}))
        .await
        .json();
    assert_eq!(after["unread_count"], 0);
}

#[tokio::test]
async fn notification_settings_keep_the_list_that_is_not_passed() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let project = json!({ "project": fx.project_key });

    let defaults = call(&app, &token, "get_notification_settings", project.clone())
        .await
        .json();
    assert_eq!(defaults["email_events"], json!([]));
    let in_app = defaults["in_app_events"].clone();
    assert!(
        in_app.as_array().is_some_and(|events| !events.is_empty()),
        "{defaults}"
    );

    let updated = call(
        &app,
        &token,
        "update_notification_settings",
        json!({ "project": fx.project_key, "email_events": ["assigned"] }),
    )
    .await
    .json();
    assert_eq!(updated["email_events"], json!(["assigned"]));
    assert_eq!(updated["in_app_events"], in_app, "渡さなかった側は前の値");

    let updated = call(
        &app,
        &token,
        "update_notification_settings",
        json!({ "project": fx.project_key, "in_app_events": [] }),
    )
    .await
    .json();
    assert_eq!(updated["in_app_events"], json!([]));
    assert_eq!(updated["email_events"], json!(["assigned"]));

    let outcome = call(&app, &token, "update_notification_settings", project).await;
    assert_eq!(outcome.error(), "Pass in_app_events, email_events or both");
    let outcome = call(
        &app,
        &token,
        "update_notification_settings",
        json!({ "project": fx.project_key, "email_events": ["no_such_event"] }),
    )
    .await;
    let message = outcome.error();
    assert!(message.starts_with("400 "), "{message}");
}

#[tokio::test]
async fn notification_settings_by_project_uuid_do_not_need_tenant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let second_tenant = insert_tenant(&app.state.db, fx.user.id).await;
    let token = app
        .insert_pat_in_tenants(
            fx.user.id,
            &[fx.tp.tenant_id, second_tenant],
            vec![Scope::Api],
            None,
        )
        .await;

    let settings = call(
        &app,
        &token,
        "get_notification_settings",
        json!({ "project": fx.tp.project_id.to_string() }),
    )
    .await
    .json();
    assert!(settings["in_app_events"].is_array(), "{settings}");

    let outcome = call(
        &app,
        &token,
        "get_notification_settings",
        json!({ "project": fx.project_key }),
    )
    .await;
    let message = outcome.error();
    assert!(message.ends_with("Pass `tenant`."), "{message}");
}

/// 通知設定を UUID で指すとテナントを解決しないが、束縛の外のプロジェクトは API が断る
#[tokio::test]
async fn notification_settings_outside_the_token_binding_are_403() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    // 持ち主が所属する別のテナントのプロジェクト
    let other_tenant = insert_tenant(&app.state.db, fx.user.id).await;
    let other_project = insert_extra_project(&app, other_tenant).await;
    let token = api_pat(&app, &fx).await;
    let outcome = call(
        &app,
        &token,
        "get_notification_settings",
        json!({ "project": other_project.to_string() }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");
    let outcome = call(
        &app,
        &token,
        "update_notification_settings",
        json!({ "project": other_project.to_string(), "email_events": ["assigned"] }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");

    // 同じテナントでも、許したプロジェクトの外は断る
    let allowed = insert_extra_project(&app, fx.tp.tenant_id).await;
    let narrowed = app
        .insert_pat(
            fx.user.id,
            fx.tp.tenant_id,
            vec![Scope::Api],
            Some(vec![allowed]),
        )
        .await;
    let outcome = call(
        &app,
        &narrowed,
        "get_notification_settings",
        json!({ "project": fx.tp.project_id.to_string() }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");
    let outcome = call(
        &app,
        &narrowed,
        "update_notification_settings",
        json!({ "project": fx.tp.project_id.to_string(), "in_app_events": [] }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");

    // 対照: 束縛の中のプロジェクトは通る
    call(
        &app,
        &narrowed,
        "get_notification_settings",
        json!({ "project": allowed.to_string() }),
    )
    .await
    .json();
    call(
        &app,
        &token,
        "get_notification_settings",
        json!({ "project": fx.tp.project_id.to_string() }),
    )
    .await
    .json();
}

/// 通知の一覧は read:task か read:review で通り、PAT には持つ scope の種別だけが見える
#[tokio::test]
async fn notifications_follow_the_task_and_review_scopes() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let task_notification = insert_notification(&app, &fx, 2).await;
    let review_notification = insert_notification_of(
        &app,
        &fx,
        1,
        common::notifications::TYPE_REVIEW_ROUND_CREATED,
    )
    .await;

    for (scopes, expected) in [
        (vec![Scope::ReadTask], vec![task_notification]),
        (vec![Scope::ReadReview], vec![review_notification]),
        (
            vec![Scope::Api],
            vec![review_notification, task_notification],
        ),
    ] {
        let token = pat(&app, &fx, scopes.clone()).await;
        let listed = call(&app, &token, "list_notifications", json!({}))
            .await
            .json();
        let expected: Vec<String> = expected.iter().map(Uuid::to_string).collect();
        assert_eq!(notification_ids(&listed), expected, "{scopes:?}");
    }
    let token = pat(&app, &fx, vec![Scope::ReadTenant]).await;
    let outcome = call(&app, &token, "list_notifications", json!({})).await;
    assert_eq!(outcome.error(), "403 forbidden");
}
