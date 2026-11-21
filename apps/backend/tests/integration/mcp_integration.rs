//! MCP の入口（`POST /mcp`）。規則は docs/dev/features/mcp.md。
//!
//! - 入口と認証: PAT か OAuth のアクセストークンの Bearer だけを通す（OAuth は mcp_oauth_integration.rs）。資格なし・でたらめな Bearer・Cookie だけは 401、
//!   Device Token は 403、許可していない `Origin` は 403
//! - ツール: REST API を内向きに呼び直すので、scope・テナント束縛・プロジェクト束縛の拒否が
//!   そのまま `isError: true` の結果になる。拒否と対照の成功を組で確かめる

use crate::common::{
    TestApp, TestTenantProject, TestUser, create_status, insert_extra_project, insert_tenant,
};
use axum::http::StatusCode;
use backend::utils::desktop_auth::s256_challenge;
use entity::scopes::Scope;
use entity::{projects, tasks, users};
use reqwest::{Client, Response, header};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use uuid::Uuid;

const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

/// docs/dev/features/mcp.md の API の表にある 18 ツール。増減したら `tools_list_returns_every_tool` が落ちる
const TOOL_NAMES: [&str; 18] = [
    "add_task_comment",
    "carry_over_review",
    "create_task",
    "get_review_summary",
    "get_task",
    "link_pull_request",
    "list_project_options",
    "list_projects",
    "list_review_findings",
    "list_review_rounds",
    "list_task_comments",
    "list_task_pull_requests",
    "list_tasks",
    "search_tasks",
    "submit_review",
    "update_review_finding",
    "update_task",
    "whoami",
];

fn initialize_body() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "mcp-integration-test", "version": "0.0.0" },
        },
    })
}

fn tools_list_body() -> Value {
    json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })
}

pub(super) fn tool_call_body(name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments },
    })
}

/// 指定のクライアントとヘッダーで `/mcp` へ送る（Cookie・`Origin`・別の版の要求に使う）。
async fn mcp_with(
    app: &TestApp,
    client: &Client,
    token: Option<&str>,
    headers: &[(&str, &str)],
    body: Value,
) -> Response {
    let mut request = client
        .post(format!("{}/mcp", app.base_url()))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream");
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("MCP-Protocol-Version"))
    {
        request = request.header("MCP-Protocol-Version", "2025-11-25");
    }
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    request
        .body(body.to_string())
        .send()
        .await
        .expect("mcp request")
}

/// ツールの結果。`is_error` と、本文の文字列（成功は API の JSON、失敗はエラー文）。
pub(super) struct ToolOutcome {
    pub(super) is_error: bool,
    pub(super) text: String,
}

impl ToolOutcome {
    pub(super) fn json(&self) -> Value {
        assert!(!self.is_error, "ツールが失敗した: {}", self.text);
        serde_json::from_str(&self.text).expect("tool result json")
    }

    pub(super) fn error(&self) -> &str {
        assert!(self.is_error, "ツールが成功した: {}", self.text);
        &self.text
    }
}

async fn outcome_of(response: Response) -> ToolOutcome {
    let status = response.status();
    let body: Value = response.json().await.expect("mcp json");
    assert_eq!(status, StatusCode::OK, "tools/call: {body}");
    let result = &body["result"];
    assert!(result.is_object(), "tools/call に result が無い: {body}");
    ToolOutcome {
        is_error: result["isError"].as_bool().unwrap_or(false),
        text: result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("content[0].text が無い: {body}"))
            .to_string(),
    }
}

pub(super) async fn call(app: &TestApp, token: &str, name: &str, arguments: Value) -> ToolOutcome {
    outcome_of(app.mcp(Some(token), tool_call_body(name, arguments)).await).await
}

pub(super) struct Fixture {
    pub(super) user: TestUser,
    pub(super) tp: TestTenantProject,
    pub(super) project_key: String,
    todo: Uuid,
    done: Uuid,
}

/// ユーザー・テナント・プロジェクトと状態 2 つ（`Todo` が既定、`Done` が完了）を作る。
/// 状態の作成にセッションが要るので、`app` のクライアントはログイン済みになる。
pub(super) async fn fixture(app: &mut TestApp) -> Fixture {
    let user = app.insert_user_default().await;
    let tp = app.insert_tenant_project(user.id).await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let todo = create_status(app, &tp, "Todo", false).await;
    let done = create_status(app, &tp, "Done", true).await;
    let project_key = project_key(app, tp.project_id).await;
    Fixture {
        user,
        tp,
        project_key,
        todo,
        done,
    }
}

async fn project_key(app: &TestApp, project_id: Uuid) -> String {
    projects::Entity::find_by_id(project_id)
        .one(&app.state.db)
        .await
        .expect("find project")
        .expect("project")
        .key
}

pub(super) async fn api_pat(app: &TestApp, fx: &Fixture) -> String {
    app.insert_pat(fx.user.id, fx.tp.tenant_id, vec![Scope::Api], None)
        .await
}

/// `create_task` で 1 件作り、(`KEY-N`, UUID) を返す。
async fn create_task(app: &TestApp, token: &str, fx: &Fixture, title: &str) -> (String, String) {
    let created = call(
        app,
        token,
        "create_task",
        json!({ "project": fx.project_key, "title": title }),
    )
    .await
    .json();
    let id = created["id"].as_str().expect("task id").to_string();
    let seq = created["seq_id"].as_i64().expect("seq_id");
    (format!("{}-{seq}", fx.project_key), id)
}

