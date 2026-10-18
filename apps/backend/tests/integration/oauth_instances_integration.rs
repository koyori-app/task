//! GitLab self-hosted の instance の許可の一覧。
//!
//! 一覧に無い instance へは、認可の開始も callback のトークン交換も行わない。
//! 一覧に無い instance を mock の server として立て、その口が一度も叩かれぬことで確かめる。

use crate::common::{MockGitLabUser, MockOAuthHandle, TestApp, is_redirect};
use service::oauth::{consume_state, store_state};
use std::sync::atomic::Ordering;
use url::Url;

async fn unlisted_instance() -> MockOAuthHandle {
    MockOAuthHandle::start(MockGitLabUser {
        id: 990_001,
        username: "unlisted_instance_user".to_string(),
        email: Some(format!("unlisted-{}@example.com", uuid::Uuid::new_v4())),
    })
    .await
}

fn state_of(start: &reqwest::Response) -> String {
    let location = start
        .headers()
        .get("location")
        .expect("authorize redirect")
        .to_str()
        .expect("authorize url");
    Url::parse(location)
        .expect("authorize url parse")
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.to_string())
        .expect("state query param")
}

#[tokio::test]
async fn oauth_start_rejects_instance_not_in_allowlist() {
    let app = TestApp::new().await;
    let unlisted = unlisted_instance().await;

    let start = app
        .client()
        .get(format!(
            "{}/v1/auth/oauth/gitlab_selfhosted?instance_url={}",
            app.base_url(),
            urlencoding::encode(&unlisted.base_url)
        ))
        .send()
        .await
        .expect("oauth start request");

    assert!(
        !is_redirect(start.status()),
        "oauth start must not redirect to an instance outside the allowlist: {}",
        start.status()
    );
    assert!(
        start.status().is_client_error(),
        "status: {}",
        start.status()
    );
    assert_eq!(unlisted.hits.authorize.load(Ordering::SeqCst), 0);
    assert_eq!(unlisted.hits.backchannel(), 0);
}

#[tokio::test]
async fn oauth_callback_rejects_instance_not_in_allowlist() {
    let app = TestApp::new().await;
    let unlisted = unlisted_instance().await;

    // 許可された instance で開始し、控えた state の instance だけを一覧に無いものへ差し替える
    let start = app.oauth_start(false).await;
    assert!(is_redirect(start.status()), "oauth start redirect");
    let state = state_of(&start);
    let mut payload = consume_state(&app.state.redis_client, &state)
        .await
        .expect("consume state")
        .expect("stored state");
    payload.instance_url = Some(unlisted.base_url.clone());
    store_state(&app.state.redis_client, &state, &payload)
        .await
        .expect("store state");

    let callback = app.follow_oauth_start(start).await;
    assert!(
        callback.status().is_client_error(),
        "callback for an instance outside the allowlist must be rejected: {}",
        callback.status()
    );
    // client secret を送る token の口も、利用者情報の口も叩かれぬ
    assert_eq!(unlisted.hits.backchannel(), 0);
    assert_eq!(
        app.get_me().await.status(),
        axum::http::StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn oauth_start_accepts_allowlisted_instance_in_another_spelling() {
    let app = TestApp::new().await;
    // scheme の大小と末尾の slash が違っても、同じ instance として受ける
    let spelled = format!("{}/", app.instance_url().replacen("http://", "HTTP://", 1));
    let start = app
        .client()
        .get(format!(
            "{}/v1/auth/oauth/gitlab_selfhosted?instance_url={}",
            app.base_url(),
            urlencoding::encode(&spelled)
        ))
        .send()
        .await
        .expect("oauth start request");
    assert!(is_redirect(start.status()), "status: {}", start.status());
    let location = start.headers()["location"].to_str().expect("location");
    assert!(
        location.starts_with(&format!("{}/oauth/authorize", app.instance_url())),
        "unexpected authorize url: {location}"
    );
}

async fn me_id(app: &TestApp) -> Option<uuid::Uuid> {
    let me = app.get_me().await;
    if me.status() != axum::http::StatusCode::OK {
        return None;
    }
    let body: serde_json::Value = me.json().await.expect("me json");
    body["id"].as_str().and_then(|id| id.parse().ok())
}

async fn user_by_email(app: &TestApp, email: &str) -> Option<entity::users::Model> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    entity::users::Entity::find()
        .filter(entity::users::Column::Email.eq(email))
        .one(&app.state.db)
        .await
        .expect("query user")
}

async fn set_registration_enabled(app: &TestApp, enabled: bool) {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, sea_query::Expr};
    common::system_settings::ensure_system_settings_row(&app.state.db)
        .await
        .expect("ensure system settings");
    entity::system_settings::Entity::update_many()
        .col_expr(
            entity::system_settings::Column::UserRegistrationEnabled,
            Expr::value(enabled),
        )
        .filter(entity::system_settings::Column::Singleton.eq(true))
        .exec(&app.state.db)
        .await
        .expect("update system settings");
}

