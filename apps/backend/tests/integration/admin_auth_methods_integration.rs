use crate::common::TestApp;
use axum::http::StatusCode;
use chrono::Utc;
use entity::{oauth_connections, passkeys};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

#[tokio::test]
async fn admin_auth_methods_returns_no_secrets_and_rejects_non_admin() {
    let mut app = TestApp::new().await;
    let admin = app.insert_user(true, false).await;
    let normal = app.insert_user(false, false).await;
    let target = app.insert_user(false, false).await;
    let now = Utc::now().fixed_offset();

    let passkey_id = Uuid::new_v4();
    passkeys::ActiveModel {
        id: Set(passkey_id),
        user_id: Set(target.id),
        credential_id: Set(b"SECRET-CREDENTIAL-ID".to_vec()),
        public_key: Set(b"SECRET-PUBLIC-KEY".to_vec()),
        aaguid: Set(None),
        sign_count: Set(7),
        name: Set("My Key".into()),
        last_used_at: Set(None),
        created_at: Set(now),
    }
    .insert(&app.state.db)
    .await
    .expect("insert passkey");
    oauth_connections::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(target.id),
        provider: Set("github".into()),
        provider_user_id: Set("SECRET-PROVIDER-UID".into()),
        provider_email: Set(Some("gh@example.com".into())),
        instance_url: Set(None),
        access_token_enc: Set(Some("SECRET-ACCESS-TOKEN".into())),
        refresh_token_enc: Set(Some("SECRET-REFRESH-TOKEN".into())),
        token_expires_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        provider_login: Set(Some("octocat".into())),
    }
    .insert(&app.state.db)
    .await
    .expect("insert oauth");

    let path = format!("/v1/admin/users/{}/auth-methods", target.id);

    // 非 admin は 403
    app.reset_session_client();
    app.login_session(&normal.email, &normal.password).await;
    assert_eq!(
        app.get_with_session(&path).await.status(),
        StatusCode::FORBIDDEN
    );

    // admin は 200。秘密値を含まない
    app.reset_session_client();
    app.login_session(&admin.email, &admin.password).await;
    let res = app.get_with_session(&path).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.text().await.expect("body");
    assert!(!body.contains("SECRET"), "secret leaked: {body}");
    assert!(!body.contains("public_key") && !body.contains("token"));
    let json: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(json["passkeys"][0]["id"], passkey_id.to_string());
    assert_eq!(json["passkeys"][0]["name"], "My Key");
    assert_eq!(json["oauth"][0]["provider"], "github");
    assert_eq!(json["oauth"][0]["provider_login"], "octocat");

    // 存在しない利用者は 404
    let missing = format!("/v1/admin/users/{}/auth-methods", Uuid::new_v4());
    assert_eq!(
        app.get_with_session(&missing).await.status(),
        StatusCode::NOT_FOUND
    );

    // 管理者の復旧操作（パスキー削除）の対照: 一覧の id でそのまま削除でき、一覧から消える
    let del = app
        .delete_with_session(&format!(
            "/v1/admin/users/{}/passkeys/{}",
            target.id, passkey_id
        ))
        .await;
    assert_eq!(del.status(), StatusCode::NO_CONTENT);
    let after: serde_json::Value = app
        .get_with_session(&path)
        .await
        .json()
        .await
        .expect("json");
    assert!(after["passkeys"].as_array().expect("arr").is_empty());

    for u in [admin.id, normal.id, target.id] {
        app.cleanup_user(u).await;
    }
}

#[tokio::test]
async fn admin_registration_policy_toggles_register_api() {
    let mut app = TestApp::new().await;
    let admin = app.insert_user(true, false).await;
    let normal = app.insert_user(false, false).await;
    ::common::system_settings::ensure_system_settings_row(&app.state.db)
        .await
        .expect("ensure settings row");
    let register = |email: String| {
        serde_json::json!({
            "username": "policy_user",
            "email": email,
            "password": "TestPassword123!"
        })
    };

    // 非 admin は設定を変えられない
    app.reset_session_client();
    app.login_session(&normal.email, &normal.password).await;
    let denied = app
        .patch_json_with_session(
            "/v1/admin/system/settings",
            serde_json::json!({ "user_registration_enabled": false }),
        )
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    app.reset_session_client();
    app.login_session(&admin.email, &admin.password).await;
    let set = |enabled: bool| {
        app.patch_json_with_session(
            "/v1/admin/system/settings",
            serde_json::json!({ "user_registration_enabled": enabled }),
        )
    };

    // 無効: 現在値が返り、登録 API は 403
    let res = set(false).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = res.json().await.expect("json");
    assert_eq!(body["user_registration_enabled"], false);
    let blocked = app
        .post_json(
            "/v1/auth/register",
            register(format!("policy-off-{}@example.com", Uuid::new_v4())),
        )
        .await;
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);

    // 有効に戻す: 登録 API は 201（過剰拒否でない）
    let res = set(true).await;
    let body: serde_json::Value = res.json().await.expect("json");
    assert_eq!(body["user_registration_enabled"], true);
    let email = format!("policy-on-{}@example.com", Uuid::new_v4());
    let ok = app
        .post_json("/v1/auth/register", register(email.clone()))
        .await;
    assert_eq!(ok.status(), StatusCode::CREATED);

    if let Some(u) = entity::users::Entity::find()
        .filter(entity::users::Column::Email.eq(&email))
        .one(&app.state.db)
        .await
        .expect("query user")
    {
        app.cleanup_user(u.id).await;
    }
    app.cleanup_user(admin.id).await;
    app.cleanup_user(normal.id).await;
}
