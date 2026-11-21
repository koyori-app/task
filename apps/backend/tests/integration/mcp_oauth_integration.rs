//! MCP の OAuth（認可コード + PKCE、CIMD、公開クライアント）。規則は docs/dev/features/mcp.md。
//!
//! - 発見: メタデータ 2 つと、401 の `WWW-Authenticate`（後者は mcp_integration.rs）
//! - 同意: セッション専用。クライアント・`redirect_uri`・PKCE・`resource`・scope の検査と、
//!   テナントの所属（PAT の発行と同じ規則）
//! - トークン: コードの交換と一度きりの消費、リフレッシュトークンの回転と再利用の検出、
//!   クライアントの認証の拒否、期限
//! - 宛先: アクセストークンは `/mcp` でだけ通る。scope とテナントの束縛は PAT と同じに効く
//! - 失効: 解除、失効のエンドポイント、パスワードの再設定、停止、テナントの削除、一括失効、
//!   許可リストからの除外
//!
//! 拒否と対照の成功を組で確かめる。クライアントのメタデータは Redis のキャッシュに置き、
//! 外へは取りに行かない。

use crate::common::{
    TEST_OAUTH_CLIENT, TestApp, TestTenantProject, TestUser, create_status, insert_extra_project,
    insert_tenant, json_body,
};
use axum::http::StatusCode;
use backend::utils::desktop_auth::s256_challenge;
use backend::utils::oauth_server::client::{ClientMetadata, fetch_rate_limit_key};
use backend::utils::oauth_server::grant::{self, CodeState, GrantError, PendingAuthorization};
use backend::utils::password_reset;
use chrono::{Duration, Utc};
use common::cache::redis::RedisConnection;
use entity::project_members::ProjectRole;
use entity::scopes::Scope;
use entity::tenant_members::TenantRole;
use entity::{oauth_tokens, personal_tokens, projects, tenant_members};
use reqwest::{Client, Response, header};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use uuid::Uuid;

const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const REDIRECT_URI: &str = "https://client.test/callback";
const STATE: &str = "state-xyz";
const ALL_SCOPES: [&str; 8] = [
    "read:tenant",
    "read:project",
    "read:task",
    "write:task",
    "read:milestone",
    "read:sprint",
    "read:review",
    "write:review",
];

// ---- 道具 ----

struct Fixture {
    user: TestUser,
    tp: TestTenantProject,
}

/// 利用者（テナントのオーナー）・テナント・プロジェクトと既定の状態を作り、ログインし、
/// 試験用のクライアントをキャッシュに置く。
async fn fixture(app: &mut TestApp) -> Fixture {
    let user = app.insert_user_default().await;
    let tp = app.insert_tenant_project(user.id).await;
    login(app, &user).await;
    create_status(app, &tp, "Todo", false).await;
    app.seed_oauth_client(TEST_OAUTH_CLIENT, &[REDIRECT_URI])
        .await;
    Fixture { user, tp }
}

async fn login(app: &mut TestApp, user: &TestUser) {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
}

/// 認可の要求。`overrides` の値で上書きし、`null` の項目は消す。
fn params(overrides: Value) -> Value {
    let mut params = json!({
        "client_id": TEST_OAUTH_CLIENT,
        "redirect_uri": REDIRECT_URI,
        "response_type": "code",
        "code_challenge": s256_challenge(VERIFIER),
        "code_challenge_method": "S256",
        "state": STATE,
    });
    for (key, value) in overrides.as_object().expect("overrides") {
        if value.is_null() {
            params.as_object_mut().unwrap().remove(key);
        } else {
            params[key] = value.clone();
        }
    }
    params
}

fn approval(overrides: Value, tenant_ids: &[Uuid], scopes: &[&str]) -> Value {
    let mut body = params(overrides);
    body["tenant_ids"] = json!(tenant_ids);
    body["scopes"] = json!(scopes);
    body
}

async fn preview(app: &TestApp, overrides: Value) -> Response {
    app.post_json_with_session("/v1/oauth_grants/preview", params(overrides))
        .await
}

async fn approve(app: &TestApp, tenant_ids: &[Uuid], scopes: &[&str]) -> Response {
    app.post_json_with_session(
        "/v1/oauth_grants/authorize",
        approval(json!({}), tenant_ids, scopes),
    )
    .await
}

/// API のエラー（`{"message": …}`）の状態と `message`。
async fn message_of(res: Response) -> (StatusCode, String) {
    let status = res.status();
    let message = json_body(res).await["message"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    (status, message)
}

fn bad_request(message: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message.to_string())
}

fn forbidden(message: &str) -> (StatusCode, String) {
    (StatusCode::FORBIDDEN, message.to_string())
}

/// `redirect_to` の query を読む。
fn query_of(redirect_to: &str) -> Vec<(String, String)> {
    url::Url::parse(redirect_to)
        .expect("redirect_to")
        .query_pairs()
        .into_owned()
        .collect()
}

fn query_value(query: &[(String, String)], name: &str) -> Option<String> {
    query
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

async fn redirect_to_of(res: Response) -> String {
    let status = res.status();
    let body = json_body(res).await;
    assert_eq!(status, StatusCode::OK, "承認できる: {body}");
    body["redirect_to"]
        .as_str()
        .expect("redirect_to")
        .to_string()
}

/// 承認して、`redirect_to` から認可コードを取り出す。
async fn code_for(app: &TestApp, tenant_ids: &[Uuid], scopes: &[&str]) -> String {
    let redirect_to = redirect_to_of(approve(app, tenant_ids, scopes).await).await;
    query_value(&query_of(&redirect_to), "code").expect("code")
}

fn form_body(fields: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in fields {
        serializer.append_pair(name, value);
    }
    serializer.finish()
}

/// Cookie を持たないクライアントで、form の本文を送る。
async fn post_form(app: &TestApp, path: &str, fields: &[(&str, &str)]) -> Response {
    Client::new()
        .post(format!("{}{path}", app.base_url()))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form_body(fields))
        .send()
        .await
        .expect("form request")
}

async fn exchange(app: &TestApp, code: &str) -> Response {
    post_form(
        app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", VERIFIER),
            ("client_id", TEST_OAUTH_CLIENT),
            ("redirect_uri", REDIRECT_URI),
        ],
    )
    .await
}

async fn refresh(app: &TestApp, refresh_token: &str) -> Response {
    refresh_as(app, refresh_token, TEST_OAUTH_CLIENT).await
}

async fn refresh_as(app: &TestApp, refresh_token: &str, client_id: &str) -> Response {
    post_form(
        app,
        "/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ],
    )
    .await
}

/// トークンのエンドポイントのエラー（`{"error": …}`）の状態と `error`。
async fn oauth_error(res: Response) -> (StatusCode, String) {
    let status = res.status();
    assert_eq!(
        res.headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store"),
        "エラーにも Cache-Control: no-store を付ける"
    );
    let body = json_body(res).await;
    assert!(
        body["error_description"].is_string(),
        "error_description が在る: {body}"
    );
    (
        status,
        body["error"].as_str().unwrap_or_default().to_string(),
    )
}

fn invalid_grant() -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, "invalid_grant".to_string())
}

struct Tokens {
    access: String,
    refresh: Option<String>,
    scope: String,
}

impl Tokens {
    fn refresh(&self) -> &str {
        self.refresh.as_deref().expect("refresh_token")
    }
}

async fn tokens_of(res: Response) -> Tokens {
    let status = res.status();
    let no_store = res
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = json_body(res).await;
    assert_eq!(status, StatusCode::OK, "トークンを得られる: {body}");
    assert_eq!(no_store.as_deref(), Some("no-store"));
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["expires_in"], 3600);
    let access = body["access_token"].as_str().expect("access_token");
    assert!(access.starts_with("mcpat_"), "{access}");
    let refresh = body["refresh_token"].as_str().map(str::to_string);
    if let Some(refresh) = &refresh {
        assert!(refresh.starts_with("mcprt_"), "{refresh}");
    }
    Tokens {
        access: access.to_string(),
        refresh,
        scope: body["scope"].as_str().expect("scope").to_string(),
    }
}

/// 承認から交換まで通す。
async fn connect(app: &TestApp, tenant_ids: &[Uuid], scopes: &[&str]) -> Tokens {
    let code = code_for(app, tenant_ids, scopes).await;
    tokens_of(exchange(app, &code).await).await
}

fn initialize_body() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "mcp-oauth-integration-test", "version": "0.0.0" },
        },
    })
}

/// その Bearer で `/mcp` の入口を通れるか（`initialize` の状態）。
async fn mcp_status(app: &TestApp, token: &str) -> StatusCode {
    app.mcp(Some(token), initialize_body()).await.status()
}

struct ToolOutcome {
    is_error: bool,
    text: String,
}

impl ToolOutcome {
    fn json(&self) -> Value {
        assert!(!self.is_error, "ツールが失敗した: {}", self.text);
        serde_json::from_str(&self.text).expect("tool result json")
    }

    fn error(&self) -> &str {
        assert!(self.is_error, "ツールが成功した: {}", self.text);
        &self.text
    }
}

async fn call(app: &TestApp, token: &str, name: &str, arguments: Value) -> ToolOutcome {
    let res = app
        .mcp(
            Some(token),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": { "name": name, "arguments": arguments },
            }),
        )
        .await;
    let status = res.status();
    let body: Value = res.json().await.expect("mcp json");
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