/// ログインから交換まで通し、Device Token の平文を返す（desktop_auth_integration.rs と同じ流れ）。
async fn device_token(app: &mut TestApp, user: &TestUser) -> String {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let res = app
        .post_json_with_session(
            "/v1/desktop/auth/codes",
            json!({ "code_challenge": s256_challenge(VERIFIER), "name": "laptop" }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "認可コードを発行できる");
    let code = res.json::<Value>().await.expect("code body")["code"]
        .as_str()
        .expect("code")
        .to_string();
    let res = Client::new()
        .post(format!("{}/v1/desktop/auth/token", app.base_url()))
        .header("x-forwarded-for", Uuid::new_v4().to_string())
        .json(&json!({ "code": code, "code_verifier": VERIFIER }))
        .send()
        .await
        .expect("token request");
    assert_eq!(res.status(), StatusCode::CREATED, "交換できる");
    res.json::<Value>().await.expect("token body")["token"]
        .as_str()
        .expect("token")
        .to_string()
}

// ---- 入口と認証 ----

#[tokio::test]
async fn request_without_credentials_is_401_with_www_authenticate() {
    let app = TestApp::new().await;

    let res = app.mcp(None, initialize_body()).await;

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    // 資格情報なし: 認可サーバーの見つけ方と、要る scope を返す。`error` は付けない
    let origin = app.state.settings.public_origin();
    assert_eq!(
        res.headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some(
            format!(
                "Bearer resource_metadata=\"{origin}/.well-known/oauth-protected-resource/api/mcp\", scope=\"read:tenant read:project read:task write:task read:milestone read:sprint read:review write:review\""
            )
            .as_str()
        )
    );
    let body: Value = res.json().await.expect("json");
    assert_eq!(body, json!({ "message": "unauthorized" }));
}

#[tokio::test]
async fn unknown_bearer_is_401() {
    let app = TestApp::new().await;

    let res = app.mcp(Some("not-a-real-token"), initialize_body()).await;

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    // 無効な Bearer: `error="invalid_token"` を足す
    let origin = app.state.settings.public_origin();
    assert_eq!(
        res.headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some(
            format!(
                "Bearer error=\"invalid_token\", resource_metadata=\"{origin}/.well-known/oauth-protected-resource/api/mcp\", scope=\"read:tenant read:project read:task write:task read:milestone read:sprint read:review write:review\""
            )
            .as_str()
        )
    );
    let body: Value = res.json().await.expect("json");
    assert_eq!(body, json!({ "message": "unauthorized" }));
}

#[tokio::test]
async fn session_cookie_alone_is_401() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;

    // 対照: 同じ Cookie は REST API では通る
    let rest = app.get_with_session("/v1/tenants").await;
    assert_eq!(rest.status(), StatusCode::OK);

    let res = mcp_with(&app, &app.session_client(), None, &[], initialize_body()).await;

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body: Value = res.json().await.expect("json");
    assert_eq!(body, json!({ "message": "unauthorized" }));
}

#[tokio::test]
async fn device_token_is_403() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let token = device_token(&mut app, &user).await;

    // 対照: 同じ Device Token は REST API では通る
    let rest = app.get_with_bearer("/v1/auth/me", &token).await;
    assert_eq!(rest.status(), StatusCode::OK);

    let res = app.mcp(Some(&token), initialize_body()).await;

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body: Value = res.json().await.expect("json");
    assert_eq!(body, json!({ "message": "forbidden" }));
}

#[tokio::test]
async fn initialize_with_pat_names_the_server_and_accepts_the_initialized_notification() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let res = app.mcp(Some(&token), initialize_body()).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    let body: Value = res.json().await.expect("json");
    assert_eq!(body["result"]["serverInfo"]["name"], "task", "{body}");
    assert!(
        body["result"]["capabilities"]["tools"].is_object(),
        "{body}"
    );

    let res = app
        .mcp(
            Some(&token),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn get_and_delete_are_405() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let url = format!("{}/mcp", app.base_url());

    let res = Client::new()
        .get(&url)
        .header(header::ACCEPT, "text/event-stream")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .expect("get mcp");
    assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);

    let res = Client::new()
        .delete(&url)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .expect("delete mcp");
    assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn tools_list_returns_every_tool() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let res = app.mcp(Some(&token), tools_list_body()).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: Value = res.json().await.expect("json");
    let tools = body["result"]["tools"].as_array().expect("tools");
    let mut names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    names.sort_unstable();
    assert_eq!(names, TOOL_NAMES);

    // 説明と引数のスキーマはクライアントの AI が読むので英語で書く。引数の型の `///` は
    // スキーマの `description` になるので、実装の覚え書き（日本語）を書くとここに出る
    for tool in tools {
        let text = tool.to_string();
        let foreign: String = text.chars().filter(|c| !c.is_ascii()).take(40).collect();
        assert!(foreign.is_empty(), "{}: {foreign}", tool["name"]);
    }

    // 読み取りのツールには readOnlyHint が付き、書き込みのツールには付かない
    let read_only = |name: &str| {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .expect("tool")["annotations"]["readOnlyHint"]
            .as_bool()
    };
    assert_eq!(read_only("get_task"), Some(true));
    assert_ne!(read_only("create_task"), Some(true));
    assert_eq!(read_only("list_review_findings"), Some(true));
    assert_ne!(read_only("submit_review"), Some(true));
}

