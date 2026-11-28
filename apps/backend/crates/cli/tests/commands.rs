//! コマンドが「どの URL へ何を送るか」を、モックサーバー越しに確かめる。

mod common;

use common::*;
use serde_json::json;
use wiremock::matchers::{
    body_json, body_partial_json, method, path, query_param, query_param_is_missing,
};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TODO_STATUS: &str = "33333333-3333-4333-8333-333333333333";
const DONE_STATUS: &str = "66666666-6666-4666-8666-666666666666";
const ALICE_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const BOB_ID: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const BUG_LABEL: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
const STALE_LABEL: &str = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";
const BUG_TYPE: &str = "c1c1c1c1-c1c1-4c1c-8c1c-c1c1c1c1c1c1";
const FEATURE_TYPE: &str = "c2c2c2c2-c2c2-4c2c-8c2c-c2c2c2c2c2c2";

fn task_type_json(id: &str, name: &str, color: &str, position: i64) -> serde_json::Value {
    json!({
        "id": id,
        "project_id": PROJECT_ID,
        "name": name,
        "color": color,
        "position": position,
    })
}

/// 並びを入れ替えて返す（CLI が position の昇順に直すことを見る）。
async fn mount_task_types(harness: &Harness, expected_calls: u64) {
    Mock::given(method("GET"))
        .and(path(project_path("task-types")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            task_type_json(FEATURE_TYPE, "機能", "#3b82f6", 1),
            task_type_json(BUG_TYPE, "バグ", "#ef4444", 0),
        ])))
        .expect(expected_calls)
        .mount(&harness.server)
        .await;
}

/// 更新・作成の本文だけを取り出す（GET を除く）。
async fn written_bodies(harness: &Harness) -> Vec<serde_json::Value> {
    harness
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() != "GET")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

fn label_json(id: &str, name: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "description": "",
        "color": "#112233",
        "icon_url": null,
        "project_id": PROJECT_ID,
    })
}

/// プロジェクトのキー解決と状態一覧は、ほぼ全てのコマンドの前段になる。
async fn mount_project_lookup(harness: &Harness) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/projects")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([project_json()])))
        .mount(&harness.server)
        .await;
}

async fn mount_statuses(harness: &Harness) {
    Mock::given(method("GET"))
        .and(path(project_path("statuses")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            status_json(TODO_STATUS, "Todo", true, false, false, 0),
            status_json(DONE_STATUS, "Complete", false, true, true, 1),
        ])))
        .mount(&harness.server)
        .await;
}

#[tokio::test]
async fn auth_whoami_reads_the_current_personal_token() {
    let harness = harness().await;
    Mock::given(method("GET"))
        .and(path("/v1/personal_tokens/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "77777777-7777-4777-8777-777777777777",
            "name": "review-bot",
            "user_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "username": "yupix",
            "tenant_id": TENANT,
            "scopes": ["read:task"],
            "allowed_project_ids": [PROJECT_ID],
            "expires_at": "2026-12-31T00:00:00Z",
        })))
        .expect(1)
        .mount(&harness.server)
        .await;

    assert_eq!(harness.run(&["task", "auth", "whoami"]).await.unwrap(), 0);
}

#[tokio::test]
async fn auth_whoami_works_without_tenant_id() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let store = task_cli::config::ConfigStore::from_home(home.path());
    store
        .save(&task_cli::config::TaskConfig {
            api_url: Some(server.uri()),
            token: Some("token-1".into()),
            tenant_id: None,
            ..Default::default()
        })
        .unwrap();

    Mock::given(method("GET"))
        .and(path("/v1/personal_tokens/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "77777777-7777-4777-8777-777777777777",
            "name": "review-bot",
            "user_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "username": "yupix",
            "tenant_id": TENANT,
            "scopes": ["read:task"],
            "allowed_project_ids": [PROJECT_ID],
            "expires_at": "2026-12-31T00:00:00Z",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let context = task_cli::Context::new(store, |_| None);
    let cli = parse(&["task", "auth", "whoami"]);
    assert_eq!(task_cli::run(cli, &context).await.unwrap(), 0);
}

#[tokio::test]
async fn auth_whoami_without_a_token_does_not_call_the_api() {
    let harness = harness().await;
    let mut config = harness.store().load().unwrap();
    config.token = None;
    harness.store().save(&config).unwrap();

    let err = harness.run(&["task", "auth", "whoami"]).await.unwrap_err();

    assert_eq!(err.exit_code, 2);
    assert!(
        err.message.contains("token (TASK_TOKEN)"),
        "{}",
        err.message
    );
    assert!(harness.sent_nothing().await);
}

#[tokio::test]
async fn config_set_persists_the_selected_key_without_calling_the_api() {
    let harness = harness().await;

    assert_eq!(
        harness
            .run(&["task", "config", "set", "tenant_id", "tenant-2"])
            .await
            .unwrap(),
        0
    );

    assert_eq!(
        harness.store().load().unwrap().tenant_id.as_deref(),
        Some("tenant-2")
    );
    assert!(harness.sent_nothing().await);
}

/// config が完全でも、env の URL とトークンの片方だけでは接続しない。
/// 補うと config のトークンが env の送り先へ（または env のトークンが config の送り先へ）飛ぶ。
#[tokio::test]
async fn a_url_or_token_alone_in_the_environment_stops_before_any_request() {
    let one_sided: [&'static [(&str, &str)]; 2] = [
        &[("TASK_API_URL", "http://127.0.0.1:9")],
        &[("TASK_TOKEN", "token-2")],
    ];
    for env in one_sided {
        let harness = harness_with(Some(TENANT), env).await;

        for args in [
            &["task", "projects", "list"][..],
            &["task", "auth", "whoami"],
        ] {
            let err = harness.run(args).await.unwrap_err();

            assert_eq!(err.exit_code, 2, "{env:?} {args:?}: {}", err.message);
            assert!(
                err.message.contains("is set without"),
                "{env:?} {args:?}: {}",
                err.message
            );
        }
        assert!(harness.sent_nothing().await, "{env:?}");
    }
}

/// 手元の設定だけを触るコマンドは接続を作らないので、片側 env でも動く。
#[tokio::test]
async fn local_commands_work_with_a_url_or_token_alone_in_the_environment() {
    let one_sided: [&'static [(&str, &str)]; 2] = [
        &[("TASK_API_URL", "http://127.0.0.1:9")],
        &[("TASK_TOKEN", "token-2")],
    ];
    for env in one_sided {
        let harness = harness_with(Some(TENANT), env).await;

        for args in [
            &["task", "config", "list"][..],
            &["task", "config", "get", "api_url"],
            &["task", "auth", "token", "token-3"],
            &["task", "auth", "logout"],
            &[
                "task",
                "config",
                "set",
                "--profile",
                "work",
                "api_url",
                "http://127.0.0.1:9",
            ],
        ] {
            assert_eq!(harness.run(args).await.unwrap(), 0, "{env:?} {args:?}");
        }
        assert_eq!(harness.store().load().unwrap().token, None, "{env:?}");
        assert!(harness.sent_nothing().await, "{env:?}");
    }
}

#[tokio::test]
async fn my_list_sends_its_filter() {
    let harness = harness().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/users/me/tasks")))
        .and(query_param("filter", "today"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "total": 0 })))
        .expect(1)
        .mount(&harness.server)
        .await;

    assert_eq!(
        harness
            .run(&["task", "my", "list", "--filter", "today"])
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn my_list_defaults_to_every_task() {
    let harness = harness().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/users/me/tasks")))
        .and(query_param("filter", "all"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "total": 0 })))
        .expect(1)
        .mount(&harness.server)
        .await;

    assert_eq!(harness.run(&["task", "my", "list"]).await.unwrap(), 0);
}