async fn list_tasks(app: &TestApp, token: &str, project_id: Uuid) -> ToolOutcome {
    call(
        app,
        token,
        "list_tasks",
        json!({ "project": project_id.to_string() }),
    )
    .await
}

async fn create_task(app: &TestApp, token: &str, project_id: Uuid) -> ToolOutcome {
    call(
        app,
        token,
        "create_task",
        json!({ "project": project_id.to_string(), "title": "OAuth で作る" }),
    )
    .await
}

async fn list_projects(app: &TestApp, token: &str, tenant_id: Uuid) -> ToolOutcome {
    call(
        app,
        token,
        "list_projects",
        json!({ "tenant": tenant_id.to_string() }),
    )
    .await
}

async fn insert_tenant_member(app: &TestApp, tenant_id: Uuid, user_id: Uuid, role: TenantRole) {
    tenant_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        tenant_id: Set(tenant_id),
        user_id: Set(user_id),
        role: Set(role),
        department: Set(None),
        position: Set(None),
        team: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert tenant member");
}

async fn set_tenant_role(app: &TestApp, tenant_id: Uuid, user_id: Uuid, role: TenantRole) {
    tenant_members::Entity::update_many()
        .col_expr(tenant_members::Column::Role, Expr::value(role))
        .filter(tenant_members::Column::TenantId.eq(tenant_id))
        .filter(tenant_members::Column::UserId.eq(user_id))
        .exec(&app.state.db)
        .await
        .expect("set tenant role");
}

async fn remove_tenant_member(app: &TestApp, tenant_id: Uuid, user_id: Uuid) {
    tenant_members::Entity::delete_many()
        .filter(tenant_members::Column::TenantId.eq(tenant_id))
        .filter(tenant_members::Column::UserId.eq(user_id))
        .exec(&app.state.db)
        .await
        .expect("remove tenant member");
}

async fn insert_project_member(app: &TestApp, project_id: Uuid, user_id: Uuid) {
    entity::project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        user_id: Set(user_id),
        role: Set(ProjectRole::Member),
    }
    .insert(&app.state.db)
    .await
    .expect("insert project member");
}

async fn enforce_2fa(app: &TestApp, tenant_id: Uuid) {
    entity::tenants::Entity::update_many()
        .col_expr(entity::tenants::Column::Require2fa, Expr::value(true))
        .filter(entity::tenants::Column::Id.eq(tenant_id))
        .exec(&app.state.db)
        .await
        .expect("enforce 2fa");
}

/// 利用者の許可の行（OAuth の許可だけ）。
async fn grants_of(app: &TestApp, user_id: Uuid) -> Vec<personal_tokens::Model> {
    personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(user_id))
        .filter(personal_tokens::Column::OauthClientId.is_not_null())
        .all(&app.state.db)
        .await
        .expect("find grants")
}

async fn only_grant(app: &TestApp, user_id: Uuid) -> personal_tokens::Model {
    let mut grants = grants_of(app, user_id).await;
    assert_eq!(grants.len(), 1, "許可は 1 件");
    grants.remove(0)
}

async fn set_token_expiry(app: &TestApp, grant_id: Uuid, kind: &str, at: chrono::DateTime<Utc>) {
    oauth_tokens::Entity::update_many()
        .col_expr(
            oauth_tokens::Column::ExpiresAt,
            Expr::value(at.fixed_offset()),
        )
        .filter(oauth_tokens::Column::GrantId.eq(grant_id))
        .filter(oauth_tokens::Column::Kind.eq(kind))
        .exec(&app.state.db)
        .await
        .expect("set token expiry");
}

async fn set_grant_expiry(app: &TestApp, grant_id: Uuid, at: chrono::DateTime<Utc>) {
    personal_tokens::Entity::update_many()
        .col_expr(
            personal_tokens::Column::ExpiresAt,
            Expr::value(at.fixed_offset()),
        )
        .filter(personal_tokens::Column::Id.eq(grant_id))
        .exec(&app.state.db)
        .await
        .expect("set grant expiry");
}

/// アクセストークンは `/mcp` で 401、リフレッシュトークンは `invalid_grant` になっている。
async fn assert_dead(app: &TestApp, tokens: &Tokens, why: &str) {
    assert_eq!(
        mcp_status(app, &tokens.access).await,
        StatusCode::UNAUTHORIZED,
        "アクセストークンは使えない: {why}"
    );
    assert_eq!(
        oauth_error(refresh(app, tokens.refresh()).await).await,
        invalid_grant(),
        "リフレッシュトークンは使えない: {why}"
    );
}

// ---- 発見 ----

#[tokio::test]
async fn protected_resource_metadata_names_the_mcp_endpoint_and_the_issuer() {
    let app = TestApp::new().await;
    let origin = app.state.settings.public_origin();

    let res = app
        .get("/.well-known/oauth-protected-resource/api/mcp")
        .await;

    assert_eq!(res.status(), StatusCode::OK);
    let body = json_body(res).await;
    assert_eq!(body["resource"], format!("{origin}/api/mcp"));
    assert_eq!(body["authorization_servers"], json!([origin]));
    assert_eq!(body["scopes_supported"], json!(ALL_SCOPES));
    assert_eq!(body["bearer_methods_supported"], json!(["header"]));
}

#[tokio::test]
async fn root_protected_resource_metadata_is_not_served() {
    let app = TestApp::new().await;
    let res = app.get("/.well-known/oauth-protected-resource").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn authorization_server_metadata_advertises_cimd_and_public_clients_only() {
    let app = TestApp::new().await;
    let origin = app.state.settings.public_origin();

    let res = app.get("/.well-known/oauth-authorization-server").await;

    assert_eq!(res.status(), StatusCode::OK);
    let body = json_body(res).await;
    assert_eq!(body["issuer"], origin);
    assert_eq!(
        body["authorization_endpoint"],
        format!("{origin}/oauth/authorize")
    );
    assert_eq!(body["token_endpoint"], format!("{origin}/api/oauth/token"));
    assert_eq!(
        body["revocation_endpoint"],
        format!("{origin}/api/oauth/revoke")
    );
    assert_eq!(body["response_types_supported"], json!(["code"]));
    assert_eq!(
        body["grant_types_supported"],
        json!(["authorization_code", "refresh_token"])
    );
    assert_eq!(body["code_challenge_methods_supported"], json!(["S256"]));
    assert_eq!(
        body["token_endpoint_auth_methods_supported"],
        json!(["none"])
    );
    assert_eq!(body["client_id_metadata_document_supported"], true);
    assert_eq!(body["authorization_response_iss_parameter_supported"], true);
    let scopes = body["scopes_supported"].as_array().expect("scopes");
    assert!(scopes.contains(&json!("offline_access")), "{scopes:?}");
    assert_eq!(scopes.len(), ALL_SCOPES.len() + 1);
    // クライアントの登録は CIMD だけ
    assert!(body.get("registration_endpoint").is_none());
}

// ---- 一連の流れと宛先 ----

#[tokio::test]
async fn full_flow_connects_and_calls_tools() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;

    let res = preview(&app, json!({})).await;
    assert_eq!(res.status(), StatusCode::OK);
    let shown = json_body(res).await;
    assert_eq!(shown["client_id"], TEST_OAUTH_CLIENT);
    assert_eq!(shown["client_name"], "Test Client");
    assert_eq!(shown["client_host"], "client.test");
    assert_eq!(shown["redirect_host"], "client.test");
    assert_eq!(shown["redirect_is_loopback"], false);
    assert_eq!(shown["scopes"], json!(ALL_SCOPES));

    let redirect_to = redirect_to_of(approve(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await).await;
    assert!(
        redirect_to.starts_with(&format!("{REDIRECT_URI}?")),
        "{redirect_to}"
    );
    let query = query_of(&redirect_to);
    assert_eq!(query_value(&query, "state").as_deref(), Some(STATE));
    assert_eq!(
        query_value(&query, "iss"),
        Some(app.state.settings.public_origin())
    );
    let code = query_value(&query, "code").expect("code");

    let tokens = tokens_of(exchange(&app, &code).await).await;
    assert_eq!(tokens.scope, ALL_SCOPES.join(" "));
    assert!(tokens.refresh.is_some(), "refresh_token を返す");

    let me = call(&app, &tokens.access, "whoami", json!({})).await.json();
    assert_eq!(me["user_id"], fx.user.id.to_string());
    assert_eq!(me["name"], "Test Client", "許可の名前はクライアント名");
    let projects = list_projects(&app, &tokens.access, fx.tp.tenant_id)
        .await
        .json();
    assert_eq!(projects.as_array().map(Vec::len), Some(1), "{projects}");
    let created = create_task(&app, &tokens.access, fx.tp.project_id)
        .await
        .json();
    assert_eq!(created["title"], "OAuth で作る");
}

#[tokio::test]
async fn access_token_is_rejected_outside_the_mcp_endpoint() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    for path in ["/v1/personal_tokens/me", "/v1/tenants", "/v1/auth/me"] {
        let res = app.get_with_bearer(path, &tokens.access).await;
        assert_eq!(
            message_of(res).await,
            (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            "{path}"
        );
    }

    // 対照: 同じトークンは `/mcp` で通り、PAT は REST で通る
    assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);
    let pat = app
        .insert_pat(fx.user.id, fx.tp.tenant_id, vec![Scope::Api], None)
        .await;
    let res = app.get_with_bearer("/v1/personal_tokens/me", &pat).await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn refresh_token_is_not_a_bearer_credential() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    let res = app.mcp(Some(tokens.refresh()), initialize_body()).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let challenge = res
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .expect("challenge");
    assert!(challenge.contains("error=\"invalid_token\""), "{challenge}");

    // 対照: アクセストークンは通る
    assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);
}