#[tokio::test]
async fn foreign_origin_is_403_and_the_frontend_origin_is_allowed() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let client = Client::new();

    let res = mcp_with(
        &app,
        &client,
        Some(&token),
        &[("Origin", "https://evil.example")],
        tools_list_body(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    let allowed = app.state.settings.allow_origin.clone();
    let res = mcp_with(
        &app,
        &client,
        Some(&token),
        &[("Origin", &allowed)],
        tools_list_body(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn tools_list_works_in_the_2026_07_28_form() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let res = mcp_with(
        &app,
        &Client::new(),
        Some(&token),
        &[
            ("MCP-Protocol-Version", "2026-07-28"),
            ("Mcp-Method", "tools/list"),
        ],
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {},
                    "io.modelcontextprotocol/clientInfo": {
                        "name": "mcp-integration-test",
                        "version": "0.0.0",
                    },
                },
            },
        }),
    )
    .await;

    let status = res.status();
    let body: Value = res.json().await.expect("json");
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"]["tools"].as_array().map(Vec::len),
        Some(TOOL_NAMES.len()),
        "{body}"
    );
}

// ---- ツール ----

#[tokio::test]
async fn whoami_returns_the_token_identity() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let identity = call(&app, &token, "whoami", json!({})).await.json();

    assert_eq!(identity["user_id"], fx.user.id.to_string());
    assert_eq!(identity["tenant_ids"], json!([fx.tp.tenant_id.to_string()]));
}

#[tokio::test]
async fn create_task_uses_the_default_status_and_get_task_reads_it_back() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let created = call(
        &app,
        &token,
        "create_task",
        json!({
            "project": fx.project_key,
            "title": "MCP から作る",
            "description": "本文",
            "priority": "High",
            "soft_deadline": "2026-12-01",
        }),
    )
    .await
    .json();
    assert_eq!(created["status_id"], fx.todo.to_string());
    assert_eq!(created["priority"], "High");
    let id: Uuid = created["id"].as_str().expect("id").parse().expect("uuid");
    let reference = format!("{}-{}", fx.project_key, created["seq_id"]);

    let row = tasks::Entity::find_by_id(id)
        .one(&app.state.db)
        .await
        .expect("find task")
        .expect("task row");
    assert_eq!(row.title, "MCP から作る");
    assert_eq!(row.status_id, fx.todo);
    assert_eq!(row.project_id, fx.tp.project_id);

    let fetched = call(&app, &token, "get_task", json!({ "task": reference }))
        .await
        .json();
    assert_eq!(fetched["id"], id.to_string());
    assert_eq!(fetched["title"], "MCP から作る");

    // UUID でも、プロジェクトを添えれば引ける
    let by_uuid = call(
        &app,
        &token,
        "get_task",
        json!({ "task": id.to_string(), "project": fx.project_key }),
    )
    .await
    .json();
    assert_eq!(by_uuid["id"], id.to_string());
}

#[tokio::test]
async fn create_task_rejects_malformed_arguments_without_creating() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let outcome = call(
        &app,
        &token,
        "create_task",
        json!({ "project": fx.project_key, "title": "x", "parent_task_id": "x" }),
    )
    .await;
    assert_eq!(outcome.error(), "parent_task_id must be a UUID");

    // priority の検証は API に任せる。API の拒否がそのまま返る
    let outcome = call(
        &app,
        &token,
        "create_task",
        json!({ "project": fx.project_key, "title": "x", "priority": "urgent" }),
    )
    .await;
    assert!(outcome.error().starts_with("4"), "{}", outcome.error());

    let count = tasks::Entity::find()
        .filter(tasks::Column::ProjectId.eq(fx.tp.project_id))
        .all(&app.state.db)
        .await
        .expect("tasks")
        .len();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn update_task_resolves_the_status_by_name() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let (reference, id) = create_task(&app, &token, &fx, "状態を変える").await;

    let updated = call(
        &app,
        &token,
        "update_task",
        json!({ "task": reference, "status": "done", "title": "状態を変えた" }),
    )
    .await
    .json();
    assert_eq!(updated["status_id"], fx.done.to_string());
    assert_eq!(updated["title"], "状態を変えた");
    let row = tasks::Entity::find_by_id(id.parse::<Uuid>().expect("uuid"))
        .one(&app.state.db)
        .await
        .expect("find task")
        .expect("task row");
    assert_eq!(row.status_id, fx.done);

    // 存在しない名前は、候補の名前を添えて断る
    let outcome = call(
        &app,
        &token,
        "update_task",
        json!({ "task": reference, "status": "Nope" }),
    )
    .await;
    assert_eq!(
        outcome.error(),
        "Status not found: Nope (this project has Todo, Done)"
    );
}

#[tokio::test]
async fn add_task_comment_shows_up_in_list_task_comments() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let (reference, _) = create_task(&app, &token, &fx, "コメントする").await;

    let comment = call(
        &app,
        &token,
        "add_task_comment",
        json!({ "task": reference, "body": "MCP からのコメント" }),
    )
    .await
    .json();
    assert_eq!(comment["body"], "MCP からのコメント");

    let listed = call(
        &app,
        &token,
        "list_task_comments",
        json!({ "task": reference }),
    )
    .await
    .json();
    let comments = listed["comments"].as_array().expect("comments");
    assert_eq!(comments.len(), 1, "{listed}");
    assert!(
        listed.to_string().contains("MCP からのコメント"),
        "{listed}"
    );
}