/// 既定の 1 ページ目。`limit` を送らないと API の既定（50 件）に任せることになり、
/// 続きの有無を CLI が計算できない。完了済みの除外は明示したときだけ。
#[tokio::test]
async fn my_list_sends_the_default_page() {
    let harness = harness().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/users/me/tasks")))
        .and(query_param("filter", "all"))
        .and(query_param("limit", "50"))
        .and(query_param("offset", "0"))
        .and(query_param_is_missing("done"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "total": 0 })))
        .expect(1)
        .mount(&harness.server)
        .await;

    assert_eq!(harness.run(&["task", "my", "list"]).await.unwrap(), 0);
}

/// 実バイナリを走らせて標準出力を取る。表示は `println!` に出るので、
/// 行と要約を確かめるにはプロセスを分けるしかない。
async fn run_binary(server: &MockServer, args: &[&str]) -> (bool, String) {
    run_binary_at(server, "", args).await
}

/// `api_url` を `{server}{prefix}` にして走らせる。`<画面の URL>/api` の形を作るのに使う。
/// モックは `{prefix}/v1/...` に置く。
async fn run_binary_at(server: &MockServer, prefix: &str, args: &[&str]) -> (bool, String) {
    let (status, stdout, _) = run_binary_full(server, prefix, args, None).await;
    (status, stdout)
}

/// `run_binary_at` に標準入力を渡し、標準エラーも返す。
async fn run_binary_full(
    server: &MockServer,
    prefix: &str,
    args: &[&str],
    stdin: Option<&str>,
) -> (bool, String, String) {
    run_binary_in(server, prefix, args, stdin, None).await
}

/// `run_binary_full` の cwd を選べる形。`None` は一時の HOME で走らせる（このリポジトリの
/// `.koyori-task.yaml` を拾わないため）。
async fn run_binary_in(
    server: &MockServer,
    prefix: &str,
    args: &[&str],
    stdin: Option<&str>,
    cwd: Option<std::path::PathBuf>,
) -> (bool, String, String) {
    let home = tempfile::tempdir().unwrap();
    let cwd = cwd.unwrap_or_else(|| home.path().to_path_buf());
    let uri = format!("{}{prefix}", server.uri());
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let stdin = stdin.map(str::to_string);
    let output = tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_task"));
        command
            .args(&args[1..])
            .current_dir(cwd)
            .env_clear()
            .env("HOME", home.path())
            .env("TASK_API_URL", uri)
            .env("TASK_TOKEN", "token-1")
            .env("TASK_TENANT", TENANT)
            // 実バイナリは `from_process` を通る。通知で GitHub へ出ないようにする
            .env("TASK_NO_UPDATE_CHECK", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        match stdin {
            Some(input) => {
                command.stdin(std::process::Stdio::piped());
                let mut child = command.spawn().unwrap();
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(input.as_bytes())
                    .unwrap();
                child.wait_with_output().unwrap()
            }
            None => command.output().unwrap(),
        }
    })
    .await
    .unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

/// `GET /users/me/tasks` の本物の応答に合わせた行（`first..=last` の連番）。
fn my_task_rows(first: usize, last: usize) -> Vec<serde_json::Value> {
    (first..=last)
        .map(|n| {
            json!({
                "id": "00000000-0000-4000-8000-000000000001",
                "seq_id": n,
                "seq_key": format!("ME-{n}"),
                "title": format!("task {n}"),
                "status": {
                    "id": TODO_STATUS,
                    "name": "Todo",
                    "color": "#000000",
                    "is_done_state": false,
                },
                "priority": "medium",
                "soft_deadline": null,
                "hard_deadline": null,
                "planned_start_on": null,
                "project": {
                    "id": PROJECT_ID,
                    "name": "me",
                    "key": "ME",
                    "is_personal": true,
                },
                "is_personal": true,
            })
        })
        .collect()
}

/// 担当が 246 件ある利用者が、先頭 50 件より先を取れる。200 件を越えた先のページも送れる。
/// 応答は API の契約どおり（`limit` 件で切れ、最後のページは残りだけ）にして、
/// 行が出ることと、続きの有無の要約まで確かめる。
#[tokio::test]
async fn my_list_pages_past_the_first_fifty() {
    // (引数, limit, offset, 返す行の範囲, 要約の最後の行)
    for (args, limit, offset, (first, last), summary) in [
        (
            &["task", "my", "list", "--page", "2", "--limit", "100"][..],
            "100",
            "100",
            (101, 200),
            "100 件表示 / 全 246 件（--page 3 で続き）",
        ),
        (
            &["task", "my", "list", "--page", "2", "--limit", "200"][..],
            "200",
            "200",
            (201, 246),
            "46 件表示 / 全 246 件",
        ),
    ] {
        let harness = harness().await;
        Mock::given(method("GET"))
            .and(path(format!("/v1/tenants/{TENANT}/users/me/tasks")))
            .and(query_param("limit", limit))
            .and(query_param("offset", offset))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tasks": my_task_rows(first, last),
                "total": 246,
            })))
            .expect(1)
            .mount(&harness.server)
            .await;

        let (ok, stdout) = run_binary(&harness.server, args).await;

        assert!(ok, "{args:?}: {stdout}");
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(lines.len(), last - first + 2, "{args:?}: {stdout}");
        assert_eq!(lines[0], format!("ME-{first}\ttask {first}"), "{args:?}");
        assert_eq!(
            lines[lines.len() - 2],
            format!("ME-{last}\ttask {last}"),
            "{args:?}"
        );
        assert_eq!(lines[lines.len() - 1], summary, "{args:?}");
    }
}

#[tokio::test]
async fn my_list_open_asks_for_tasks_that_are_not_done() {
    let harness = harness().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/users/me/tasks")))
        .and(query_param("done", "false"))
        .and(query_param("filter", "no_due_date"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "total": 0 })))
        .expect(1)
        .mount(&harness.server)
        .await;

    assert_eq!(
        harness
            .run(&["task", "my", "list", "--open", "--filter", "no_due_date"])
            .await
            .unwrap(),
        0
    );
}