#[tokio::test]
async fn consent_narrows_the_scope() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &["read:task"]).await;
    assert_eq!(tokens.scope, "read:task");

    let denied = create_task(&app, &tokens.access, fx.tp.project_id).await;
    assert!(
        denied.error().starts_with("403 forbidden"),
        "{}",
        denied.text
    );

    // 対照: 読み取りは通る
    let listed = list_tasks(&app, &tokens.access, fx.tp.project_id)
        .await
        .json();
    assert_eq!(listed["total"], 0, "{listed}");
}

#[tokio::test]
async fn oauth_grant_with_read_review_can_list_findings() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let project = fx.tp.project_id.to_string();
    let list = json!({ "project": project, "pr": 5 });
    let submit = json!({
        "project": project,
        "pr": 5,
        "head_sha": "60cdd77a1b2c3d4e5f60718293a4b5c6d7e8f901",
    });

    let reader = connect(&app, &[fx.tp.tenant_id], &["read:project", "read:review"]).await;
    assert_eq!(reader.scope, "read:project read:review");
    let listed = call(&app, &reader.access, "list_review_findings", list.clone())
        .await
        .json();
    assert_eq!(listed, json!([]));
    let denied = call(&app, &reader.access, "submit_review", submit.clone()).await;
    assert_eq!(denied.error(), "403 forbidden");

    // レビューの scope を持たない許可では読めない
    let tasks_only = connect(&app, &[fx.tp.tenant_id], &["read:task"]).await;
    let denied = call(&app, &tasks_only.access, "list_review_findings", list).await;
    assert_eq!(denied.error(), "403 forbidden");

    // 対照: `write:review` の許可は起票できる
    let writer = connect(&app, &[fx.tp.tenant_id], &["write:review"]).await;
    let created = call(&app, &writer.access, "submit_review", submit)
        .await
        .json();
    assert_eq!(created["round"], 1, "{created}");
}

#[tokio::test]
async fn consent_binds_the_grant_to_the_chosen_tenants() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    // 持ち主は同じ。選ばなかったテナントは、所属していても触れない
    let other_tenant = insert_tenant(&app.state.db, fx.user.id).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    let denied = list_projects(&app, &tokens.access, other_tenant).await;
    assert_eq!(denied.error(), "403 forbidden");

    let projects = list_projects(&app, &tokens.access, fx.tp.tenant_id)
        .await
        .json();
    assert_eq!(projects.as_array().map(Vec::len), Some(1), "{projects}");
}

// ---- 同意できる人 ----

#[tokio::test]
async fn every_tenant_role_can_consent() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;

    let owner_tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    assert!(
        !list_projects(&app, &owner_tokens.access, fx.tp.tenant_id)
            .await
            .is_error
    );

    for role in [TenantRole::Admin, TenantRole::Member, TenantRole::Viewer] {
        let user = app.insert_user_default().await;
        insert_tenant_member(&app, fx.tp.tenant_id, user.id, role.clone()).await;
        login(&mut app, &user).await;

        let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
        let projects = list_projects(&app, &tokens.access, fx.tp.tenant_id).await;
        assert!(!projects.is_error, "{role:?}: {}", projects.text);
    }
}

#[tokio::test]
async fn outsider_and_project_only_guest_cannot_consent() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let outsider = app.insert_user_default().await;
    let guest = app.insert_user_default().await;
    insert_project_member(&app, fx.tp.project_id, guest.id).await;

    for user in [&outsider, &guest] {
        login(&mut app, user).await;
        let res = approve(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
        assert_eq!(
            message_of(res).await,
            forbidden("tenant-membership-missing")
        );
        assert!(grants_of(&app, user.id).await.is_empty());
    }
}

#[tokio::test]
async fn consent_is_rejected_as_a_whole_when_one_tenant_fails() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let stranger = app.insert_user_default().await;
    let foreign_tenant = insert_tenant(&app.state.db, stranger.id).await;

    let res = approve(&app, &[fx.tp.tenant_id, foreign_tenant], &ALL_SCOPES).await;
    let status = res.status();
    let body = json_body(res).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["message"], "tenant-membership-missing");
    assert!(body.get("redirect_to").is_none(), "コードを発行しない");

    // 対照: 所属するテナントだけなら通る。無いテナントは 404
    assert_eq!(
        approve(&app, &[fx.tp.tenant_id], &ALL_SCOPES)
            .await
            .status(),
        StatusCode::OK
    );
    let res = approve(&app, &[fx.tp.tenant_id, Uuid::new_v4()], &ALL_SCOPES).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn member_grant_reaches_what_a_member_can_do_and_survives_a_demotion() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let user = app.insert_user_default().await;
    insert_tenant_member(&app, fx.tp.tenant_id, user.id, TenantRole::Admin).await;
    login(&mut app, &user).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    assert!(
        !create_task(&app, &tokens.access, fx.tp.project_id)
            .await
            .is_error
    );

    // 下げても許可は生きていて、結果は変わらない（ツールが呼ぶ操作にロールで止めるものが無い）
    set_tenant_role(&app, fx.tp.tenant_id, user.id, TenantRole::Member).await;
    assert!(
        !create_task(&app, &tokens.access, fx.tp.project_id)
            .await
            .is_error
    );
    let listed = list_tasks(&app, &tokens.access, fx.tp.project_id)
        .await
        .json();
    assert_eq!(listed["total"], 2, "{listed}");
    assert!(!only_grant(&app, user.id).await.revoked);
}

#[tokio::test]
async fn tenant_2fa_enforcement_applies_to_consent() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let member = app.insert_user_default().await;
    insert_tenant_member(&app, fx.tp.tenant_id, member.id, TenantRole::Member).await;
    let with_2fa = app.insert_user_default().await;
    insert_tenant_member(&app, fx.tp.tenant_id, with_2fa.id, TenantRole::Member).await;
    let enabled = app.enable_2fa(&with_2fa).await;
    // 強制の前に本認証のセッションを取る
    login(&mut app, &member).await;
    enforce_2fa(&app, fx.tp.tenant_id).await;

    let res = approve(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    assert_eq!(message_of(res).await, forbidden("2fa-setup-required"));
    assert!(grants_of(&app, member.id).await.is_empty());

    // 対照: 2FA を設定した利用者は同意できる
    app.reset_session_client();
    app.login_half_authed(&with_2fa).await;
    let verified = app
        .post_json_with_session(
            "/v1/auth/2fa/verify",
            json!({ "recovery_code": enabled.recovery_codes[0] }),
        )
        .await;
    assert_eq!(verified.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        approve(&app, &[fx.tp.tenant_id], &ALL_SCOPES)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn consent_endpoints_are_session_only() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let body = approval(json!({}), &[fx.tp.tenant_id], &ALL_SCOPES);
    let pat = app
        .insert_pat(fx.user.id, fx.tp.tenant_id, vec![Scope::Api], None)
        .await;
    let oauth = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    for path in [
        "/v1/oauth_grants/preview",
        "/v1/oauth_grants/authorize",
        "/v1/oauth_grants/deny",
    ] {
        let res = app.post_json_with_bearer(path, body.clone(), &pat).await;
        assert_eq!(message_of(res).await, forbidden("forbidden"), "PAT: {path}");
        // OAuth のアクセストークンは `/v1` では認証そのものが通らない
        let res = app
            .post_json_with_bearer(path, body.clone(), &oauth.access)
            .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "OAuth: {path}");
        // Cookie を持たないクライアントで送る
        let res = Client::new()
            .post(format!("{}{path}", app.base_url()))
            .json(&body)
            .send()
            .await
            .expect("anonymous request");
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "資格なし: {path}");
    }
    let res = app.get_with_bearer("/v1/oauth_grants", &pat).await;
    assert_eq!(message_of(res).await, forbidden("forbidden"));
    let res = Client::new()
        .get(format!("{}/v1/oauth_grants", app.base_url()))
        .send()
        .await
        .expect("anonymous request");
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 対照: セッションなら通る
    let res = app
        .post_json_with_session("/v1/oauth_grants/preview", params(json!({})))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
}