fn mock_user(tag: &str) -> MockGitLabUser {
    let unique = uuid::Uuid::new_v4().simple().to_string();
    MockGitLabUser {
        id: i64::from_str_radix(&unique[..12], 16).expect("hex id") % 1_000_000_000,
        username: format!("{tag}_{}", &unique[..8]),
        email: Some(format!("{tag}-{unique}@example.com")),
    }
}

#[tokio::test]
async fn oauth_selfhosted_user_is_created_with_unverified_email() {
    let app = TestApp::new().await;
    let user = mock_user("selfhosted_unverified");
    let email = user.email.clone().expect("email");
    // mock は confirmed_at を返すが、selfhosted の印は採らぬ
    app.set_mock_user(user);

    let callback = app.follow_oauth_start(app.oauth_start(false).await).await;
    assert!(
        is_redirect(callback.status()),
        "status: {}",
        callback.status()
    );

    let created = user_by_email(&app, &email).await.expect("created user");
    assert!(
        !created.email_verified,
        "selfhosted email must start unverified"
    );
    app.cleanup_user(created.id).await;
}

#[tokio::test]
async fn oauth_creates_no_user_while_registration_is_closed_but_existing_users_sign_in() {
    let mut app = TestApp::new().await;
    let existing = mock_user("registration_existing");
    app.set_mock_user(existing.clone());
    let callback = app.follow_oauth_start(app.oauth_start(false).await).await;
    assert!(
        is_redirect(callback.status()),
        "status: {}",
        callback.status()
    );
    let existing_id = me_id(&app).await.expect("existing user signed in");

    set_registration_enabled(&app, false).await;

    // 既存の利用者は、登録を止めても OAuth でログインできる
    app.reset_session_client();
    app.set_mock_user(existing);
    let existing_login = app.follow_oauth_start(app.oauth_start(false).await).await;
    let signed_in_again = me_id(&app).await;

    // 新しい利用者は作らぬ
    app.reset_session_client();
    let newcomer = mock_user("registration_newcomer");
    let newcomer_email = newcomer.email.clone().expect("email");
    app.set_mock_user(newcomer);
    let newcomer_login = app.follow_oauth_start(app.oauth_start(false).await).await;
    let newcomer_row = user_by_email(&app, &newcomer_email).await;

    set_registration_enabled(&app, true).await;

    assert!(
        is_redirect(existing_login.status()),
        "status: {}",
        existing_login.status()
    );
    assert_eq!(signed_in_again, Some(existing_id));
    assert_eq!(newcomer_login.status(), axum::http::StatusCode::FORBIDDEN);
    assert!(
        newcomer_row.is_none(),
        "no user must be created while registration is closed"
    );
    app.cleanup_user(existing_id).await;
}