/// サーバーは 200 を越える `limit` を黙って丸める。丸められたままページを送ると行が飛ぶので、
/// 送る前に断る（対照の `--limit 200` は `my_list_pages_past_the_first_fifty` が通す）。
#[tokio::test]
async fn my_list_rejects_a_page_size_the_server_would_round_down() {
    for args in [
        &["task", "my", "list", "--limit", "201"][..],
        &["task", "my", "list", "--limit", "0"],
        &["task", "my", "list", "--page", "0"],
    ] {
        let harness = harness().await;

        let err = harness.run(args).await.unwrap_err();

        assert_eq!(err.exit_code, 2, "{args:?}: {}", err.message);
        assert!(harness.sent_nothing().await, "{args:?}");
    }
}

#[tokio::test]
async fn projects_list_reads_the_project_index() {
    let harness = harness().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/projects")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([project_json()])))
        .expect(1)
        .mount(&harness.server)
        .await;

    assert_eq!(harness.run(&["task", "projects", "list"]).await.unwrap(), 0);
}

/// inherit_tenant_members を返さない旧いサーバーに対しても、プロジェクトを引くコマンドは通る（TASK-501）。
#[tokio::test]
async fn projects_list_reads_a_server_without_the_inheritance_field() {
    let harness = harness().await;
    let mut legacy = project_json();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("inherit_tenant_members");
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/projects")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([legacy])))
        .expect(1)
        .mount(&harness.server)
        .await;

    assert_eq!(harness.run(&["task", "projects", "list"]).await.unwrap(), 0);
}

#[tokio::test]
async fn projects_show_matches_a_key_regardless_of_case() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;

    assert_eq!(
        harness
            .run(&["task", "projects", "show", "app"])
            .await
            .unwrap(),
        0
    );
}

async fn mount_labels(harness: &Harness) {
    Mock::given(method("GET"))
        .and(path(project_path("labels")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            label_json(BUG_LABEL, "bug"),
            label_json(STALE_LABEL, "stale"),
        ])))
        .expect(1)
        .mount(&harness.server)
        .await;
}

#[tokio::test]
async fn projects_labels_lists_the_labels_of_the_resolved_project() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_labels(&harness).await;

    let code = harness
        .run(&["task", "projects", "labels", "--project", "APP"])
        .await
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(harness.calls_to(&project_path("statuses")).await, 0);
}

#[tokio::test]
async fn projects_labels_accepts_the_json_flag() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_labels(&harness).await;

    let code = harness
        .run(&["task", "--json", "projects", "labels", "--project", "APP"])
        .await
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(harness.calls_to(&project_path("labels")).await, 1);
}

#[tokio::test]
async fn projects_labels_reports_a_missing_project_without_calling_the_labels_endpoint() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;

    let err = harness
        .run(&["task", "projects", "labels", "--project", "NOPE"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 5);
    assert_eq!(harness.calls_to(&project_path("labels")).await, 0);
}

#[tokio::test]
async fn projects_show_reports_a_key_that_does_not_exist() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;

    let err = harness
        .run(&["task", "projects", "show", "NOPE"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 5);
    assert!(
        err.message.contains("Project not found: NOPE"),
        "{}",
        err.message
    );
}