#[tokio::test]
async fn list_tasks_and_search_tasks_return_the_created_task() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let (_, first) = create_task(&app, &token, &fx, "OAuth login flow").await;
    let (_, second) = create_task(&app, &token, &fx, "Billing export").await;

    let listed = call(
        &app,
        &token,
        "list_tasks",
        json!({ "project": fx.project_key }),
    )
    .await
    .json();
    assert_eq!(listed["total"], 2, "{listed}");
    let mut ids: Vec<&str> = listed["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|task| task["id"].as_str().expect("id"))
        .collect();
    ids.sort_unstable();
    let mut expected = [first.as_str(), second.as_str()];
    expected.sort_unstable();
    assert_eq!(ids, expected);

    // limit と offset はクエリへそのまま渡る
    let page = call(
        &app,
        &token,
        "list_tasks",
        json!({ "project": fx.project_key, "limit": 1, "offset": 1 }),
    )
    .await
    .json();
    assert_eq!(page["tasks"].as_array().map(Vec::len), Some(1), "{page}");
    assert_eq!(page["total"], 2, "{page}");

    let hits = call(
        &app,
        &token,
        "search_tasks",
        json!({ "project": fx.project_key, "q": "OAuth" }),
    )
    .await
    .json();
    let hit_ids: Vec<&str> = hits["tasks"]
        .as_array()
        .expect("hits")
        .iter()
        .map(|task| task["id"].as_str().expect("id"))
        .collect();
    assert_eq!(hit_ids, [first.as_str()], "{hits}");
}