/// 本番のセッション Cookie は SameSite=None なので、同意と解除を守るのは `Origin` の検査である。
/// ほかのサイトの `Origin` が付いた、Cookie だけの要求は断る。
#[tokio::test]
async fn consent_and_disconnect_reject_a_foreign_origin() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let grant_id = only_grant(&app, fx.user.id).await.id;
    let body = approval(json!({}), &[fx.tp.tenant_id], &ALL_SCOPES);
    let frontend_origin = app.state.settings.public_origin();
    let post = |path: &str, origin: &str| {
        app.client()
            .post(format!("{}{path}", app.base_url()))
            .header(header::ORIGIN, origin)
            .json(&body)
            .send()
    };
    let delete = |origin: &str| {
        app.client()
            .delete(format!("{}/v1/oauth_grants/{grant_id}", app.base_url()))
            .header(header::ORIGIN, origin)
            .send()
    };

    for path in [
        "/v1/oauth_grants/authorize",
        "/v1/oauth_grants/deny",
        "/v1/oauth_grants/preview",
    ] {
        let res = post(path, "https://evil.example").await.expect("request");
        assert_eq!(message_of(res).await, forbidden("forbidden"), "{path}");
    }
    let res = delete("https://evil.example").await.expect("request");
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        grants_of(&app, fx.user.id).await.len(),
        1,
        "許可は増えていない"
    );
    assert!(
        !only_grant(&app, fx.user.id).await.revoked,
        "解除されていない"
    );

    // 対照: 画面の origin からの要求は通る
    let res = post("/v1/oauth_grants/authorize", &frontend_origin)
        .await
        .expect("request");
    assert_eq!(res.status(), StatusCode::OK);
    let res = delete(&frontend_origin).await.expect("request");
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn device_token_cannot_consent() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let res = app
        .post_json_with_session(
            "/v1/desktop/auth/codes",
            json!({ "code_challenge": s256_challenge(VERIFIER), "name": "laptop" }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let code = json_body(res).await["code"]
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
    assert_eq!(res.status(), StatusCode::CREATED);
    let device_token = json_body(res).await["token"]
        .as_str()
        .expect("token")
        .to_string();

    let res = app
        .post_json_with_bearer(
            "/v1/oauth_grants/authorize",
            approval(json!({}), &[fx.tp.tenant_id], &ALL_SCOPES),
            &device_token,
        )
        .await;
    assert_eq!(message_of(res).await, forbidden("forbidden"));
}

// ---- 認可の要求の検査 ----

#[tokio::test]
async fn client_outside_the_allowlist_is_rejected_even_when_cached() {
    let mut app = TestApp::new().await;
    fixture(&mut app).await;
    let unlisted = "https://unlisted.test/oauth/metadata.json";
    app.seed_oauth_client(unlisted, &[REDIRECT_URI]).await;

    for client_id in [
        unlisted,
        // 完全一致だけ。前方一致や末尾の `/` では通らない
        "https://client.test/oauth/metadata.json/",
        "https://client.test/oauth/",
    ] {
        let res = preview(&app, json!({ "client_id": client_id })).await;
        assert_eq!(
            message_of(res).await,
            bad_request("oauth-client-not-allowed"),
            "{client_id}"
        );
    }
}

#[tokio::test]
async fn redirect_uri_must_match_the_registration_exactly() {
    let mut app = TestApp::new().await;
    fixture(&mut app).await;

    for redirect_uri in [
        "https://client.test/callbacl",
        "https://client.test/other",
        "https://evil.test/callback",
        "https://client.test:8443/callback",
        "https://client.test/callback?x=1",
        "https://client.test/callback/",
    ] {
        let res = preview(&app, json!({ "redirect_uri": redirect_uri })).await;
        assert_eq!(
            message_of(res).await,
            bad_request("oauth-invalid-redirect-uri"),
            "{redirect_uri}"
        );
    }

    assert_eq!(preview(&app, json!({})).await.status(), StatusCode::OK);
}

const NATIVE_CLIENT: &str = "https://native.test/oauth/metadata.json";

async fn app_with_native_client() -> TestApp {
    let app =
        TestApp::new_with_oauth_clients(&format!("{TEST_OAUTH_CLIENT}, {NATIVE_CLIENT}")).await;
    app.seed_oauth_client(
        NATIVE_CLIENT,
        &["http://localhost/callback", "http://127.0.0.1/callback"],
    )
    .await;
    app
}

#[tokio::test]
async fn loopback_redirect_uri_ignores_the_port_only() {
    let mut app = app_with_native_client().await;
    fixture(&mut app).await;

    let res = preview(
        &app,
        json!({ "client_id": NATIVE_CLIENT, "redirect_uri": "http://localhost:53124/callback" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let shown = json_body(res).await;
    assert_eq!(shown["redirect_host"], "localhost:53124");
    assert_eq!(shown["redirect_is_loopback"], true);
    assert_eq!(shown["client_host"], "native.test");

    let res = preview(
        &app,
        json!({ "client_id": NATIVE_CLIENT, "redirect_uri": "http://127.0.0.1:41000/callback" }),
    )
    .await;
    assert_eq!(json_body(res).await["redirect_host"], "127.0.0.1:41000");

    for redirect_uri in [
        "http://localhost:53124/other",
        "http://localhost:53124/callback/abc",
        "http://localhost:1@evil.test/callback",
        "http://[::1]:53124/callback",
    ] {
        let res = preview(
            &app,
            json!({ "client_id": NATIVE_CLIENT, "redirect_uri": redirect_uri }),
        )
        .await;
        assert_eq!(
            message_of(res).await,
            bad_request("oauth-invalid-redirect-uri"),
            "{redirect_uri}"
        );
    }
}

#[tokio::test]
async fn malformed_authorization_requests_are_rejected() {
    let mut app = TestApp::new().await;
    fixture(&mut app).await;

    for (overrides, message) in [
        (
            json!({ "response_type": "token" }),
            "oauth-unsupported-response-type",
        ),
        (
            json!({ "response_type": null }),
            "oauth-unsupported-response-type",
        ),
        (
            json!({ "code_challenge_method": "plain" }),
            "oauth-invalid-code-challenge",
        ),
        (
            json!({ "code_challenge_method": null }),
            "oauth-invalid-code-challenge",
        ),
        (
            json!({ "code_challenge": null }),
            "oauth-invalid-code-challenge",
        ),
        (
            json!({ "code_challenge": "short" }),
            "oauth-invalid-code-challenge",
        ),
        (
            json!({ "resource": "https://other.test/api/mcp" }),
            "oauth-invalid-target",
        ),
        (
            json!({ "resource": format!("{}/api", app.state.settings.public_origin()) }),
            "oauth-invalid-target",
        ),
    ] {
        let res = preview(&app, overrides.clone()).await;
        assert_eq!(message_of(res).await, bad_request(message), "{overrides}");
    }

    let origin = app.state.settings.public_origin();
    let uppercase_host = origin.replacen("localhost", "LOCALHOST", 1);
    for resource in [
        json!(null),
        json!(format!("{origin}/api/mcp")),
        json!(format!("{origin}/api/mcp/")),
        json!(format!("{uppercase_host}/api/mcp")),
    ] {
        let res = preview(&app, json!({ "resource": resource })).await;
        assert_eq!(res.status(), StatusCode::OK, "{resource}");
    }
}

#[tokio::test]
async fn requested_scope_is_filtered_to_the_grantable_ones() {
    let mut app = TestApp::new().await;
    fixture(&mut app).await;

    let res = preview(&app, json!({ "scope": "write:project" })).await;
    assert_eq!(message_of(res).await, bad_request("oauth-invalid-scope"));

    let res = preview(&app, json!({ "scope": "write:project read:task" })).await;
    assert_eq!(json_body(res).await["scopes"], json!(["read:task"]));
    for scope in [json!("offline_access"), json!(null)] {
        let res = preview(&app, json!({ "scope": scope })).await;
        assert_eq!(json_body(res).await["scopes"], json!(ALL_SCOPES), "{scope}");
    }
}

#[tokio::test]
async fn approval_cannot_add_a_scope_that_was_not_requested() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;

    let res = approve(&app, &[fx.tp.tenant_id], &["read:task", "write:project"]).await;
    assert_eq!(message_of(res).await, bad_request("bad-request"));

    // 要求が `read:task` だけなら、渡せる scope でも足せない
    let body = approval(
        json!({ "scope": "read:task" }),
        &[fx.tp.tenant_id],
        &["read:task", "write:task"],
    );
    let res = app
        .post_json_with_session("/v1/oauth_grants/authorize", body)
        .await;
    assert_eq!(message_of(res).await, bad_request("bad-request"));

    // 対照: 部分集合は通る
    let tokens = connect(&app, &[fx.tp.tenant_id], &["read:task", "read:project"]).await;
    assert_eq!(tokens.scope, "read:project read:task");
}

#[tokio::test]
async fn denial_redirects_with_access_denied() {
    let mut app = TestApp::new().await;
    fixture(&mut app).await;

    let res = app
        .post_json_with_session("/v1/oauth_grants/deny", params(json!({})))
        .await;
    let redirect_to = redirect_to_of(res).await;
    assert!(
        redirect_to.starts_with(&format!("{REDIRECT_URI}?")),
        "{redirect_to}"
    );
    let query = query_of(&redirect_to);
    assert_eq!(
        query_value(&query, "error").as_deref(),
        Some("access_denied")
    );
    assert_eq!(query_value(&query, "state").as_deref(), Some(STATE));
    assert_eq!(
        query_value(&query, "iss"),
        Some(app.state.settings.public_origin())
    );
    assert!(query_value(&query, "code").is_none());

    // 確かめていない `redirect_uri` へは飛ばさない
    let res = app
        .post_json_with_session(
            "/v1/oauth_grants/deny",
            params(json!({ "redirect_uri": "https://evil.test/callback" })),
        )
        .await;
    let status = res.status();
    let body = json_body(res).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["message"], "oauth-invalid-redirect-uri");
    assert!(body.get("redirect_to").is_none());
}

#[tokio::test]
async fn request_limits_are_enforced_at_the_boundary() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;

    // state: 2048 文字は通り、2049 文字は 400
    let res = preview(&app, json!({ "state": "s".repeat(2048) })).await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = preview(&app, json!({ "state": "s".repeat(2049) })).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = approval(
        json!({ "state": "s".repeat(2049) }),
        &[fx.tp.tenant_id],
        &ALL_SCOPES,
    );
    let res = app
        .post_json_with_session("/v1/oauth_grants/authorize", body)
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST, "承認でも上限が効く");

    // tenant_ids: 20 件は通り、21 件は 400
    let mut tenants = vec![fx.tp.tenant_id];
    for _ in 0..20 {
        tenants.push(insert_tenant(&app.state.db, fx.user.id).await);
    }
    let redirect_to = redirect_to_of(approve(&app, &tenants[..20], &ALL_SCOPES).await).await;
    let code = query_value(&query_of(&redirect_to), "code").expect("code");
    tokens_of(exchange(&app, &code).await).await;
    let grant = only_grant(&app, fx.user.id).await;
    let bound = entity::personal_token_tenants::Entity::find()
        .filter(entity::personal_token_tenants::Column::TokenId.eq(grant.id))
        .count(&app.state.db)
        .await
        .expect("count bindings");
    assert_eq!(bound, 20);

    let res = approve(&app, &tenants, &ALL_SCOPES).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST, "21 件は 400");
    let res = approve(&app, &[], &ALL_SCOPES).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST, "0 件は 400");
    let res = approve(&app, &[fx.tp.tenant_id], &[]).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST, "scope 0 件は 400");
}