#[tokio::test]
async fn sprints_list_builds_the_tenant_and_project_path_parameters() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("GET"))
        .and(path(project_path("sprints")))
        .and(query_param("status", "active"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "sprints",
            "list",
            "--project",
            "APP",
            "--status",
            "active",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn tasks_create_resolves_the_named_status_and_posts_it() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    Mock::given(method("POST"))
        .and(path(project_path("tasks")))
        .and(body_json(json!({
            "title": "Golden task",
            "description": null,
            "status_id": TODO_STATUS,
            "priority": "Medium",
            "progress_pct": null,
            "parent_task_id": null,
            "milestone_id": null,
            "task_type_id": null,
            "sprint_id": null,
            "soft_deadline": null,
            "hard_deadline": null,
            "planned_start_on": null,
            "estimated_minutes": null,
            "assignees": [],
            "label_ids": [],
            "custom_field_values": [],
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(task_detail_json()))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "tasks",
            "create",
            "--project",
            "APP",
            "--title",
            "Golden task",
            "--priority",
            "medium",
            "--status",
            "Todo",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

/// 作成 API は `status_id` を必須で受ける。省略時に送らないと必ず 400 になるので、
/// プロジェクトの既定の状態で埋める。
#[tokio::test]
async fn tasks_create_falls_back_to_the_projects_default_status() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    Mock::given(method("POST"))
        .and(path(project_path("tasks")))
        .and(body_json(json!({
            "title": "No status given",
            "description": null,
            "status_id": TODO_STATUS,
            "priority": null,
            "progress_pct": null,
            "parent_task_id": null,
            "milestone_id": null,
            "task_type_id": null,
            "sprint_id": null,
            "soft_deadline": null,
            "hard_deadline": null,
            "planned_start_on": null,
            "estimated_minutes": null,
            "assignees": [],
            "label_ids": [],
            "custom_field_values": [],
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(task_detail_json()))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "tasks",
            "create",
            "--project",
            "APP",
            "--title",
            "No status given",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn tasks_update_sends_all_changes_in_one_request() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("GET"))
        .and(path(project_path("labels")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            label_json(BUG_LABEL, "bug"),
            label_json(STALE_LABEL, "stale"),
        ])))
        .mount(&harness.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(project_path("tasks/APP-7")))
        .respond_with(ResponseTemplate::new(200).set_body_json(task_detail_json()))
        .expect(1)
        .mount(&harness.server)
        .await;
    harness
        .run(&[
            "task",
            "tasks",
            "update",
            "APP-7",
            "--title",
            "Updated",
            "--add-label",
            "bug",
            "--remove-label",
            "stale",
            "--assignee",
            ALICE_ID,
            "--assignee",
            BOB_ID,
        ])
        .await
        .unwrap();
    let requests = harness.server.received_requests().await.unwrap();
    let writes: Vec<_> = requests
        .iter()
        .filter(|r| r.method.as_str() != "GET")
        .collect();
    assert_eq!(writes.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&writes[0].body).unwrap();
    assert_eq!(body["title"], "Updated");
    assert!(body["label_ids"].is_null());
    assert_eq!(body["add_label_ids"], json!([BUG_LABEL]));
    assert_eq!(body["remove_label_ids"], json!([STALE_LABEL]));
    assert_eq!(
        body["assignees"],
        json!([
            { "user_id": ALICE_ID, "role": "assignee" },
            { "user_id": BOB_ID, "role": "assignee" },
        ])
    );
    assert!(
        !requests
            .iter()
            .any(|r| r.method.as_str() == "GET" && r.url.path().ends_with("tasks/APP-7"))
    );
}

#[tokio::test]
async fn tasks_update_clears_every_assignee_in_the_update_request() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("PUT"))
        .and(path(project_path("tasks/APP-7")))
        .respond_with(ResponseTemplate::new(200).set_body_json(task_detail_json()))
        .expect(1)
        .mount(&harness.server)
        .await;
    harness
        .run(&["task", "tasks", "update", "APP-7", "--clear-assignees"])
        .await
        .unwrap();
    let requests = harness.server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(body["assignees"], json!([]));
}

/// 途中のコマンドが失敗して空のファイルができても、本文を消さない。
#[tokio::test]
async fn tasks_update_refuses_an_empty_description_file_before_any_request() {
    let harness = harness().await;
    let dir = tempfile::tempdir().unwrap();
    // BOM・ゼロ幅の文字だけの本文も、空とみなす（`trim` は U+FEFF・U+200B を空白にしない）
    for (name, content) in [
        ("empty.md", ""),
        ("blank.md", " \n\t\n"),
        ("bom.md", "\u{feff}"),
        ("bom-newline.md", "\u{feff}\r\n"),
        ("zero-width.md", " \u{200b}\n\u{feff}\u{200d}\n"),
    ] {
        let file = dir.path().join(name);
        std::fs::write(&file, content).unwrap();
        let error = harness
            .run(&[
                "task",
                "tasks",
                "update",
                "APP-7",
                "--description-file",
                file.to_str().unwrap(),
            ])
            .await
            .unwrap_err();
        assert_eq!(error.exit_code, 2, "{name}: {}", error.message);
        assert!(
            error.message.contains("--clear-description"),
            "{}",
            error.message
        );
    }
    for inline in ["", "\u{feff}", "\u{feff} \u{200b}"] {
        let error = harness
            .run(&["task", "tasks", "update", "APP-7", "--description", inline])
            .await
            .unwrap_err();
        assert_eq!(error.exit_code, 2, "{inline:?}");
    }
    assert!(harness.sent_nothing().await);
}

/// 標準入力（`--description-file -`）から来る BOM だけの本文も、更新・作成の両方で API を叩く前に断る。
#[tokio::test]
async fn a_bom_only_description_from_stdin_is_refused_before_any_request() {
    let harness = harness().await;
    for args in [
        &[
            "task",
            "tasks",
            "update",
            "APP-7",
            "--description-file",
            "-",
        ][..],
        &[
            "task",
            "tasks",
            "create",
            "--project",
            "APP",
            "--title",
            "T",
            "--description-file",
            "-",
        ][..],
    ] {
        let (ok, stdout, stderr) =
            run_binary_full(&harness.server, "", args, Some("\u{feff}\r\n")).await;
        assert!(!ok, "{args:?}: {stdout}");
        assert!(stderr.contains("The description is empty"), "{stderr}");
    }
    assert!(harness.sent_nothing().await);
}

/// 対照: 本文を消す明示のフラグだけが、本文を空にする要求を送る。
#[tokio::test]
async fn tasks_update_clears_the_description_only_with_the_explicit_flag() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("PUT"))
        .and(path(project_path("tasks/APP-7")))
        .and(body_partial_json(json!({ "clear_description": true })))
        .respond_with(ResponseTemplate::new(200).set_body_json(task_detail_json()))
        .expect(1)
        .mount(&harness.server)
        .await;
    let code = harness
        .run(&["task", "tasks", "update", "APP-7", "--clear-description"])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn tasks_create_refuses_an_empty_description_file_before_any_request() {
    let harness = harness().await;
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in [("empty.md", ""), ("bom.md", "\u{feff}\n")] {
        let file = dir.path().join(name);
        std::fs::write(&file, content).unwrap();
        let error = harness
            .run(&[
                "task",
                "tasks",
                "create",
                "--project",
                "APP",
                "--title",
                "T",
                "--description-file",
                file.to_str().unwrap(),
            ])
            .await
            .unwrap_err();
        assert_eq!(error.exit_code, 2, "{name}: {}", error.message);
    }
    assert!(harness.sent_nothing().await);
}

fn comment_list_json() -> serde_json::Value {
    json!({
        "comments": [{
            "id": "c0000000-0000-4000-8000-000000000001",
            "user": { "id": ALICE_ID, "name": "alice", "account_kind": "human", "avatar_url": null },
            "body": "確認お願いします",
            "replies": [],
            "created_at": "2026-01-02T03:04:05Z",
            "updated_at": "2026-01-02T03:04:05Z",
            "is_deleted": false,
        }],
        "mentions": {},
    })
}

#[tokio::test]
async fn tasks_comments_reads_the_thread_endpoint() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("GET"))
        .and(path(project_path("tasks/APP-7/comments")))
        .respond_with(ResponseTemplate::new(200).set_body_json(comment_list_json()))
        .mount(&harness.server)
        .await;

    for args in [
        &["task", "tasks", "comments", "APP-7"][..],
        &["task", "--json", "tasks", "comments", "APP-7"][..],
    ] {
        assert_eq!(harness.run(args).await.unwrap(), 0);
    }
    assert_eq!(
        harness
            .calls_to(&project_path("tasks/APP-7/comments"))
            .await,
        2
    );
    let requests = harness.server.received_requests().await.unwrap();
    assert!(requests.iter().all(|r| r.method.as_str() == "GET"));
}

#[tokio::test]
async fn tasks_comments_reports_a_missing_task() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("GET"))
        .and(path(project_path("tasks/APP-9/comments")))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "message": "not found" })))
        .mount(&harness.server)
        .await;

    let err = harness
        .run(&["task", "tasks", "comments", "APP-9"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 5);
}

/// 作成の応答の `url` を引くために、`<画面の URL>/api` の形で走らせる足場。
/// プロジェクト・状態・作成のモックを `/api/v1/...` に置き、`/api/v1/tenants` は呼び側が決める。
async fn mount_create_under_api(server: &MockServer, bulk: bool) {
    let base = format!("/api/v1/tenants/{TENANT}/projects");
    Mock::given(method("GET"))
        .and(path(base.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([project_json()])))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{base}/{PROJECT_ID}/statuses")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([status_json(
            TODO_STATUS,
            "Todo",
            true,
            false,
            false,
            0
        )])))
        .mount(server)
        .await;
    let (post_path, response) = if bulk {
        (
            format!("{base}/{PROJECT_ID}/tasks/bulk-create"),
            json!({ "tasks": [task_detail_json()] }),
        )
    } else {
        (format!("{base}/{PROJECT_ID}/tasks"), task_detail_json())
    };
    Mock::given(method("POST"))
        .and(path(post_path))
        .respond_with(ResponseTemplate::new(201).set_body_json(response))
        .mount(server)
        .await;
}