async fn insert_connection(
    app: &TestApp,
    user_id: uuid::Uuid,
    provider: &str,
    instance_url: Option<&str>,
) {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    let now = chrono::Utc::now();
    entity::oauth_connections::ActiveModel {
        id: Set(uuid::Uuid::new_v4()),
        user_id: Set(user_id),
        provider: Set(provider.to_string()),
        provider_user_id: Set(uuid::Uuid::new_v4().to_string()),
        provider_email: Set(None),
        instance_url: Set(instance_url.map(str::to_string)),
        access_token_enc: Set(None),
        refresh_token_enc: Set(None),
        token_expires_at: Set(None),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        provider_login: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert connection");
}

async fn connections_of(app: &TestApp, user_id: uuid::Uuid) -> Vec<(String, Option<String>)> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
    entity::oauth_connections::Entity::find()
        .filter(entity::oauth_connections::Column::UserId.eq(user_id))
        .order_by_asc(entity::oauth_connections::Column::Provider)
        .all(&app.state.db)
        .await
        .expect("list connections")
        .into_iter()
        .map(|row| (row.provider, row.instance_url))
        .collect()
}

async fn complete_password_reset(app: &TestApp, user_id: uuid::Uuid) {
    let token = format!("test-token-{}", uuid::Uuid::new_v4());
    backend::utils::password_reset::store_token(&app.state.redis_client, user_id, &token)
        .await
        .expect("store token");
    let complete = app
        .post_json(
            "/v1/auth/password-reset/complete",
            serde_json::json!({ "token": token, "new_password": "NewPassword123!" }),
        )
        .await;
    assert_eq!(complete.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn password_reset_detaches_connections_of_an_unverified_account() {
    let app = TestApp::new().await;
    let user = mock_user("reset_unverified");
    let email = user.email.clone().expect("email");
    app.set_mock_user(user);
    let callback = app.follow_oauth_start(app.oauth_start(false).await).await;
    assert!(
        is_redirect(callback.status()),
        "status: {}",
        callback.status()
    );
    let created = user_by_email(&app, &email).await.expect("created user");
    assert!(!created.email_verified);
    assert_eq!(connections_of(&app, created.id).await.len(), 1);

    complete_password_reset(&app, created.id).await;

    assert!(connections_of(&app, created.id).await.is_empty());
    app.cleanup_user(created.id).await;
}

#[tokio::test]
async fn password_reset_keeps_allowlisted_connections_of_a_verified_account() {
    let app = TestApp::new().await;
    let user = app.insert_user(false, false).await;
    insert_connection(&app, user.id, "gitlab_selfhosted", Some(app.instance_url())).await;
    insert_connection(
        &app,
        user.id,
        "gitlab_selfhosted",
        Some("https://gitlab.unlisted.example"),
    )
    .await;
    insert_connection(&app, user.id, "github", None).await;

    complete_password_reset(&app, user.id).await;

    assert_eq!(
        connections_of(&app, user.id).await,
        [
            ("github".to_string(), None),
            (
                "gitlab_selfhosted".to_string(),
                Some(app.instance_url().to_string())
            ),
        ]
    );
    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn bootstrap_admin_requires_a_verified_email_from_a_trusted_path() {
    use backend::utils::bootstrap_admin::bootstrap_ineligibility;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
    let app = TestApp::new().await;
    let db = &app.state.db;
    let model = |id| async move {
        entity::users::Entity::find_by_id(id)
            .one(db)
            .await
            .expect("query user")
            .expect("user row")
    };

    let verified = app.insert_user(false, false).await;
    assert_eq!(
        bootstrap_ineligibility(db, &model(verified.id).await)
            .await
            .unwrap(),
        None
    );

    let unverified = app.insert_user(false, false).await;
    let mut active: entity::users::ActiveModel = model(unverified.id).await.into();
    active.email_verified = Set(false);
    active.update(db).await.expect("mark unverified");
    assert!(
        bootstrap_ineligibility(db, &model(unverified.id).await)
            .await
            .unwrap()
            .is_some()
    );

    let selfhosted = app.insert_user(false, false).await;
    insert_connection(
        &app,
        selfhosted.id,
        "gitlab_selfhosted",
        Some(app.instance_url()),
    )
    .await;
    assert!(
        bootstrap_ineligibility(db, &model(selfhosted.id).await)
            .await
            .unwrap()
            .is_some()
    );

    for id in [verified.id, unverified.id, selfhosted.id] {
        app.cleanup_user(id).await;
    }
}