#[tokio::test]
async fn metadata_fetches_are_rate_limited_per_user() {
    // `resolve_public` が名前解決の前に拒否する宛先なので、外へ出ずに取得が失敗する
    let unreachable = "https://127.0.0.1/oauth/metadata.json";
    let mut app =
        TestApp::new_with_oauth_clients(&format!("{TEST_OAUTH_CLIENT},{unreachable}")).await;
    let fx = fixture(&mut app).await;

    for attempt in 1..=11 {
        let res = preview(&app, json!({ "client_id": unreachable })).await;
        assert_eq!(
            message_of(res).await,
            bad_request("oauth-client-metadata-unavailable"),
            "{attempt} 回目"
        );
    }
    let mut conn = app.state.redis_client.conn.acquire().await.expect("redis");
    let count: i64 = redis::cmd("GET")
        .arg(fetch_rate_limit_key(fx.user.id))
        .query_async(&mut conn)
        .await
        .expect("rate limit counter");
    assert_eq!(count, 11, "11 回とも数え、11 回目は取りに行かない");

    // 対照: キャッシュに在るクライアントは、上限を超えたあとも通る（数えない）
    assert_eq!(preview(&app, json!({})).await.status(), StatusCode::OK);
    let count: i64 = redis::cmd("GET")
        .arg(fetch_rate_limit_key(fx.user.id))
        .query_async(&mut conn)
        .await
        .expect("rate limit counter");
    assert_eq!(count, 11);
}

// ---- コードの交換 ----

#[tokio::test]
async fn pkce_mismatch_consumes_the_code() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let code = code_for(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    let res = post_form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            (
                "code_verifier",
                "wrong-verifier-wrong-verifier-wrong-verifier-123",
            ),
            ("client_id", TEST_OAUTH_CLIENT),
        ],
    )
    .await;
    assert_eq!(oauth_error(res).await, invalid_grant());

    // コードは消費済みなので、正しい verifier でも通らない
    assert_eq!(
        oauth_error(exchange(&app, &code).await).await,
        invalid_grant()
    );
    assert!(grants_of(&app, fx.user.id).await.is_empty());
}

#[tokio::test]
async fn reusing_a_code_revokes_the_grant_it_created() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let code = code_for(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let tokens = tokens_of(exchange(&app, &code).await).await;
    assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);

    assert_eq!(
        oauth_error(exchange(&app, &code).await).await,
        invalid_grant()
    );

    assert_dead(&app, &tokens, "コードの二重使用").await;
    assert!(only_grant(&app, fx.user.id).await.revoked);
}

#[tokio::test]
async fn code_is_bound_to_the_client_and_the_redirect_uri() {
    let mut app = app_with_native_client().await;
    let fx = fixture(&mut app).await;

    let code = code_for(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let res = post_form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("code_verifier", VERIFIER),
            ("client_id", NATIVE_CLIENT),
        ],
    )
    .await;
    assert_eq!(
        oauth_error(res).await,
        invalid_grant(),
        "client_id の不一致"
    );

    let code = code_for(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let res = post_form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("code_verifier", VERIFIER),
            ("client_id", TEST_OAUTH_CLIENT),
            ("redirect_uri", "https://client.test/other"),
        ],
    )
    .await;
    assert_eq!(
        oauth_error(res).await,
        invalid_grant(),
        "redirect_uri の不一致"
    );

    let code = code_for(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let res = post_form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("code_verifier", VERIFIER),
            ("client_id", TEST_OAUTH_CLIENT),
            ("resource", "https://other.test/api/mcp"),
        ],
    )
    .await;
    assert_eq!(
        oauth_error(res).await,
        (StatusCode::BAD_REQUEST, "invalid_target".to_string())
    );
    assert!(grants_of(&app, fx.user.id).await.is_empty());

    // 対照: `redirect_uri` を省いた要求と、正しい `resource` を付けた要求は通る
    let resource = format!("{}/api/mcp", app.state.settings.public_origin());
    let res = post_form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("code_verifier", VERIFIER),
            ("client_id", TEST_OAUTH_CLIENT),
            ("resource", &resource),
        ],
    )
    .await;
    tokens_of(res).await;
}

#[tokio::test]
async fn token_endpoint_rejects_other_grant_types_and_bodies() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let code = code_for(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    for fields in [
        vec![
            ("grant_type", "password"),
            ("username", "a"),
            ("password", "b"),
        ],
        vec![("grant_type", "client_credentials")],
        vec![("code", code.as_str())],
    ] {
        assert_eq!(
            oauth_error(post_form(&app, "/oauth/token", &fields).await).await,
            (
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type".to_string()
            ),
            "{fields:?}"
        );
    }

    let invalid_request = (StatusCode::BAD_REQUEST, "invalid_request".to_string());
    let json_request = Client::new()
        .post(format!("{}/oauth/token", app.base_url()))
        .json(&json!({
            "grant_type": "authorization_code",
            "code": code,
            "code_verifier": VERIFIER,
            "client_id": TEST_OAUTH_CLIENT,
        }))
        .send()
        .await
        .expect("json request");
    assert_eq!(
        oauth_error(json_request).await,
        invalid_request,
        "JSON の本文"
    );
    for fields in [
        vec![
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
        ],
        vec![
            ("grant_type", "authorization_code"),
            ("code_verifier", VERIFIER),
            ("client_id", TEST_OAUTH_CLIENT),
        ],
        vec![
            ("grant_type", "refresh_token"),
            ("client_id", TEST_OAUTH_CLIENT),
        ],
        vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", "mcprt_x"),
        ],
    ] {
        assert_eq!(
            oauth_error(post_form(&app, "/oauth/token", &fields).await).await,
            invalid_request,
            "{fields:?}"
        );
    }

    // どの拒否もコードを消費していない
    tokens_of(exchange(&app, &code).await).await;
}

#[tokio::test]
async fn client_authentication_is_rejected_without_consuming_the_code() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let code = code_for(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let fields = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("code_verifier", VERIFIER),
        ("client_id", TEST_OAUTH_CLIENT),
    ];
    let invalid_client = (StatusCode::UNAUTHORIZED, "invalid_client".to_string());

    let res = Client::new()
        .post(format!("{}/oauth/token", app.base_url()))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::AUTHORIZATION, "Basic Y2xpZW50OnNlY3JldA==")
        .body(form_body(&fields))
        .send()
        .await
        .expect("basic request");
    assert_eq!(
        res.headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic")
    );
    assert_eq!(oauth_error(res).await, invalid_client);

    for extra in [
        ("client_secret", "s3cret"),
        ("client_assertion", "eyJ.x.y"),
        (
            "client_assertion_type",
            "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
        ),
    ] {
        let mut with_auth = fields.to_vec();
        with_auth.push(extra);
        let res = post_form(&app, "/oauth/token", &with_auth).await;
        assert!(res.headers().get(header::WWW_AUTHENTICATE).is_none());
        assert_eq!(oauth_error(res).await, invalid_client, "{}", extra.0);
    }

    // 対照: 同じコードを認証なしで送り直すと通る
    tokens_of(post_form(&app, "/oauth/token", &fields).await).await;
}