async fn mount_api_tenants(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/api/v1/tenants"))
        .respond_with(response)
        .mount(server)
        .await;
}

fn tenants_ok() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!([
        { "id": TENANT, "display_id": "acme", "name": "Acme" }
    ]))
}

async fn posts_to(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.method.as_str() == "POST")
        .count()
}

/// 起票した番号を知るのに `list` を取り直さずに済むよう、作成の応答に `key` と画面の URL を入れる。
#[tokio::test]
async fn tasks_create_prints_the_key_and_url() {
    let harness = harness().await;
    let server = &harness.server;
    mount_create_under_api(server, false).await;
    mount_api_tenants(server, tenants_ok()).await;
    let create = [
        "task",
        "tasks",
        "create",
        "--project",
        "APP",
        "--title",
        "T",
    ];

    let (ok, stdout) = run_binary_at(server, "/api", &create).await;
    assert!(ok, "{stdout}");
    assert_eq!(
        stdout,
        format!("APP-7\t{}/acme/projects/APP/tasks/APP-7\n", server.uri())
    );

    let mut json_args = create.to_vec();
    json_args.insert(1, "--json");
    let (ok, stdout) = run_binary_at(server, "/api", &json_args).await;
    assert!(ok, "{stdout}");
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["key"], "APP-7");
    assert_eq!(
        value["url"],
        format!("{}/acme/projects/APP/tasks/APP-7", server.uri())
    );
    // 今までの項目はそのまま残る
    assert_eq!(value["seq_id"], 7);
    assert_eq!(value["title"], "Golden task");
}

/// `read:tenant` が無いトークンでも作成はできる。URL だけが付かない。
#[tokio::test]
async fn tasks_create_omits_the_url_when_the_token_cannot_list_tenants() {
    let harness = harness().await;
    let server = &harness.server;
    mount_create_under_api(server, false).await;
    mount_api_tenants(server, ResponseTemplate::new(403)).await;
    let create = [
        "task",
        "tasks",
        "create",
        "--project",
        "APP",
        "--title",
        "T",
    ];

    let (ok, stdout) = run_binary_at(server, "/api", &create).await;
    assert!(ok, "{stdout}");
    assert_eq!(stdout, "APP-7\n");

    let mut json_args = create.to_vec();
    json_args.insert(1, "--json");
    let (ok, stdout) = run_binary_at(server, "/api", &json_args).await;
    assert!(ok, "{stdout}");
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["key"], "APP-7");
    assert!(value["url"].is_null());
}

/// URL は付加情報。テナント一覧が 403 以外で失敗しても、作成は止めない（`url: null` で続ける）。
/// 参照は POST の前に済ませる（作ったあとに叩いて失敗すると、再実行で二重に起票する）。
#[tokio::test]
async fn tasks_create_still_posts_without_a_url_when_the_tenant_lookup_fails() {
    let harness = harness().await;
    let server = &harness.server;
    mount_create_under_api(server, false).await;
    mount_api_tenants(server, ResponseTemplate::new(500)).await;
    let create = [
        "task",
        "tasks",
        "create",
        "--project",
        "APP",
        "--title",
        "T",
    ];

    let (ok, stdout, stderr) = run_binary_full(server, "/api", &create, None).await;
    assert!(ok, "{stdout}{stderr}");
    assert_eq!(stdout, "APP-7\n");
    assert!(
        stderr.contains("warning: cannot look up the web URL"),
        "{stderr}"
    );
    assert_eq!(posts_to(server).await, 1);
    assert_tenants_before_post(server).await;

    let mut json_args = create.to_vec();
    json_args.insert(1, "--json");
    let (ok, stdout, _) = run_binary_full(server, "/api", &json_args, None).await;
    assert!(ok, "{stdout}");
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["key"], "APP-7");
    assert!(value["url"].is_null());
}

#[tokio::test]
async fn tasks_bulk_create_still_posts_without_a_url_when_the_tenant_lookup_fails() {
    let harness = harness().await;
    let server = &harness.server;
    mount_create_under_api(server, true).await;
    mount_api_tenants(server, ResponseTemplate::new(500)).await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.json");
    std::fs::write(&file, r#"[{"title": "Golden task"}]"#).unwrap();

    let (ok, stdout, stderr) = run_binary_full(
        server,
        "/api",
        &[
            "task",
            "tasks",
            "bulk-create",
            "--project",
            "APP",
            file.to_str().unwrap(),
        ],
        None,
    )
    .await;

    assert!(ok, "{stdout}{stderr}");
    assert_eq!(stdout, "APP-7\t\tGolden task\n");
    assert!(
        stderr.contains("warning: cannot look up the web URL"),
        "{stderr}"
    );
    assert_eq!(posts_to(server).await, 1);
    assert_tenants_before_post(server).await;
}

/// テナント一覧の参照は、作成の POST より前に行われている。
async fn assert_tenants_before_post(server: &MockServer) {
    let requests = server.received_requests().await.unwrap();
    let tenants = requests
        .iter()
        .position(|request| request.url.path() == "/api/v1/tenants")
        .expect("tenants was not looked up");
    let post = requests
        .iter()
        .position(|request| request.method.as_str() == "POST")
        .expect("no POST");
    assert!(tenants < post);
}

#[tokio::test]
async fn tasks_bulk_create_prints_the_key_and_url_of_each_task() {
    let harness = harness().await;
    let server = &harness.server;
    mount_create_under_api(server, true).await;
    mount_api_tenants(server, tenants_ok()).await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.json");
    std::fs::write(&file, r#"[{"title": "Golden task"}]"#).unwrap();
    let file = file.to_str().unwrap();
    let url = format!("{}/acme/projects/APP/tasks/APP-7", server.uri());

    let (ok, stdout) = run_binary_at(
        server,
        "/api",
        &["task", "tasks", "bulk-create", "--project", "APP", file],
    )
    .await;
    assert!(ok, "{stdout}");
    assert_eq!(stdout, format!("APP-7\t{url}\tGolden task\n"));

    let (ok, stdout) = run_binary_at(
        server,
        "/api",
        &[
            "task",
            "--json",
            "tasks",
            "bulk-create",
            "--project",
            "APP",
            file,
        ],
    )
    .await;
    assert!(ok, "{stdout}");
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["tasks"][0]["key"], "APP-7");
    assert_eq!(value["tasks"][0]["url"], url);
    assert_eq!(value["tasks"][0]["seq_id"], 7);
}

/// 対照: `api_url` が `<画面の URL>/api` の形でなければ、URL を作れないので何も引かない。
#[tokio::test]
async fn tasks_create_does_not_look_up_tenants_when_the_url_cannot_be_built() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    Mock::given(method("POST"))
        .and(path(project_path("tasks")))
        .respond_with(ResponseTemplate::new(201).set_body_json(task_detail_json()))
        .mount(&harness.server)
        .await;

    let (ok, stdout) = run_binary(
        &harness.server,
        &[
            "task",
            "tasks",
            "create",
            "--project",
            "APP",
            "--title",
            "T",
        ],
    )
    .await;

    assert!(ok, "{stdout}");
    assert_eq!(stdout, "APP-7\n");
    assert_eq!(harness.calls_to("/v1/tenants").await, 0);
}