#[tokio::test]
async fn list_project_options_returns_every_kind() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    for kind in [
        "statuses",
        "labels",
        "task_types",
        "milestones",
        "sprints",
        "assignable_users",
    ] {
        let options = call(
            &app,
            &token,
            "list_project_options",
            json!({ "project": fx.project_key, "kind": kind }),
        )
        .await
        .json();
        assert!(options.is_array(), "{kind}: {options}");
    }

    let statuses = call(
        &app,
        &token,
        "list_project_options",
        json!({ "project": fx.project_key, "kind": "statuses" }),
    )
    .await
    .json();
    let names: Vec<&str> = statuses
        .as_array()
        .expect("statuses")
        .iter()
        .map(|status| status["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["Todo", "Done"]);

    // 未知の kind は引数の誤りとして返る
    let outcome = call(
        &app,
        &token,
        "list_project_options",
        json!({ "project": fx.project_key, "kind": "nope" }),
    )
    .await;
    assert!(outcome.is_error, "{}", outcome.text);
}

#[tokio::test]
async fn write_tool_is_403_without_the_write_scope() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let writer = api_pat(&app, &fx).await;
    let (reference, id) = create_task(&app, &writer, &fx, "読むだけ").await;
    let reader = app
        .insert_pat(
            fx.user.id,
            fx.tp.tenant_id,
            vec![Scope::ReadProject, Scope::ReadTask],
            None,
        )
        .await;

    let outcome = call(
        &app,
        &reader,
        "create_task",
        json!({ "project": fx.project_key, "title": "書けない" }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");

    // 対照: 同じ PAT で読み取りは通る
    let fetched = call(&app, &reader, "get_task", json!({ "task": reference }))
        .await
        .json();
    assert_eq!(fetched["id"], id);
}

#[tokio::test]
async fn resolving_a_key_needs_read_project_and_uuids_do_not() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let writer = api_pat(&app, &fx).await;
    let (reference, id) = create_task(&app, &writer, &fx, "UUID で読む").await;
    let reader = app
        .insert_pat(fx.user.id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;

    let outcome = call(&app, &reader, "get_task", json!({ "task": reference })).await;
    assert_eq!(
        outcome.error(),
        format!(
            "403 forbidden (could not look up project key \"{}\": the token may lack the read:project scope, or it is not allowed in this tenant; if the scope is missing, pass the project UUID instead)",
            fx.project_key
        )
    );

    let fetched = call(
        &app,
        &reader,
        "get_task",
        json!({ "task": id, "project": fx.tp.project_id.to_string() }),
    )
    .await
    .json();
    assert_eq!(fetched["id"], id);
}

#[tokio::test]
async fn tenant_outside_the_token_binding_is_403() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    // 持ち主は同じ。束縛していないテナントは、所属していても PAT では触れない
    let other_tenant = insert_tenant(&app.state.db, fx.user.id).await;

    let outcome = call(
        &app,
        &token,
        "list_projects",
        json!({ "tenant": other_tenant.to_string() }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");

    let projects = call(
        &app,
        &token,
        "list_projects",
        json!({ "tenant": fx.tp.tenant_id.to_string() }),
    )
    .await
    .json();
    let keys: Vec<&str> = projects
        .as_array()
        .expect("projects")
        .iter()
        .map(|project| project["key"].as_str().expect("key"))
        .collect();
    assert_eq!(keys, [fx.project_key.as_str()]);
}

#[tokio::test]
async fn project_outside_allowed_project_ids_is_denied() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let writer = api_pat(&app, &fx).await;
    let (_, allowed_task) = create_task(&app, &writer, &fx, "許可した側").await;
    let other_project = crate::common::insert_extra_project(&app, fx.tp.tenant_id).await;
    let other = TestTenantProject {
        tenant_id: fx.tp.tenant_id,
        project_id: other_project,
    };
    let other_status = create_status(&app, &other, "Todo", false).await;
    let other_task = call(
        &app,
        &writer,
        "create_task",
        json!({
            "project": other_project.to_string(),
            "title": "許可していない側",
            "status": other_status.to_string(),
        }),
    )
    .await
    .json();
    let other_task = other_task["id"].as_str().expect("id").to_string();
    let scoped = app
        .insert_pat(
            fx.user.id,
            fx.tp.tenant_id,
            vec![Scope::Api],
            Some(vec![fx.tp.project_id]),
        )
        .await;

    let outcome = call(
        &app,
        &scoped,
        "get_task",
        json!({ "task": other_task, "project": other_project.to_string() }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");

    let fetched = call(
        &app,
        &scoped,
        "get_task",
        json!({ "task": allowed_task, "project": fx.tp.project_id.to_string() }),
    )
    .await
    .json();
    assert_eq!(fetched["id"], allowed_task);
}

#[tokio::test]
async fn token_bound_to_several_tenants_must_pass_tenant() {
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

    let outcome = call(&app, &token, "list_projects", json!({})).await;
    let message = outcome.error();
    assert!(
        message.starts_with("This token can be used in 2 tenants: "),
        "{message}"
    );
    assert!(message.ends_with("Pass `tenant`."), "{message}");
    for tenant in [fx.tp.tenant_id, second_tenant] {
        assert!(message.contains(&tenant.to_string()), "{message}");
    }

    let projects = call(
        &app,
        &token,
        "list_projects",
        json!({ "tenant": fx.tp.tenant_id.to_string() }),
    )
    .await
    .json();
    assert_eq!(projects.as_array().map(Vec::len), Some(1), "{projects}");
}

#[tokio::test]
async fn tenant_can_be_passed_by_name() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let display_id = entity::tenants::Entity::find_by_id(fx.tp.tenant_id)
        .one(&app.state.db)
        .await
        .expect("find tenant")
        .expect("tenant")
        .display_id;

    let projects = call(
        &app,
        &token,
        "list_projects",
        json!({ "tenant": display_id }),
    )
    .await
    .json();
    assert_eq!(projects.as_array().map(Vec::len), Some(1), "{projects}");

    // 名前を引くには read:tenant が要る。無ければ原因を添えて断る
    let narrow = app
        .insert_pat(fx.user.id, fx.tp.tenant_id, vec![Scope::ReadProject], None)
        .await;
    let outcome = call(
        &app,
        &narrow,
        "list_projects",
        json!({ "tenant": display_id }),
    )
    .await;
    assert_eq!(
        outcome.error(),
        format!(
            "403 forbidden (could not look up tenant \"{display_id}\" by name: the token may lack the read:tenant scope; if so, pass the tenant UUID instead)"
        )
    );
}

/// プロジェクトのキーは大文字小文字を区別しない。`KEY-N` も同じ綴りで通る
#[tokio::test]
async fn lowercase_task_reference_resolves_to_the_same_task() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let (reference, id) = create_task(&app, &token, &fx, "小文字で指す").await;

    let fetched = call(
        &app,
        &token,
        "get_task",
        json!({ "task": reference.to_lowercase() }),
    )
    .await
    .json();
    assert_eq!(fetched["id"], id);

    // 対照: 番号が違えば 404 のまま
    let outcome = call(
        &app,
        &token,
        "get_task",
        json!({ "task": format!("{}-99999", fx.project_key.to_lowercase()) }),
    )
    .await;
    assert_eq!(outcome.error(), "404 not-found");
}

/// 使わない引数に空文字を入れてくるクライアントがある。空文字は省略として読む
#[tokio::test]
async fn blank_optional_arguments_are_treated_as_omitted() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    // read:tenant が無いので、空文字を名前として引くと 403 になる PAT
    let token = app
        .insert_pat(
            fx.user.id,
            fx.tp.tenant_id,
            vec![Scope::ReadProject, Scope::WriteTask],
            None,
        )
        .await;

    let created = call(
        &app,
        &token,
        "create_task",
        json!({
            "project": fx.project_key,
            "title": "空文字の引数",
            "tenant": "",
            "status": " ",
            "priority": "",
            "parent_task_id": "",
        }),
    )
    .await
    .json();
    assert_eq!(created["status_id"], fx.todo.to_string());

    let reference = format!("{}-{}", fx.project_key, created["seq_id"]);
    let fetched = call(
        &app,
        &token,
        "get_task",
        json!({ "task": reference, "project": "", "tenant": "" }),
    )
    .await
    .json();
    assert_eq!(fetched["id"], created["id"]);
}

/// 解決の 403 は scope の不足とは限らない。束縛の外のテナントでは、scope があっても 403 になる
#[tokio::test]
async fn forbidden_while_resolving_does_not_claim_the_scope_is_missing() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let other_tenant = insert_tenant(&app.state.db, fx.user.id).await;

    let outcome = call(
        &app,
        &token,
        "get_task",
        json!({ "task": "KEY-1", "tenant": other_tenant.to_string() }),
    )
    .await;

    let message = outcome.error();
    assert!(message.starts_with("403 forbidden ("), "{message}");
    assert!(
        message.contains("or it is not allowed in this tenant"),
        "{message}"
    );
    assert!(!message.contains("needs the read:project"), "{message}");
}