async fn reset_password(app: &TestApp, user: &TestUser) {
    let token = format!("oauth-reset-{}", Uuid::new_v4());
    password_reset::store_token(&app.state.redis_client, user.id, &token)
        .await
        .expect("store token");
    let res = app
        .post_json(
            "/v1/auth/password-reset/complete",
            json!({ "token": token, "new_password": "NewPassword456!" }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn password_reset_between_approval_and_exchange_invalidates_the_code() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let code = code_for(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    // 承認とパスワードの再設定が同じミリ秒に入らないようにする
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    reset_password(&app, &fx.user).await;

    assert_eq!(
        oauth_error(exchange(&app, &code).await).await,
        invalid_grant()
    );
    assert!(grants_of(&app, fx.user.id).await.is_empty());
}

#[tokio::test]
async fn leaving_the_tenant_between_approval_and_exchange_invalidates_the_code() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let user = app.insert_user_default().await;
    insert_tenant_member(&app, fx.tp.tenant_id, user.id, TenantRole::Admin).await;
    login(&mut app, &user).await;

    let code = code_for(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    remove_tenant_member(&app, fx.tp.tenant_id, user.id).await;
    assert_eq!(
        oauth_error(exchange(&app, &code).await).await,
        invalid_grant()
    );
    assert!(grants_of(&app, user.id).await.is_empty());

    // 対照: 下げただけなら通る（交換はロールを見ない）
    insert_tenant_member(&app, fx.tp.tenant_id, user.id, TenantRole::Admin).await;
    let code = code_for(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    set_tenant_role(&app, fx.tp.tenant_id, user.id, TenantRole::Member).await;
    tokens_of(exchange(&app, &code).await).await;
}

fn pending_for(fx: &Fixture) -> PendingAuthorization {
    PendingAuthorization {
        grant_id: Uuid::new_v4(),
        user_id: fx.user.id,
        client_id: TEST_OAUTH_CLIENT.to_string(),
        client_name: "Test Client".to_string(),
        redirect_uri: REDIRECT_URI.to_string(),
        code_challenge: s256_challenge(VERIFIER),
        scopes: vec![Scope::ReadTenant, Scope::ReadProject],
        tenant_ids: vec![fx.tp.tenant_id],
        allow_refresh: true,
        issued_at_ms: Utc::now().timestamp_millis(),
    }
}

async fn issue(app: &TestApp, pending: &PendingAuthorization) -> String {
    grant::issue_code(&app.state.redis_client, pending)
        .await
        .expect("issue code")
}

async fn take(app: &TestApp, code: &str) -> CodeState {
    grant::take_code(&app.state.redis_client, code)
        .await
        .expect("take code")
}

async fn replayed(app: &TestApp, code: &str) -> bool {
    grant::code_was_replayed(&app.state.redis_client, code)
        .await
        .expect("replay flag")
}

/// 交換の本体（許可を作り、commit のあとで二重使用を確かめる）。トークンのエンドポイントが呼ぶものと同じ。
async fn create_for_code(
    app: &TestApp,
    redis: &RedisConnection,
    code: &str,
    pending: &PendingAuthorization,
) -> Result<Tokens, GrantError> {
    let issued = grant::create_grant_for_code(
        redis,
        &app.state.db,
        &app.state.settings.personal_token_secret,
        code,
        pending,
    )
    .await?;
    Ok(Tokens {
        access: issued.access_token,
        refresh: issued.refresh_token,
        scope: String::new(),
    })
}

/// 許可の行は取り消されていて、トークンの行は残っていない。
async fn assert_grant_revoked(app: &TestApp, grant_id: Uuid) {
    let row = personal_tokens::Entity::find_by_id(grant_id)
        .one(&app.state.db)
        .await
        .expect("find grant")
        .expect("許可の行は作られている");
    assert!(row.revoked, "許可は取り消されている");
    let tokens = oauth_tokens::Entity::find()
        .filter(oauth_tokens::Column::GrantId.eq(grant_id))
        .count(&app.state.db)
        .await
        .expect("count tokens");
    assert_eq!(tokens, 0, "トークンの行は残らない");
}

/// 2 つの交換が、後から来た側の取り消しのあとで先の側が許可を作る順に進んでも、許可は残らない。
#[tokio::test]
async fn code_replay_is_caught_when_the_second_exchange_revokes_before_the_grant_exists() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let pending = pending_for(&fx);
    let code = issue(&app, &pending).await;

    // A がコードを消費する
    assert_eq!(
        take(&app, &code).await,
        CodeState::Fresh(Box::new(pending.clone()))
    );
    // B が同じコードで来る。許可の行はまだ無いので、取り消しは何も変えない
    assert_eq!(take(&app, &code).await, CodeState::Used(pending.grant_id));
    grant::revoke_grant(&app.state.db, pending.grant_id)
        .await
        .expect("revoke before the grant exists");
    assert!(grants_of(&app, fx.user.id).await.is_empty());

    // A が許可を作る。commit のあとで B の印を見つけ、自分で取り消す
    let result = create_for_code(&app, &app.state.redis_client, &code, &pending).await;

    assert!(
        matches!(result, Err(GrantError::InvalidGrant)),
        "A はトークンを受け取らない"
    );
    assert_grant_revoked(&app, pending.grant_id).await;
}

#[tokio::test]
async fn code_replay_is_caught_when_the_second_exchange_comes_after_the_grant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let pending = pending_for(&fx);
    let code = issue(&app, &pending).await;

    assert!(matches!(take(&app, &code).await, CodeState::Fresh(_)));
    // 対照: B が来なければ印は無く、A はトークンを受け取り、トークンは通る
    let tokens = create_for_code(&app, &app.state.redis_client, &code, &pending)
        .await
        .expect("印が無ければ交換は成功する");
    assert!(!replayed(&app, &code).await);
    assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);

    assert_eq!(take(&app, &code).await, CodeState::Used(pending.grant_id));
    grant::revoke_grant(&app.state.db, pending.grant_id)
        .await
        .expect("revoke");

    assert_dead(&app, &tokens, "B が取り消す").await;
    assert!(replayed(&app, &code).await);
}

/// 印を読めなかったときは、確かめられないまま有効な許可を残さない。
#[tokio::test]
async fn grant_is_revoked_when_the_replay_flag_cannot_be_read() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let pending = pending_for(&fx);
    let code = issue(&app, &pending).await;
    assert!(matches!(take(&app, &code).await, CodeState::Fresh(_)));
    // 繋がらない Redis（閉じているポート）
    let unreachable = RedisConnection::new("redis://127.0.0.1:1");

    let result = create_for_code(&app, &unreachable, &code, &pending).await;

    assert!(
        matches!(result, Err(GrantError::Internal(_))),
        "読めなかったことは、拒否ではなく失敗として返す"
    );
    assert_grant_revoked(&app, pending.grant_id).await;
}

#[tokio::test]
async fn unknown_code_is_unknown() {
    let app = TestApp::new().await;
    assert_eq!(take(&app, "no-such-code").await, CodeState::Unknown);
    assert!(!replayed(&app, "no-such-code").await);
    assert_eq!(
        oauth_error(exchange(&app, "no-such-code").await).await,
        invalid_grant()
    );
}

// ---- リフレッシュトークン ----

#[tokio::test]
async fn refresh_rotates_the_pair_and_extends_the_grant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let first = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let grant = only_grant(&app, fx.user.id).await;
    set_grant_expiry(&app, grant.id, Utc::now() + Duration::days(1)).await;

    let second = tokens_of(refresh(&app, first.refresh()).await).await;

    assert_ne!(second.access, first.access);
    assert_ne!(second.refresh, first.refresh);
    assert_eq!(second.scope, ALL_SCOPES.join(" "));
    // 新旧のアクセストークンはどちらも期限まで通る
    assert_eq!(mcp_status(&app, &first.access).await, StatusCode::OK);
    assert_eq!(mcp_status(&app, &second.access).await, StatusCode::OK);
    // 許可の期限は、今から 90 日先に延びる
    let expires_at = only_grant(&app, fx.user.id)
        .await
        .expires_at
        .expect("expires_at")
        .with_timezone(&Utc);
    let drift = (expires_at - (Utc::now() + Duration::days(90)))
        .num_seconds()
        .abs();
    assert!(drift < 60, "90 日先（± 数秒）: {expires_at}");
}

#[tokio::test]
async fn reusing_a_rotated_refresh_token_revokes_the_grant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let first = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let second = tokens_of(refresh(&app, first.refresh()).await).await;

    assert_eq!(
        oauth_error(refresh(&app, first.refresh()).await).await,
        invalid_grant()
    );

    assert_dead(&app, &second, "回し済みのリフレッシュトークンの再利用").await;
    assert_eq!(
        mcp_status(&app, &first.access).await,
        StatusCode::UNAUTHORIZED
    );
    assert!(only_grant(&app, fx.user.id).await.revoked);
}

#[tokio::test]
async fn refresh_with_another_client_id_fails_without_touching_the_grant() {
    let mut app = app_with_native_client().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    for client_id in [NATIVE_CLIENT, "https://unlisted.test/oauth/metadata.json"] {
        assert_eq!(
            oauth_error(refresh_as(&app, tokens.refresh(), client_id).await).await,
            invalid_grant(),
            "{client_id}"
        );
    }
    // 形の違う値は DB を引かずに断る
    for bogus in [
        "not-a-refresh-token",
        tokens.access.as_str(),
        "mcprt_unknown",
    ] {
        assert_eq!(
            oauth_error(refresh(&app, bogus).await).await,
            invalid_grant(),
            "{bogus}"
        );
    }

    // 対照: 許可は生きている
    assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);
    tokens_of(refresh(&app, tokens.refresh()).await).await;
}