/// `Context::new` には更新の相手がいない。テストの足場から GitHub へ出る経路は無い。
#[tokio::test]
async fn self_update_is_not_available_without_an_updater() {
    let harness = harness().await;

    let err = harness.run(&["task", "self-update"]).await.unwrap_err();

    assert!(
        err.message.contains("not available in this build"),
        "{}",
        err.message
    );
    assert!(harness.sent_nothing().await);
}

#[tokio::test]
async fn tasks_update_propagates_rejection_and_failure_without_further_writes() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("PUT"))
        .and(path(project_path("tasks/APP-7")))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({ "message": "update failed" })),
        )
        .expect(1)
        .mount(&harness.server)
        .await;
    let error = harness
        .run(&[
            "task",
            "tasks",
            "update",
            "APP-7",
            "--title",
            "Rejected",
            "--add-label",
            BUG_LABEL,
            "--assignee",
            ALICE_ID,
        ])
        .await
        .unwrap_err();
    assert_eq!(error.exit_code, 4);
    let requests = harness.server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
}

#[tokio::test]
async fn tasks_create_deduplicates_resolved_assignees() {
    for second in ["alice", "Alice", ALICE_ID] {
        let harness = harness().await;
        mount_project_lookup(&harness).await;
        mount_statuses(&harness).await;
        Mock::given(method("GET"))
            .and(path(project_path("assignable-users")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": ALICE_ID, "username": "alice", "avatar_url": null },
            ])))
            .mount(&harness.server)
            .await;
        Mock::given(method("POST"))
            .and(path(project_path("tasks")))
            .respond_with(ResponseTemplate::new(201).set_body_json(task_detail_json()))
            .expect(1)
            .mount(&harness.server)
            .await;
        harness
            .run(&[
                "task",
                "tasks",
                "create",
                "--project",
                "APP",
                "--title",
                "One assignee",
                "--assignee",
                "alice",
                "--assignee",
                second,
            ])
            .await
            .unwrap();
        let requests = harness.server.received_requests().await.unwrap();
        let create = requests
            .iter()
            .find(|r| r.method.as_str() == "POST")
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&create.body).unwrap();
        assert_eq!(
            body["assignees"],
            json!([{ "user_id": ALICE_ID, "role": "assignee" }])
        );
    }
}

#[tokio::test]
async fn ambiguous_assignee_names_fail_before_updating() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("GET"))
        .and(path(project_path("assignable-users")))
        .and(query_param("username", "alice"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": ALICE_ID, "username": "alice", "avatar_url": null },
            { "id": BOB_ID, "username": "alice", "avatar_url": null },
        ])))
        .expect(1)
        .mount(&harness.server)
        .await;
    let error = harness
        .run(&["task", "tasks", "update", "APP-7", "--assignee", "alice"])
        .await
        .unwrap_err();
    assert_eq!(error.exit_code, 2);
    assert!(error.message.contains(ALICE_ID));
    assert!(error.message.contains(BOB_ID));
    assert!(
        harness
            .server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method.as_str() == "GET")
    );
}

/// 一覧の絞り込みで担当者を名前で指すとき、候補の列挙（`write:task` が要る）は使わない。
/// 読むだけの PAT でも `--assignee` を名前で書けるようにする。
#[tokio::test]
async fn tasks_list_resolves_the_assignee_name_without_listing_candidates() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    Mock::given(method("GET"))
        .and(path(project_path("assignable-users")))
        .and(query_param("username", "alice"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": ALICE_ID, "username": "alice", "avatar_url": null }
        ])))
        .expect(1)
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path(project_path("tasks")))
        .and(query_param("assignee_id", ALICE_ID))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "total": 0 })))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "tasks",
            "list",
            "--project",
            "APP",
            "--assignee",
            "alice",
        ])
        .await
        .unwrap();

    assert_eq!(code, 0);
}

/// `--unassigned` は `unassigned=true` として送る。
#[tokio::test]
async fn tasks_list_sends_the_unassigned_filter() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    Mock::given(method("GET"))
        .and(path(project_path("tasks")))
        .and(query_param("unassigned", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "total": 0 })))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&["task", "tasks", "list", "--project", "APP", "--unassigned"])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

/// 一覧の絞り込みはクエリ、本文は enum の綴り。取り違えるとサーバーが 400 を返す。
#[tokio::test]
async fn tasks_list_sends_the_priority_filter_in_the_query_form() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    Mock::given(method("GET"))
        .and(path(project_path("tasks")))
        .and(query_param("priority", "critical_fire"))
        .and(query_param_is_missing("unassigned"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "total": 0 })))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "tasks",
            "list",
            "--project",
            "APP",
            "--priority",
            "critical_fire",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

const LIST_DESCRIPTION: &str = "長い本文の印がここに入る";

fn listed_task(seq_id: i64, title: &str) -> serde_json::Value {
    let mut task = task_detail_json();
    task["seq_id"] = json!(seq_id);
    task["title"] = json!(title);
    task["description"] = json!(LIST_DESCRIPTION);
    task["labels"] = json!([label_json(BUG_LABEL, "bug")]);
    task["updated_at"] = json!("2026-01-02T03:04:05Z");
    task
}

async fn mount_two_listed_tasks(harness: &Harness) {
    Mock::given(method("GET"))
        .and(path(project_path("tasks")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tasks": [listed_task(1, "最初"), listed_task(2, "次")],
            "total": 2,
            "next_cursor": null,
        })))
        .mount(&harness.server)
        .await;
}

/// タイトルで 1 件を探すだけで本文まで読ませない。`--json` に項目を並べて選べる。
#[tokio::test]
async fn tasks_list_json_fields_print_only_those_fields() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    mount_two_listed_tasks(&harness).await;

    for args in [
        // `=` なしでも、`tasks list` では次の語を項目として読む
        &[
            "task",
            "tasks",
            "list",
            "--project",
            "APP",
            "--json",
            "key,title",
        ][..],
        &[
            "task",
            "tasks",
            "list",
            "--json=key,title",
            "--project",
            "APP",
        ][..],
    ] {
        let (ok, stdout) = run_binary(&harness.server, args).await;
        assert!(ok, "{args:?}: {stdout}");
        let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(value["total"], 2, "{stdout}");
        let rows = value["tasks"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for row in rows {
            let keys: Vec<&String> = row.as_object().unwrap().keys().collect();
            assert_eq!(keys, ["key", "title"], "{stdout}");
        }
        assert_eq!(rows[0]["key"], "APP-1");
        assert_eq!(rows[1]["title"], "次");
        assert!(!stdout.contains(LIST_DESCRIPTION), "{stdout}");
    }
}