#[tokio::test]
async fn task_uuid_without_project_is_rejected() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let (_, id) = create_task(&app, &token, &fx, "UUID だけ").await;

    let outcome = call(&app, &token, "get_task", json!({ "task": id })).await;

    assert_eq!(outcome.error(), "project is required when task is a UUID");
}

#[tokio::test]
async fn link_pull_request_without_github_integration_is_409() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let (reference, _) = create_task(&app, &token, &fx, "PR をつなぐ").await;

    let outcome = call(
        &app,
        &token,
        "link_pull_request",
        json!({ "task": reference, "number": 12 }),
    )
    .await;
    assert_eq!(outcome.error(), "409 github-integration-missing");

    // 対照: 連携が無くても、リンクの一覧は読める
    let forge = call(
        &app,
        &token,
        "list_task_pull_requests",
        json!({ "task": reference }),
    )
    .await
    .json();
    assert!(forge.is_object(), "{forge}");
}

#[tokio::test]
async fn missing_task_is_404() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let outcome = call(
        &app,
        &token,
        "get_task",
        json!({ "task": format!("{}-99999", fx.project_key) }),
    )
    .await;

    assert_eq!(outcome.error(), "404 not-found");
}

/// PAT とセッション Cookie を両方付けても、判定は PAT の scope で行う。
/// 内向きの呼び出しへ Cookie を写さないこと自体は、`handler` の単体テスト
/// （`mcp::inner` の `only_authorization_is_copied_from_the_outer_request`）で固定している
#[tokio::test]
async fn session_cookie_does_not_widen_the_token_scope() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let reader = app
        .insert_pat(
            fx.user.id,
            fx.tp.tenant_id,
            vec![Scope::ReadProject, Scope::ReadTask],
            None,
        )
        .await;
    let arguments = json!({ "project": fx.project_key, "title": "Cookie では書けない" });

    // 対照: 同じ Cookie は REST API ではタスクを作れる
    let rest = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/tasks",
                fx.tp.tenant_id, fx.tp.project_id
            ),
            json!({ "title": "セッションで作る", "status_id": fx.todo }),
        )
        .await;
    assert_eq!(rest.status(), StatusCode::CREATED);

    let outcome = outcome_of(
        mcp_with(
            &app,
            &app.session_client(),
            Some(&reader),
            &[],
            tool_call_body("create_task", arguments),
        )
        .await,
    )
    .await;

    assert_eq!(outcome.error(), "403 forbidden");
}

/// プロジェクトの下に 1 件作り、ID を返す（セッションで REST API に送る）。
async fn create_option(app: &TestApp, fx: &Fixture, collection: &str, body: Value) -> Uuid {
    let response = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/{collection}",
                fx.tp.tenant_id, fx.tp.project_id
            ),
            body,
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "create {collection}"
    );
    let created: Value = response.json().await.expect("created json");
    created["id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("created id")
}

async fn create_label(app: &TestApp, fx: &Fixture, name: &str) -> Uuid {
    create_option(
        app,
        fx,
        "labels",
        json!({ "name": name, "color": "#336699" }),
    )
    .await
}

async fn create_task_type(app: &TestApp, fx: &Fixture, name: &str) -> Uuid {
    create_option(
        app,
        fx,
        "task-types",
        json!({ "name": name, "color": "#336699" }),
    )
    .await
}

async fn create_milestone(app: &TestApp, fx: &Fixture, name: &str) -> Uuid {
    create_option(
        app,
        fx,
        "milestones",
        json!({ "name": name, "due_date": "2026-12-31" }),
    )
    .await
}

async fn create_sprint(app: &TestApp, fx: &Fixture, name: &str) -> Uuid {
    create_option(
        app,
        fx,
        "sprints",
        json!({ "name": name, "start_date": "2026-12-01", "end_date": "2026-12-14" }),
    )
    .await
}

async fn username(app: &TestApp, user_id: Uuid) -> String {
    users::Entity::find_by_id(user_id)
        .one(&app.state.db)
        .await
        .expect("find user")
        .expect("user")
        .username
}

async fn task_count(app: &TestApp, fx: &Fixture) -> usize {
    tasks::Entity::find()
        .filter(tasks::Column::ProjectId.eq(fx.tp.project_id))
        .all(&app.state.db)
        .await
        .expect("tasks")
        .len()
}

fn ids_of(items: &Value, pick: impl Fn(&Value) -> &Value) -> Vec<String> {
    let mut ids: Vec<String> = items
        .as_array()
        .expect("array")
        .iter()
        .map(|item| pick(item).as_str().expect("id").to_string())
        .collect();
    ids.sort();
    ids
}

fn sorted(ids: &[Uuid]) -> Vec<String> {
    let mut ids: Vec<String> = ids.iter().map(Uuid::to_string).collect();
    ids.sort();
    ids
}

async fn get_task_json(app: &TestApp, token: &str, created: &Value, fx: &Fixture) -> Value {
    call(
        app,
        token,
        "get_task",
        json!({ "task": format!("{}-{}", fx.project_key, created["seq_id"]) }),
    )
    .await
    .json()
}

