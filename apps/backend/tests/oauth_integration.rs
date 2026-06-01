mod common;

use axum::http::StatusCode;
use backend::entities::{oauth_connections, users};
use common::{is_redirect, MockGitLabUser, TestApp};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use url::Url;

#[tokio::test]
async fn oauth_start_callback_flow_issues_session() {
    let app = TestApp::new().await;
    let unique = uuid::Uuid::new_v4();
    app.set_mock_user(MockGitLabUser {
        id: 100_001,
        username: format!("oauth_user_{unique}"),
        email: Some(format!("oauth-flow-{unique}@example.com")),
    });

    let start = app.oauth_start(false).await;
    assert!(is_redirect(start.status()), "oauth start redirect");
    let authorize_url = start
        .headers()
        .get("location")
        .expect("authorize redirect")
        .to_str()
        .expect("authorize url");
    let parsed = Url::parse(authorize_url).expect("authorize url parse");
    let state = parsed
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.to_string())
        .expect("state query param");
    assert!(!state.is_empty(), "oauth state must be generated");

    let callback = app.follow_oauth_start(start).await;
    assert!(is_redirect(callback.status()), "oauth callback redirect");
    let frontend = callback
        .headers()
        .get("location")
        .expect("frontend redirect")
        .to_str()
        .expect("frontend url");
    assert!(
        frontend.starts_with("http://localhost:3000/dashboard"),
        "unexpected frontend redirect: {frontend}"
    );

    let me = app.get_me().await;
    assert_eq!(me.status(), StatusCode::OK);
    let body: serde_json::Value = me.json().await.expect("me json");
    assert_eq!(
        body["email"].as_str(),
        Some(format!("oauth-flow-{unique}@example.com").as_str())
    );

    let user_id: uuid::Uuid = body["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("uuid parse");
    assert_eq!(app.count_connections_for_user(user_id).await, 1);

    app.cleanup_user(user_id).await;
}

#[tokio::test]
async fn oauth_callback_rejects_state_mismatch() {
    let app = TestApp::new().await;
    let start = app.oauth_start(false).await;
    assert!(is_redirect(start.status()), "oauth start redirect");

    let response = app
        .get("/v1/auth/oauth/gitlab_selfhosted/callback?code=mock-auth-code&state=wrong-state-value")
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.text().await.expect("body");
    assert!(body.contains("invalid-oauth-state"));
}

#[tokio::test]
async fn oauth_disconnect_removes_connection() {
    let mut app = TestApp::new().await;
    let unique = uuid::Uuid::new_v4();
    let user = app.insert_oauth_user(None).await;
    app.login_session(&user.email, &user.password).await;

    app.set_mock_user(MockGitLabUser {
        id: 200_001,
        username: format!("linked_{unique}"),
        email: Some(format!("linked-{unique}@example.com")),
    });

    let start = app.oauth_start(true).await;
    let callback = app.follow_oauth_start(start).await;
    assert!(is_redirect(callback.status()), "oauth callback redirect");

    assert_eq!(app.count_connections_for_user(user.id).await, 1);

    let disconnect_path = format!(
        "/v1/auth/oauth/connections/gitlab_selfhosted?instance_url={}",
        urlencoding::encode(app.instance_url())
    );
    let disconnect = app.delete_with_session(&disconnect_path).await;
    assert_eq!(disconnect.status(), StatusCode::NO_CONTENT);
    assert_eq!(app.count_connections_for_user(user.id).await, 0);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn oauth_disconnect_not_found_for_unlinked_user() {
    let mut app = TestApp::new().await;
    let user = app.insert_oauth_user(None).await;
    app.login_session(&user.email, &user.password).await;

    let disconnect_path = format!(
        "/v1/auth/oauth/connections/gitlab_selfhosted?instance_url={}",
        urlencoding::encode(app.instance_url())
    );
    let response = app.delete_with_session(&disconnect_path).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response.text().await.expect("body");
    assert!(body.contains("oauth-connection-not-found"));

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn oauth_callback_returns_email_conflict() {
    let app = TestApp::new().await;
    let unique = uuid::Uuid::new_v4();
    let email = format!("conflict-{unique}@example.com");
    let existing = app.insert_oauth_user(Some(&email)).await;

    app.set_mock_user(MockGitLabUser {
        id: 300_001,
        username: format!("conflict_{unique}"),
        email: Some(email.clone()),
    });

    let start = app.oauth_start(false).await;
    let callback = app.follow_oauth_start(start).await;
    assert_eq!(callback.status(), StatusCode::CONFLICT);
    let body = callback.text().await.expect("body");
    assert!(body.contains("oauth-email-conflict"));

    let still_one = users::Entity::find()
        .filter(users::Column::Email.eq(email))
        .all(&app.state.db)
        .await
        .expect("query users")
        .len();
    assert_eq!(still_one, 1);

    let connections = oauth_connections::Entity::find()
        .filter(oauth_connections::Column::ProviderUserId.eq("300001"))
        .all(&app.state.db)
        .await
        .expect("query connections");
    assert!(connections.is_empty());

    app.cleanup_user(existing.id).await;
}