/// 項目の順は指定どおり。状態は名前で出す。
#[tokio::test]
async fn tasks_list_json_fields_keep_the_requested_order_and_name_the_status() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    mount_two_listed_tasks(&harness).await;

    let (ok, stdout) = run_binary(
        &harness.server,
        &[
            "task",
            "tasks",
            "list",
            "--project",
            "APP",
            "--json=status,key,seq_id",
        ],
    )
    .await;

    assert!(ok, "{stdout}");
    // 順は出力の文字列で見る（`serde_json::Value` は順を保たない）
    let position = |name: &str| stdout.find(&format!("\"{name}\":")).unwrap();
    assert!(position("status") < position("key"), "{stdout}");
    assert!(position("key") < position("seq_id"), "{stdout}");
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let row = value["tasks"][0].as_object().unwrap();
    assert_eq!(row.len(), 3);
    assert_eq!(row["status"], "Todo");
    assert_eq!(row["seq_id"], 1);
}

/// 既定の表では本文を出さない（長い本文が一覧の読み取りを埋めるため）。
#[tokio::test]
async fn tasks_list_does_not_print_descriptions_by_default() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    mount_two_listed_tasks(&harness).await;

    let (ok, stdout) = run_binary(
        &harness.server,
        &["task", "tasks", "list", "--project", "APP"],
    )
    .await;

    assert!(ok, "{stdout}");
    assert!(!stdout.contains(LIST_DESCRIPTION), "{stdout}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{stdout}");
    assert_eq!(lines[0], "APP-1\tTodo\t最初\tbug\t2026-01-02T03:04:05Z");
    assert_eq!(lines[1], "APP-2\tTodo\t次\tbug\t2026-01-02T03:04:05Z");
    assert_eq!(lines[2], "2 件表示 / 全 2 件");
}

/// 項目なしの `--json` は今までどおり全項目を出す（スキルが `.tasks[] | .seq_id` で読む）。
/// 足すのは `key` だけで、状態の一覧とテナントの一覧は引かない。
#[tokio::test]
async fn tasks_list_bare_json_keeps_every_field() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_two_listed_tasks(&harness).await;

    for args in [
        &["task", "tasks", "list", "--project", "APP", "--json"][..],
        &["task", "--json", "tasks", "list", "--project", "APP"][..],
    ] {
        let (ok, stdout) = run_binary(&harness.server, args).await;
        assert!(ok, "{args:?}: {stdout}");
        let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(value["total"], 2);
        let row = &value["tasks"][0];
        assert_eq!(row["key"], "APP-1");
        assert_eq!(row["seq_id"], 1);
        assert_eq!(row["description"], LIST_DESCRIPTION);
        assert!(row["status_id"].is_string());
        assert!(row.get("status").is_none());
        assert!(row.get("url").is_none());
    }
    assert_eq!(harness.calls_to(&project_path("statuses")).await, 0);
    assert_eq!(harness.calls_to("/v1/tenants").await, 0);
}

#[tokio::test]
async fn tasks_list_rejects_an_unknown_json_field_before_any_request() {
    let harness = harness().await;

    let err = harness
        .run(&[
            "task",
            "tasks",
            "list",
            "--project",
            "APP",
            "--json=key,titel",
        ])
        .await
        .unwrap_err();

    assert_eq!(err.exit_code, 2);
    assert!(
        err.message.contains("Unknown JSON field: titel"),
        "{}",
        err.message
    );
    assert!(err.message.contains("Available fields:"), "{}", err.message);
    assert!(err.message.contains("title"), "{}", err.message);
    assert!(harness.sent_nothing().await);
}

/// 項目を選べるのは `tasks list` だけ。ほかで黙って無視すると、絞れたつもりで全部読んでしまう。
#[tokio::test]
async fn json_fields_are_refused_outside_tasks_list() {
    let harness = harness().await;

    let err = harness
        .run(&["task", "projects", "list", "--json=key"])
        .await
        .unwrap_err();

    assert_eq!(err.exit_code, 2);
    assert!(err.message.contains("tasks list"), "{}", err.message);
    assert!(harness.sent_nothing().await);
}

#[tokio::test]
async fn tasks_complete_moves_the_task_to_the_done_state() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    Mock::given(method("PUT"))
        .and(path(project_path("tasks/APP-7")))
        .and(body_partial_json(json!({ "status_id": DONE_STATUS })))
        .respond_with(ResponseTemplate::new(200).set_body_json(task_detail_json()))
        .expect(1)
        .mount(&harness.server)
        .await;

    assert_eq!(
        harness
            .run(&["task", "tasks", "complete", "APP-7"])
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn tasks_delete_reports_success_from_a_body_less_response() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("DELETE"))
        .and(path(project_path("tasks/APP-7")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&harness.server)
        .await;

    assert_eq!(
        harness
            .run(&["task", "tasks", "delete", "APP-7"])
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn tasks_needs_a_project_when_the_reference_is_a_uuid() {
    let harness = harness().await;
    let err = harness
        .run(&[
            "task",
            "tasks",
            "show",
            "22222222-2222-4222-8222-222222222222",
        ])
        .await
        .unwrap_err();

    assert_eq!(err.exit_code, 2);
    assert!(
        err.message.contains("--project is required"),
        "{}",
        err.message
    );
    assert!(harness.sent_nothing().await);
}

/// 実プロセスは cwd から `.koyori-task.yaml` を探す。既定値の由来は標準エラーにだけ出し、
/// `--json` の標準出力は JSON のまま変えない。
#[tokio::test]
async fn the_binary_reads_the_project_from_the_file_in_the_current_directory() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/projects")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([project_json()])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(project_path("statuses")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(project_path("tasks")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "total": 0 })))
        .expect(1)
        .mount(&server)
        .await;
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    std::fs::write(repo.path().join(".koyori-task.yaml"), "project: APP\n").unwrap();
    let cwd = repo.path().join("sub");
    std::fs::create_dir(&cwd).unwrap();

    let (ok, stdout, stderr) = run_binary_in(
        &server,
        "",
        &["task", "--json", "tasks", "list"],
        None,
        Some(cwd),
    )
    .await;

    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("project: APP (from "), "{stderr}");
    serde_json::from_str::<serde_json::Value>(&stdout).expect("stdout stays JSON");
}

#[tokio::test]
async fn an_expired_token_and_a_forbidden_resource_exit_with_distinct_codes() {
    let harness = harness().await;
    Mock::given(method("GET"))
        .and(path("/v1/personal_tokens/me"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({ "message": "no" })))
        .mount(&harness.server)
        .await;

    let err = harness.run(&["task", "auth", "whoami"]).await.unwrap_err();
    assert_eq!(err.exit_code, 4);
}