#[tokio::test]
async fn create_task_takes_names_for_labels_assignee_type_milestone_and_sprint() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let bug = create_label(&app, &fx, "bug").await;
    let ui = create_label(&app, &fx, "ui").await;
    let feature = create_task_type(&app, &fx, "Feature").await;
    let v1 = create_milestone(&app, &fx, "v1").await;
    let week1 = create_sprint(&app, &fx, "week-1").await;
    let me = username(&app, fx.user.id).await;

    let created = call(
        &app,
        &token,
        "create_task",
        json!({
            "project": fx.project_key,
            "title": "名前で渡す",
            "labels": ["bug", "ui"],
            "assignees": [me],
            "task_type": "Feature",
            "milestone": "v1",
            "sprint": "week-1",
        }),
    )
    .await
    .json();

    let fetched = get_task_json(&app, &token, &created, &fx).await;
    assert_eq!(
        ids_of(&fetched["labels"], |label| &label["id"]),
        sorted(&[bug, ui])
    );
    assert_eq!(
        ids_of(&fetched["assignees"], |assignee| &assignee["user"]["id"]),
        vec![fx.user.id.to_string()]
    );
    assert_eq!(fetched["task_type"]["id"], feature.to_string());
    assert_eq!(fetched["milestone_id"], v1.to_string());
    assert_eq!(fetched["sprint_id"], week1.to_string());
}

#[tokio::test]
async fn create_task_matches_names_ignoring_case_and_passes_uuids_through() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let bug = create_label(&app, &fx, "bug").await;
    let ui = create_label(&app, &fx, "ui").await;
    let me = username(&app, fx.user.id).await;

    let created = call(
        &app,
        &token,
        "create_task",
        json!({
            "project": fx.project_key,
            "title": "大文字小文字と UUID",
            "labels": ["BUG", ui.to_string()],
            // 同じ人を名前と UUID で 2 回渡しても 1 人になる
            "assignees": [me.to_uppercase(), fx.user.id.to_string()],
        }),
    )
    .await
    .json();

    let fetched = get_task_json(&app, &token, &created, &fx).await;
    assert_eq!(
        ids_of(&fetched["labels"], |label| &label["id"]),
        sorted(&[bug, ui])
    );
    assert_eq!(
        ids_of(&fetched["assignees"], |assignee| &assignee["user"]["id"]),
        vec![fx.user.id.to_string()]
    );
}

#[tokio::test]
async fn update_task_adds_and_removes_labels_by_name() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let bug = create_label(&app, &fx, "bug").await;
    let ui = create_label(&app, &fx, "ui").await;
    let (reference, _) = create_task(&app, &token, &fx, "ラベルを付け外す").await;

    call(
        &app,
        &token,
        "update_task",
        json!({ "task": reference, "add_labels": ["bug", "UI"] }),
    )
    .await
    .json();
    let fetched = call(&app, &token, "get_task", json!({ "task": reference }))
        .await
        .json();
    assert_eq!(
        ids_of(&fetched["labels"], |label| &label["id"]),
        sorted(&[bug, ui])
    );

    call(
        &app,
        &token,
        "update_task",
        json!({ "task": reference, "remove_labels": ["bug"] }),
    )
    .await
    .json();
    let fetched = call(&app, &token, "get_task", json!({ "task": reference }))
        .await
        .json();
    assert_eq!(
        ids_of(&fetched["labels"], |label| &label["id"]),
        sorted(&[ui])
    );

    // 旧名の引数も受ける
    call(
        &app,
        &token,
        "update_task",
        json!({ "task": reference, "add_label_ids": [bug.to_string()] }),
    )
    .await
    .json();
    let fetched = call(&app, &token, "get_task", json!({ "task": reference }))
        .await
        .json();
    assert_eq!(
        ids_of(&fetched["labels"], |label| &label["id"]),
        sorted(&[bug, ui])
    );
}

#[tokio::test]
async fn unknown_label_name_lists_the_candidates_without_creating() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    create_label(&app, &fx, "bug").await;
    create_task_type(&app, &fx, "Feature").await;
    create_milestone(&app, &fx, "v1").await;

    for (arguments, expected) in [
        (
            json!({ "labels": ["nope"] }),
            "Label not found: nope (this project has bug)",
        ),
        (
            json!({ "task_type": "nope" }),
            "Type not found: nope (this project has Feature)",
        ),
        (
            json!({ "milestone": "nope" }),
            "Milestone not found: nope (this project has v1)",
        ),
        (
            json!({ "sprint": "nope" }),
            "Sprint not found: nope (this project has none)",
        ),
    ] {
        let mut arguments = arguments;
        arguments["project"] = json!(fx.project_key);
        arguments["title"] = json!("作られない");
        let outcome = call(&app, &token, "create_task", arguments).await;
        assert_eq!(outcome.error(), expected);
    }
    assert_eq!(task_count(&app, &fx).await, 0);
}

