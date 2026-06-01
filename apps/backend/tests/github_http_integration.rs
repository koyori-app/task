mod common;

use axum::http::StatusCode;
use backend::{
    entities::github_integrations,
    utils::github_oauth_state::{self, GithubOAuthStatePayload},
};
use common::{TestApp, TestTenantProject};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mount_github_api_mocks(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path_regex(r"^/app/installations/\d+/access_tokens$"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "token": "ghs_test_installation_token",
            "expires_at": "2030-01-01T00:00:00Z"
        })))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/app/installations/\d+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "account": { "login": "acme" }
        })))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path("/installation/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "repositories": [{
                "full_name": "acme/backend",
                "owner": { "login": "acme" }
            }]
        })))
        .mount(server)
        .await;

    Mock::given(method("DELETE"))
        .and(path_regex(r"^/app/installations/\d+$"))
        .respond_with(ResponseTemplate::new(204))
        .mount(server)
        .await;
}

fn unique_installation_id() -> i64 {
    300_000_000_000_i64 + (Uuid::new_v4().as_u128() % 900_000_000_000) as i64
}

fn install_path(tp: &TestTenantProject) -> String {
    format!(
        "/v1/tenants/{}/projects/{}/github/install",
        tp.tenant_id, tp.project_id
    )
}

fn callback_path(state: &str, installation_id: i64) -> String {
    format!(
        "/v1/github/callback?state={}&installation_id={installation_id}",
        urlencoding::encode(state)
    )
}

fn integration_path(tp: &TestTenantProject) -> String {
    format!(
        "/v1/tenants/{}/projects/{}/github/integration",
        tp.tenant_id, tp.project_id
    )
}

#[tokio::test]
async fn github_http_integration_suite() {
    let mock_server = MockServer::start().await;
    // SAFETY: set before TestApp::new spawns the server task.
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    mount_github_api_mocks(&mock_server).await;

    let mut app = TestApp::new_with_github().await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // 1. GET /install — GitHub インストール URL へリダイレクト
    {
        let user = app.insert_user(false, false).await;
        let tp = app.insert_tenant_project(user.id).await;
        app.login_session(&user.email, &user.password).await;

        let response = app.get_with_session(&install_path(&tp)).await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let location = response
            .headers()
            .get("location")
            .expect("location header")
            .to_str()
            .expect("location utf8");
        assert!(location.contains("github.com/apps/task-app/installations/new"));
        assert!(location.contains("state="));

        app.cleanup_user(user.id).await;
        app.reset_session_client();
    }

    // 2. GET /callback 正常系 — state 一致・DB に integration 作成
    {
        let user = app.insert_user(false, false).await;
        let tp = app.insert_tenant_project(user.id).await;
        app.login_session(&user.email, &user.password).await;

        let installation_id = unique_installation_id();
        let state_token = github_oauth_state::new_state_token();
        github_oauth_state::store_state(
            &app.state.redis_client,
            &state_token,
            &GithubOAuthStatePayload {
                tenant_id: tp.tenant_id,
                project_id: tp.project_id,
                user_id: user.id,
                installation_id: None,
            },
        )
        .await
        .expect("store oauth state");

        let response = app
            .get_with_session(&callback_path(&state_token, installation_id))
            .await;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        assert!(
            status == StatusCode::FOUND || status == StatusCode::TEMPORARY_REDIRECT,
            "callback failed: status={status} body={body}"
        );

        let row = github_integrations::Entity::find()
            .filter(github_integrations::Column::ProjectId.eq(tp.project_id))
            .one(&app.state.db)
            .await
            .expect("query integration")
            .expect("integration row");
        assert_eq!(row.installation_id, installation_id);
        assert_eq!(row.repo_owner, "acme");
        assert_eq!(row.repo_name, "backend");

        app.cleanup_user(user.id).await;
        app.reset_session_client();
    }

    // 3. GET /callback — 無効な state → 400
    {
        let user = app.insert_user(false, false).await;
        app.login_session(&user.email, &user.password).await;

        let response = app
            .get_with_session(&callback_path("nonexistent-state-token", unique_installation_id()))
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup_user(user.id).await;
        app.reset_session_client();
    }

    // 4. GET /callback — installation_id が state と不一致 → 400
    {
        let user = app.insert_user(false, false).await;
        let tp = app.insert_tenant_project(user.id).await;
        app.login_session(&user.email, &user.password).await;

        let bound_id = unique_installation_id();
        let state_token = github_oauth_state::new_state_token();
        github_oauth_state::store_state(
            &app.state.redis_client,
            &state_token,
            &GithubOAuthStatePayload {
                tenant_id: tp.tenant_id,
                project_id: tp.project_id,
                user_id: user.id,
                installation_id: Some(bound_id),
            },
        )
        .await
        .expect("store oauth state");

        let response = app
            .get_with_session(&callback_path(&state_token, bound_id + 1))
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        app.cleanup_user(user.id).await;
        app.reset_session_client();
    }

    // 5. DELETE /integration 正常系
    {
        let user = app.insert_user(false, false).await;
        let tp = app.insert_tenant_project(user.id).await;
        app.login_session(&user.email, &user.password).await;

        let installation_id = unique_installation_id();
        let state_token = github_oauth_state::new_state_token();
        github_oauth_state::store_state(
            &app.state.redis_client,
            &state_token,
            &GithubOAuthStatePayload {
                tenant_id: tp.tenant_id,
                project_id: tp.project_id,
                user_id: user.id,
                installation_id: None,
            },
        )
        .await
        .expect("store oauth state");

        let callback = app
            .get_with_session(&callback_path(&state_token, installation_id))
            .await;
        let cb_status = callback.status();
        assert!(
            cb_status == StatusCode::FOUND || cb_status == StatusCode::TEMPORARY_REDIRECT,
            "callback status={cb_status}"
        );

        let delete = app.delete_with_session(&integration_path(&tp)).await;
        assert_eq!(delete.status(), StatusCode::NO_CONTENT);

        let remaining = github_integrations::Entity::find()
            .filter(github_integrations::Column::ProjectId.eq(tp.project_id))
            .one(&app.state.db)
            .await
            .expect("query integration");
        assert!(remaining.is_none());

        app.cleanup_user(user.id).await;
        app.reset_session_client();
    }

    // 6. DELETE /integration — 未連携 → 404
    {
        let user = app.insert_user(false, false).await;
        let tp = app.insert_tenant_project(user.id).await;
        app.login_session(&user.email, &user.password).await;

        let response = app.delete_with_session(&integration_path(&tp)).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        app.cleanup_user(user.id).await;
    }
}