#[tokio::test]
async fn a_response_that_no_longer_matches_the_shared_type_is_reported_not_swallowed() {
    let harness = harness().await;
    Mock::given(method("GET"))
        .and(path("/v1/personal_tokens/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "not-a-user" })))
        .mount(&harness.server)
        .await;

    let err = harness.run(&["task", "auth", "whoami"]).await.unwrap_err();
    assert!(
        err.message.contains("Cannot parse the API response")
            && err.message.contains("update to the latest release"),
        "{}",
        err.message
    );
}

#[tokio::test]
async fn tasks_list_keeps_results_when_the_url_lookup_fails() {
    let harness = harness().await;
    // Reuse the /api project fixture; list data is independent of creation.
    mount_create_under_api(&harness.server, false).await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v1/tenants/{TENANT}/projects/{PROJECT_ID}/tasks"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tasks": [task_detail_json()], "total": 1, "next_cursor": null
        })))
        .mount(&harness.server)
        .await;
    mount_api_tenants(&harness.server, ResponseTemplate::new(500)).await;
    let (ok, stdout, stderr) = run_binary_full(
        &harness.server,
        "/api",
        &[
            "task",
            "tasks",
            "list",
            "--project",
            "APP",
            "--json=key,url",
        ],
        None,
    )
    .await;
    assert!(ok, "{stdout}{stderr}");
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["tasks"][0]["key"], "APP-7");
    assert!(value["tasks"][0]["url"].is_null());
    assert!(
        stderr.contains("warning: cannot look up the web URL"),
        "{stderr}"
    );
}

#[tokio::test]
async fn projects_types_lists_name_and_color_in_position_order() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_task_types(&harness, 2).await;

    let code = harness
        .run(&["task", "projects", "types", "--project", "APP"])
        .await
        .unwrap();
    assert_eq!(code, 0);
    let code = harness
        .run(&["task", "--json", "projects", "types", "--project", "APP"])
        .await
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(harness.calls_to(&project_path("task-types")).await, 2);
}

#[tokio::test]
async fn tasks_create_resolves_the_type_name_and_posts_its_id() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    mount_task_types(&harness, 1).await;
    Mock::given(method("POST"))
        .and(path(project_path("tasks")))
        .and(body_partial_json(json!({ "task_type_id": BUG_TYPE })))
        .respond_with(ResponseTemplate::new(201).set_body_json(task_detail_json()))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "tasks",
            "create",
            "--project",
            "APP",
            "--title",
            "t",
            "--type",
            "バグ",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn tasks_create_with_an_unknown_type_lists_the_candidates_and_posts_nothing() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    mount_task_types(&harness, 1).await;

    let err = harness
        .run(&[
            "task",
            "tasks",
            "create",
            "--project",
            "APP",
            "--title",
            "t",
            "--type",
            "存在しない",
        ])
        .await
        .unwrap_err();

    assert!(err.message.contains("Type not found"), "{}", err.message);
    assert!(err.message.contains("バグ"), "{}", err.message);
    assert!(written_bodies(&harness).await.is_empty());
}

#[tokio::test]
async fn tasks_update_sends_or_clears_the_type() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    // `--type` が UUID のときは一覧を取らない
    mount_task_types(&harness, 1).await;
    Mock::given(method("PUT"))
        .and(path(project_path("tasks/APP-7")))
        .respond_with(ResponseTemplate::new(200).set_body_json(task_detail_json()))
        .expect(4)
        .mount(&harness.server)
        .await;

    for args in [
        vec!["--type", "バグ"],
        vec!["--type", FEATURE_TYPE],
        vec!["--clear-type"],
        vec!["--title", "only the title"],
    ] {
        let mut command = vec!["task", "tasks", "update", "APP-7"];
        command.extend(args);
        assert_eq!(harness.run(&command).await.unwrap(), 0);
    }

    let bodies = written_bodies(&harness).await;
    assert_eq!(bodies[0]["task_type_id"], BUG_TYPE);
    assert_eq!(bodies[0]["clear_task_type_id"], false);
    assert_eq!(bodies[1]["task_type_id"], FEATURE_TYPE);
    assert!(bodies[2]["task_type_id"].is_null());
    assert_eq!(bodies[2]["clear_task_type_id"], true);
    // 種別を指定しない更新は、種別を触らない
    assert!(bodies[3]["task_type_id"].is_null());
    assert_eq!(bodies[3]["clear_task_type_id"], false);
}

#[test]
fn a_type_and_clearing_it_cannot_be_combined() {
    let result = <task_cli::cli::Cli as clap::Parser>::try_parse_from([
        "task",
        "tasks",
        "update",
        "APP-7",
        "--type",
        "バグ",
        "--clear-type",
    ]);
    assert!(result.is_err());
}

#[tokio::test]
async fn tasks_list_filters_by_the_type() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    // 一覧の表は状態名の列を出すので、状態一覧も引く
    mount_statuses(&harness).await;
    mount_task_types(&harness, 1).await;
    Mock::given(method("GET"))
        .and(path(project_path("tasks")))
        .and(query_param("task_type_id", BUG_TYPE))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "total": 0 })))
        .expect(1)
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path(project_path("tasks")))
        .and(query_param_is_missing("task_type_id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [], "total": 0 })))
        .expect(1)
        .mount(&harness.server)
        .await;

    for args in [
        vec![
            "task",
            "tasks",
            "list",
            "--project",
            "APP",
            "--type",
            "バグ",
        ],
        vec!["task", "tasks", "list", "--project", "APP"],
    ] {
        assert_eq!(harness.run(&args).await.unwrap(), 0);
    }
}

#[tokio::test]
async fn tasks_bulk_create_resolves_each_elements_type_with_one_lookup() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_statuses(&harness).await;
    mount_task_types(&harness, 1).await;
    Mock::given(method("POST"))
        .and(path(project_path("tasks/bulk-create")))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({ "tasks": [task_detail_json()] })),
        )
        .expect(1)
        .mount(&harness.server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.json");
    std::fs::write(
        &file,
        json!([
            { "title": "a", "type": "バグ" },
            { "title": "b", "type": "機能" },
            { "title": "c" },
        ])
        .to_string(),
    )
    .unwrap();

    let code = harness
        .run(&[
            "task",
            "tasks",
            "bulk-create",
            "--project",
            "APP",
            file.to_str().unwrap(),
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);

    let bodies = written_bodies(&harness).await;
    let tasks = bodies[0]["tasks"].as_array().unwrap();
    assert_eq!(tasks[0]["task_type_id"], BUG_TYPE);
    assert_eq!(tasks[1]["task_type_id"], FEATURE_TYPE);
    assert!(tasks[2]["task_type_id"].is_null());
}