#[tokio::test]
async fn concurrent_refreshes_yield_one_success_and_revoke_the_grant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let grant = only_grant(&app, fx.user.id).await;

    let (a, b) = tokio::join!(
        refresh(&app, tokens.refresh()),
        refresh(&app, tokens.refresh())
    );

    let mut statuses = [a.status(), b.status()];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::BAD_REQUEST]);
    let live_refresh_tokens = oauth_tokens::Entity::find()
        .filter(oauth_tokens::Column::GrantId.eq(grant.id))
        .filter(oauth_tokens::Column::Kind.eq(oauth_tokens::KIND_REFRESH))
        .filter(oauth_tokens::Column::ConsumedAt.is_null())
        .count(&app.state.db)
        .await
        .expect("count refresh tokens");
    assert!(
        live_refresh_tokens <= 1,
        "回していない行が 2 つ以上残らない"
    );
    // 猶予は置かない。負けた側が再利用と判定され、許可は失効する
    assert!(only_grant(&app, fx.user.id).await.revoked);
    assert_eq!(
        mcp_status(&app, &tokens.access).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn refresh_ignores_the_scope_parameter() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let mut tokens = connect(&app, &[fx.tp.tenant_id], &["read:task"]).await;
    let wide = format!("{} offline_access", ALL_SCOPES.join(" "));

    for scope in [None, Some(wide.as_str()), Some("write:project bogus")] {
        let mut fields = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", tokens.refresh()),
            ("client_id", TEST_OAUTH_CLIENT),
        ];
        if let Some(scope) = scope {
            fields.push(("scope", scope));
        }
        let next = tokens_of(post_form(&app, "/oauth/token", &fields).await).await;
        assert_eq!(next.scope, "read:task", "{scope:?}");
        tokens = next;
    }

    let denied = create_task(&app, &tokens.access, fx.tp.project_id).await;
    assert!(
        denied.error().starts_with("403 forbidden"),
        "{}",
        denied.text
    );
    assert!(
        !list_tasks(&app, &tokens.access, fx.tp.project_id)
            .await
            .is_error
    );
}

#[tokio::test]
async fn client_without_the_refresh_grant_gets_no_refresh_token() {
    let mut app = app_with_native_client().await;
    let fx = fixture(&mut app).await;
    app.seed_oauth_client_metadata(&ClientMetadata {
        client_id: NATIVE_CLIENT.to_string(),
        client_name: "No Refresh".to_string(),
        client_uri: None,
        redirect_uris: vec![REDIRECT_URI.to_string()],
        allow_refresh: false,
    })
    .await;

    let res = app
        .post_json_with_session(
            "/v1/oauth_grants/authorize",
            approval(
                json!({ "client_id": NATIVE_CLIENT }),
                &[fx.tp.tenant_id],
                &ALL_SCOPES,
            ),
        )
        .await;
    let code = query_value(&query_of(&redirect_to_of(res).await), "code").expect("code");
    let res = post_form(
        &app,
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("code_verifier", VERIFIER),
            ("client_id", NATIVE_CLIENT),
        ],
    )
    .await;
    let body_has_refresh = {
        let tokens = tokens_of(res).await;
        assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);
        tokens.refresh.is_some()
    };
    assert!(!body_has_refresh, "refresh_token を返さない");
    // 許可の期限はアクセストークンと同じ 1 時間
    let expires_at = only_grant(&app, fx.user.id)
        .await
        .expires_at
        .expect("expires_at")
        .with_timezone(&Utc);
    let drift = (expires_at - (Utc::now() + Duration::hours(1)))
        .num_seconds()
        .abs();
    assert!(drift < 60, "1 時間先: {expires_at}");

    // 対照: 宣言するクライアントは refresh_token を得る
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    assert!(tokens.refresh.is_some());
}

// ---- 期限 ----

#[tokio::test]
async fn expired_tokens_are_rejected_at_the_boundary() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let grant = only_grant(&app, fx.user.id).await;
    let past = Utc::now() - Duration::seconds(1);
    // 期限の少し手前。遅い環境でも検査の前に切れない幅にする
    let soon = Utc::now() + Duration::seconds(10);

    set_token_expiry(&app, grant.id, oauth_tokens::KIND_ACCESS, past).await;
    assert_eq!(
        mcp_status(&app, &tokens.access).await,
        StatusCode::UNAUTHORIZED
    );
    set_token_expiry(&app, grant.id, oauth_tokens::KIND_ACCESS, soon).await;
    assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);

    set_token_expiry(&app, grant.id, oauth_tokens::KIND_REFRESH, past).await;
    assert_eq!(
        oauth_error(refresh(&app, tokens.refresh()).await).await,
        invalid_grant()
    );
    // 期限切れは再利用ではないので、許可は取り消さない
    assert!(!only_grant(&app, fx.user.id).await.revoked);
    set_token_expiry(&app, grant.id, oauth_tokens::KIND_REFRESH, soon).await;
    tokens_of(refresh(&app, tokens.refresh()).await).await;
}

#[tokio::test]
async fn expired_grant_is_rejected_at_the_boundary() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let grant = only_grant(&app, fx.user.id).await;

    set_grant_expiry(&app, grant.id, Utc::now() - Duration::seconds(1)).await;
    assert_dead(&app, &tokens, "許可の期限切れ").await;
    let res = app.get_with_session("/v1/oauth_grants").await;
    assert_eq!(json_body(res).await, json!([]), "期限切れは一覧に出ない");

    set_grant_expiry(&app, grant.id, Utc::now() + Duration::seconds(10)).await;
    assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);
    tokens_of(refresh(&app, tokens.refresh()).await).await;
}

// ---- 失効 ----

async fn grant_id_of(app: &TestApp, user_id: Uuid) -> Uuid {
    only_grant(app, user_id).await.id
}

#[tokio::test]
async fn disconnecting_from_settings_revokes_the_grant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let grant_id = grant_id_of(&app, fx.user.id).await;

    let res = app
        .delete_with_session(&format!("/v1/oauth_grants/{grant_id}"))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    assert_dead(&app, &tokens, "設定画面からの解除").await;
    // 取り消し済みでも 204
    let res = app
        .delete_with_session(&format!("/v1/oauth_grants/{grant_id}"))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn revocation_endpoint_revokes_by_access_token_or_refresh_token() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;

    let by_access = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let res = post_form(&app, "/oauth/revoke", &[("token", &by_access.access)]).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.text().await.expect("body"), "");
    assert_dead(&app, &by_access, "アクセストークンで失効").await;

    let by_refresh = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let res = post_form(
        &app,
        "/oauth/revoke",
        &[
            ("token", by_refresh.refresh()),
            ("token_type_hint", "refresh_token"),
            ("client_id", TEST_OAUTH_CLIENT),
        ],
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_dead(&app, &by_refresh, "リフレッシュトークンで失効").await;
}

#[tokio::test]
async fn revocation_endpoint_ignores_unknown_tokens_and_foreign_clients() {
    let mut app = app_with_native_client().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let pat = app
        .insert_pat(fx.user.id, fx.tp.tenant_id, vec![Scope::Api], None)
        .await;

    for token in ["mcpat_unknown", "mcprt_unknown", "garbage", pat.as_str()] {
        let res = post_form(&app, "/oauth/revoke", &[("token", token)]).await;
        assert_eq!(res.status(), StatusCode::OK, "{token}");
    }
    // PAT は失効のエンドポイントでは取り消せない
    let res = app.get_with_bearer("/v1/personal_tokens/me", &pat).await;
    assert_eq!(res.status(), StatusCode::OK);

    // 別の `client_id` を付けた要求は 200 だが、何もしない
    let res = post_form(
        &app,
        "/oauth/revoke",
        &[("token", &tokens.access), ("client_id", NATIVE_CLIENT)],
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);

    let res = post_form(
        &app,
        "/oauth/revoke",
        &[("token_type_hint", "access_token")],
    )
    .await;
    assert_eq!(
        oauth_error(res).await,
        (StatusCode::BAD_REQUEST, "invalid_request".to_string())
    );

    // 対照: 正しい `client_id` なら失効する
    let res = post_form(
        &app,
        "/oauth/revoke",
        &[("token", &tokens.access), ("client_id", TEST_OAUTH_CLIENT)],
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_dead(&app, &tokens, "正しい client_id で失効").await;
}

#[tokio::test]
async fn password_reset_revokes_the_grant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    reset_password(&app, &fx.user).await;

    assert_dead(&app, &tokens, "パスワードの再設定").await;
}

#[tokio::test]
async fn suspending_the_user_stops_the_grant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    // 停止だけを見る（管理者の停止の API は PAT も取り消すので、行を直に書き換える）
    entity::users::Entity::update_many()
        .col_expr(entity::users::Column::IsSuspended, Expr::value(true))
        .filter(entity::users::Column::Id.eq(fx.user.id))
        .exec(&app.state.db)
        .await
        .expect("suspend");

    let res = app.mcp(Some(&tokens.access), initialize_body()).await;
    assert_eq!(
        message_of(res).await,
        forbidden("account-suspended"),
        "停止した利用者のトークンは使うたびに 403"
    );
    assert_eq!(
        oauth_error(refresh(&app, tokens.refresh()).await).await,
        invalid_grant()
    );
}