#[tokio::test]
async fn ambiguous_label_name_asks_for_a_uuid() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    // 同じ綴りのラベルは作れない（プロジェクトと名前の UNIQUE）。大文字小文字違いは作れるので、
    // どちらとも完全一致しない綴りで引くと複数に当たる
    let first = create_label(&app, &fx, "Bug").await;
    let second = create_label(&app, &fx, "bug").await;

    let outcome = call(
        &app,
        &token,
        "create_task",
        json!({ "project": fx.project_key, "title": "作られない", "labels": ["BUG"] }),
    )
    .await;
    let message = outcome.error();
    assert!(
        message.starts_with("Ambiguous Label: BUG (candidates: "),
        "{message}"
    );
    for expected in [
        first.to_string(),
        second.to_string(),
        "specify a UUID".into(),
    ] {
        assert!(message.contains(&expected), "{message}");
    }
    assert_eq!(task_count(&app, &fx).await, 0);

    // 対照: UUID で渡せば作れる
    let created = call(
        &app,
        &token,
        "create_task",
        json!({ "project": fx.project_key, "title": "UUID で渡す", "labels": [first.to_string()] }),
    )
    .await
    .json();
    let fetched = get_task_json(&app, &token, &created, &fx).await;
    assert_eq!(
        ids_of(&fetched["labels"], |label| &label["id"]),
        sorted(&[first])
    );
}

#[tokio::test]
async fn unknown_assignee_lists_candidates_only_with_write_scope() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let me = username(&app, fx.user.id).await;

    let outcome = call(
        &app,
        &token,
        "create_task",
        json!({ "project": fx.project_key, "title": "作られない", "assignees": ["nobody"] }),
    )
    .await;
    assert_eq!(
        outcome.error(),
        format!("Assignable user not found: nobody (this project has {me})")
    );

    // 読むだけの PAT でも名前の解決は POST の前に走る。`?username=` は read:task で通り、
    // 候補の全件は write:task が要るので候補なしになる
    let reader = app
        .insert_pat(
            fx.user.id,
            fx.tp.tenant_id,
            vec![Scope::ReadProject, Scope::ReadTask],
            None,
        )
        .await;
    let outcome = call(
        &app,
        &reader,
        "create_task",
        json!({ "project": fx.project_key, "title": "作られない", "assignees": ["nobody"] }),
    )
    .await;
    assert_eq!(
        outcome.error(),
        "Assignable user not found: nobody (this project has none)"
    );
    assert_eq!(task_count(&app, &fx).await, 0);
}

/// 名前は指したプロジェクトの一覧からだけ引く。同じテナントの別のプロジェクトの同名は拾わない
#[tokio::test]
async fn names_are_looked_up_only_in_the_given_project() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let other = insert_extra_project(&app, fx.tp.tenant_id).await;
    let response = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/projects/{other}/labels", fx.tp.tenant_id),
            json!({ "name": "bug", "color": "#336699" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let outcome = call(
        &app,
        &token,
        "create_task",
        json!({ "project": fx.project_key, "title": "作られない", "labels": ["bug"] }),
    )
    .await;
    assert_eq!(
        outcome.error(),
        "Label not found: bug (this project has none)"
    );
    assert_eq!(task_count(&app, &fx).await, 0);

    // 対照: 指したプロジェクトに同名を作れば引ける
    let bug = create_label(&app, &fx, "bug").await;
    let created = call(
        &app,
        &token,
        "create_task",
        json!({ "project": fx.project_key, "title": "引ける", "labels": ["bug"] }),
    )
    .await
    .json();
    let fetched = get_task_json(&app, &token, &created, &fx).await;
    assert_eq!(
        ids_of(&fetched["labels"], |label| &label["id"]),
        sorted(&[bug])
    );
}

#[tokio::test]
async fn milestone_and_sprint_names_need_their_read_scopes_and_uuids_do_not() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = app
        .insert_pat(
            fx.user.id,
            fx.tp.tenant_id,
            vec![Scope::ReadProject, Scope::WriteTask],
            None,
        )
        .await;
    let bug = create_label(&app, &fx, "bug").await;
    let v1 = create_milestone(&app, &fx, "v1").await;
    let week1 = create_sprint(&app, &fx, "week-1").await;
    let me = username(&app, fx.user.id).await;

    for (arguments, expected) in [
        (
            json!({ "milestone": "v1" }),
            "403 forbidden (could not list the milestones to resolve the milestone: the token may lack the read:milestone scope, or it is not allowed in this project; if the scope is missing, pass the milestone UUID instead)",
        ),
        (
            json!({ "sprint": "week-1" }),
            "403 forbidden (could not list the sprints to resolve the sprint: the token may lack the read:sprint scope, or it is not allowed in this project; if the scope is missing, pass the sprint UUID instead)",
        ),
    ] {
        let mut arguments = arguments;
        arguments["project"] = json!(fx.project_key);
        arguments["title"] = json!("作られない");
        let outcome = call(&app, &token, "create_task", arguments).await;
        assert_eq!(outcome.error(), expected);
    }
    assert_eq!(task_count(&app, &fx).await, 0);

    // 対照: 同じ PAT でも UUID なら作れ、ラベルと担当者は名前でも引ける
    let created = call(
        &app,
        &token,
        "create_task",
        json!({
            "project": fx.project_key,
            "title": "UUID で渡す",
            "milestone": v1.to_string(),
            "sprint": week1.to_string(),
            "labels": ["bug"],
            "assignees": [me],
        }),
    )
    .await
    .json();
    assert_eq!(created["milestone_id"], v1.to_string());
    assert_eq!(created["sprint_id"], week1.to_string());
    let fetched = get_task_json(&app, &token, &created, &fx).await;
    assert_eq!(
        ids_of(&fetched["labels"], |label| &label["id"]),
        sorted(&[bug])
    );
    assert_eq!(
        ids_of(&fetched["assignees"], |assignee| &assignee["user"]["id"]),
        vec![fx.user.id.to_string()]
    );
}