#[tokio::test]
async fn admin_suspension_revokes_the_grant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let admin = app.insert_user(true, false).await;
    login(&mut app, &admin).await;

    let res = app
        .patch_json_with_session(
            &format!("/v1/admin/users/{}", fx.user.id),
            json!({ "is_suspended": true }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    assert_eq!(
        oauth_error(refresh(&app, tokens.refresh()).await).await,
        invalid_grant()
    );
    assert_ne!(mcp_status(&app, &tokens.access).await, StatusCode::OK);
    assert!(only_grant(&app, fx.user.id).await.revoked);
}

#[tokio::test]
async fn deleting_the_tenant_revokes_the_grant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tenant = insert_tenant(&app.state.db, fx.user.id).await;
    let tokens = connect(&app, &[tenant], &ALL_SCOPES).await;
    assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);

    let res = app
        .delete_with_session(&format!("/v1/tenants/{tenant}"))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    assert_dead(&app, &tokens, "テナントの削除").await;
}

#[tokio::test]
async fn revoke_all_unbinds_the_tenant_and_keeps_grants_bound_elsewhere() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let second_tenant = insert_tenant(&app.state.db, fx.user.id).await;
    let only_first = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let both = connect(&app, &[fx.tp.tenant_id, second_tenant], &ALL_SCOPES).await;

    let res = app
        .delete_json_with_session(
            "/v1/personal_tokens/revoke-all",
            json!({ "confirm_tenant_id": fx.tp.tenant_id }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    assert_dead(&app, &only_first, "束縛が尽きた許可").await;
    // 対照: 2 テナントに束縛した許可は、残りのテナントで通る
    assert!(
        !list_projects(&app, &both.access, second_tenant)
            .await
            .is_error
    );
    assert_eq!(
        list_projects(&app, &both.access, fx.tp.tenant_id)
            .await
            .error(),
        "403 forbidden"
    );
    tokens_of(refresh(&app, both.refresh()).await).await;
}

#[tokio::test]
async fn removing_a_client_from_the_allowlist_stops_its_grants() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    // 同じ DB と Redis で、許可リストだけが違うサーバー
    for allowlist in ["https://other.test/oauth/metadata.json", "none"] {
        let mut narrowed = TestApp::new_with_oauth_clients(allowlist).await;
        assert_eq!(
            mcp_status(&narrowed, &tokens.access).await,
            StatusCode::UNAUTHORIZED,
            "{allowlist}"
        );
        assert_eq!(
            oauth_error(refresh(&narrowed, tokens.refresh()).await).await,
            invalid_grant(),
            "{allowlist}"
        );
        login(&mut narrowed, &fx.user).await;
        let res = narrowed.get_with_session("/v1/oauth_grants").await;
        assert_eq!(json_body(res).await, json!([]), "{allowlist}");
        let res = preview(&narrowed, json!({})).await;
        assert_eq!(
            message_of(res).await,
            bad_request("oauth-client-not-allowed"),
            "{allowlist}"
        );
    }

    // 対照: リストに戻せば（元のサーバーでは）期限の内の許可は使える。`*` でも通る
    assert_eq!(mcp_status(&app, &tokens.access).await, StatusCode::OK);
    let any = TestApp::new_with_oauth_clients("*").await;
    assert_eq!(mcp_status(&any, &tokens.access).await, StatusCode::OK);
    tokens_of(refresh(&app, tokens.refresh()).await).await;
}

// ---- 連携中のアプリの管理 ----

#[tokio::test]
async fn grant_list_shows_only_the_users_live_grants() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let other = app.insert_user_default().await;
    let other_tenant = insert_tenant(&app.state.db, other.id).await;

    login(&mut app, &other).await;
    connect(&app, &[other_tenant], &ALL_SCOPES).await;
    let others_grant = grant_id_of(&app, other.id).await;

    login(&mut app, &fx.user).await;
    let revoked = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let res = post_form(&app, "/oauth/revoke", &[("token", &revoked.access)]).await;
    assert_eq!(res.status(), StatusCode::OK);
    connect(&app, &[fx.tp.tenant_id], &["read:task", "read:project"]).await;
    // PAT は連携中のアプリに出ない
    app.insert_pat(fx.user.id, fx.tp.tenant_id, vec![Scope::Api], None)
        .await;

    let res = app.get_with_session("/v1/oauth_grants").await;
    assert_eq!(res.status(), StatusCode::OK);
    let listed = json_body(res).await;
    let listed = listed.as_array().expect("grants");
    assert_eq!(listed.len(), 1, "{listed:?}");
    let shown = &listed[0];
    assert_eq!(shown["client_id"], TEST_OAUTH_CLIENT);
    assert_eq!(shown["client_name"], "Test Client");
    assert_eq!(shown["tenant_ids"], json!([fx.tp.tenant_id]));
    assert_eq!(shown["scopes"], json!(["read:project", "read:task"]));
    assert!(shown["expires_at"].is_string());
    assert!(shown["last_used_at"].is_null());

    // 他人の許可は解除できない
    let res = app
        .delete_with_session(&format!("/v1/oauth_grants/{others_grant}"))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    // PAT の id も、連携中のアプリとしては 404
    let pat_id = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(fx.user.id))
        .filter(personal_tokens::Column::OauthClientId.is_null())
        .one(&app.state.db)
        .await
        .expect("find pat")
        .expect("pat")
        .id;
    let res = app
        .delete_with_session(&format!("/v1/oauth_grants/{pat_id}"))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 対照: 自分の許可は 204
    let own = shown["id"].as_str().expect("id");
    let res = app
        .delete_with_session(&format!("/v1/oauth_grants/{own}"))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = app.get_with_session("/v1/oauth_grants").await;
    assert_eq!(json_body(res).await, json!([]));
}

#[tokio::test]
async fn grant_row_never_authenticates_as_a_pat() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    let grant = only_grant(&app, fx.user.id).await;
    // 許可の行の `token_hash` を、平文の分かっている値のハッシュに書き換える
    let known = "pat_known-plaintext-for-the-grant-row";
    let hash = backend::utils::auth::create_personal_token_hash(
        known,
        &app.state.settings.personal_token_secret,
    )
    .expect("hash");
    personal_tokens::Entity::update_many()
        .col_expr(personal_tokens::Column::TokenHash, Expr::value(hash))
        .filter(personal_tokens::Column::Id.eq(grant.id))
        .exec(&app.state.db)
        .await
        .expect("rewrite hash");

    let res = app.get_with_bearer("/v1/personal_tokens/me", known).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    // `/mcp` でも PAT としては通らない
    assert_eq!(mcp_status(&app, known).await, StatusCode::UNAUTHORIZED);
}

// ---- テナントから外れたあと ----

#[tokio::test]
async fn grant_of_a_removed_member_is_kept_but_blocked_in_that_tenant() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let user = app.insert_user_default().await;
    insert_tenant_member(&app, fx.tp.tenant_id, user.id, TenantRole::Member).await;
    login(&mut app, &user).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;
    assert!(
        !list_projects(&app, &tokens.access, fx.tp.tenant_id)
            .await
            .is_error
    );

    remove_tenant_member(&app, fx.tp.tenant_id, user.id).await;

    let projects = list_projects(&app, &tokens.access, fx.tp.tenant_id).await;
    assert_eq!(projects.error(), "403 tenant-membership-missing");
    let tasks = list_tasks(&app, &tokens.access, fx.tp.project_id).await;
    assert!(tasks.error().starts_with("403"), "{}", tasks.text);
    // 取り消さない。リフレッシュトークンは回せる
    let tokens = tokens_of(refresh(&app, tokens.refresh()).await).await;
    assert!(!only_grant(&app, user.id).await.revoked);

    // 戻れば再び使える
    insert_tenant_member(&app, fx.tp.tenant_id, user.id, TenantRole::Member).await;
    assert!(
        !list_projects(&app, &tokens.access, fx.tp.tenant_id)
            .await
            .is_error
    );
    assert!(
        !list_tasks(&app, &tokens.access, fx.tp.project_id)
            .await
            .is_error
    );
}

#[tokio::test]
async fn removed_member_still_reaches_projects_that_name_them() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let other_project = insert_extra_project(&app, fx.tp.tenant_id).await;
    let user = app.insert_user_default().await;
    insert_tenant_member(&app, fx.tp.tenant_id, user.id, TenantRole::Member).await;
    insert_project_member(&app, fx.tp.project_id, user.id).await;
    login(&mut app, &user).await;
    let tokens = connect(&app, &[fx.tp.tenant_id], &ALL_SCOPES).await;

    remove_tenant_member(&app, fx.tp.tenant_id, user.id).await;

    // 明示の指定が残るプロジェクトには届く（PAT とセッションにも共通の既存の規則）
    assert!(
        !list_tasks(&app, &tokens.access, fx.tp.project_id)
            .await
            .is_error
    );
    let denied = list_tasks(&app, &tokens.access, other_project).await;
    assert!(denied.error().starts_with("403"), "{}", denied.text);
    // プロジェクトの一覧は、明示の指定が在るプロジェクトだけに絞られる
    let projects = list_projects(&app, &tokens.access, fx.tp.tenant_id)
        .await
        .json();
    let ids: Vec<&str> = projects
        .as_array()
        .expect("projects")
        .iter()
        .map(|project| project["id"].as_str().expect("id"))
        .collect();
    let named = projects::Entity::find_by_id(fx.tp.project_id)
        .one(&app.state.db)
        .await
        .expect("find project")
        .expect("project");
    assert_eq!(ids, [named.id.to_string().as_str()]);
}
