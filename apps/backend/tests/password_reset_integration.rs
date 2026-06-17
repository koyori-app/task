mod common;

use axum::http::StatusCode;
use backend::entities::{personal_tokens, tenants, users};
use backend::utils::password_reset;
use chrono::Utc;
use common::{TestApp, insert_personal_token_for_test, insert_tenant};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

const RESET_MESSAGE: &str =
    "入力されたメールアドレスにリセットリンクを送信しました（登録済みの場合）";

#[tokio::test]
async fn password_reset_integration_suite() {
    let app = TestApp::new().await;

    // Test 1: 存在しないメールでも登録済みメールでも同一 200 レスポンス（列挙防止）
    {
        let user = app.insert_user(false, false).await;
        let unknown = format!("unknown-{}@example.com", uuid::Uuid::new_v4());

        let unknown_resp = app
            .post_json(
                "/v1/auth/password-reset/request",
                serde_json::json!({ "email": unknown }),
            )
            .await;
        assert_eq!(unknown_resp.status(), StatusCode::OK);
        let unknown_body: serde_json::Value = unknown_resp.json().await.expect("json");
        assert_eq!(
            unknown_body["message"].as_str(),
            Some(RESET_MESSAGE),
            "unknown email message"
        );

        let known_resp = app
            .post_json(
                "/v1/auth/password-reset/request",
                serde_json::json!({ "email": user.email }),
            )
            .await;
        assert_eq!(known_resp.status(), StatusCode::OK);
        let known_body: serde_json::Value = known_resp.json().await.expect("json");
        assert_eq!(
            known_body["message"].as_str(),
            Some(RESET_MESSAGE),
            "known email message"
        );

        app.cleanup_user(user.id).await;
    }

    // Test 2: トークン verify — 有効 200 / 無効 404
    {
        let user = app.insert_user(false, false).await;
        let token = format!("test-token-{}", uuid::Uuid::new_v4());
        password_reset::store_token(&app.state.redis_client, user.id, &token)
            .await
            .expect("store token");

        let valid = app
            .post_json(
                "/v1/auth/password-reset/verify",
                serde_json::json!({ "token": token }),
            )
            .await;
        assert_eq!(valid.status(), StatusCode::OK);

        let invalid = app
            .post_json(
                "/v1/auth/password-reset/verify",
                serde_json::json!({ "token": "not-a-real-token" }),
            )
            .await;
        assert_eq!(invalid.status(), StatusCode::NOT_FOUND);
        let body = invalid.text().await.expect("body");
        assert!(body.contains("password-reset-token-not-found"));

        app.cleanup_user(user.id).await;
    }

    // Test 3: reset complete 後、旧セッションは /me で 401
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user(false, false).await;
        app.login_session(&user.email, &user.password).await;

        let me_ok = app.get_with_session("/v1/auth/me").await;
        assert_eq!(me_ok.status(), StatusCode::OK);

        let token = format!("session-revoke-{}", uuid::Uuid::new_v4());
        password_reset::store_token(&app.state.redis_client, user.id, &token)
            .await
            .expect("store token");

        let tenant_id = insert_tenant(&app.state.db, user.id).await;
        let _pat = insert_personal_token_for_test(
            &app.state.db,
            user.id,
            tenant_id,
            &app.state.settings.personal_token_secret,
        )
        .await;
        let pat_id = personal_tokens::Entity::find()
            .filter(personal_tokens::Column::UserId.eq(user.id))
            .one(&app.state.db)
            .await
            .expect("load pat")
            .expect("pat exists")
            .id;

        let complete = app
            .post_json(
                "/v1/auth/password-reset/complete",
                serde_json::json!({
                    "token": token,
                    "new_password": "NewPassword456!"
                }),
            )
            .await;
        assert_eq!(complete.status(), StatusCode::OK);

        let me_revoked = app.get_with_session("/v1/auth/me").await;
        assert_eq!(me_revoked.status(), StatusCode::UNAUTHORIZED);

        let row = backend::entities::users::Entity::find_by_id(user.id)
            .one(&app.state.db)
            .await
            .expect("load user")
            .expect("user exists");
        assert!(row.sessions_revoked_at.is_some());

        let pat_row = personal_tokens::Entity::find_by_id(pat_id)
            .one(&app.state.db)
            .await
            .expect("load pat")
            .expect("pat exists");
        assert!(
            pat_row.revoked,
            "personal_tokens.revoked must be true after password reset complete"
        );

        let _ = tenants::Entity::delete_by_id(tenant_id)
            .exec(&app.state.db)
            .await;

        app.cleanup_user(user.id).await;
    }

    // Test 4: トークンは一度きり — 二回目の complete は 400
    {
        let user = app.insert_user(false, false).await;
        let token = format!("single-use-{}", uuid::Uuid::new_v4());
        password_reset::store_token(&app.state.redis_client, user.id, &token)
            .await
            .expect("store token");

        let first = app
            .post_json(
                "/v1/auth/password-reset/complete",
                serde_json::json!({
                    "token": token,
                    "new_password": "AnotherPass789!"
                }),
            )
            .await;
        assert_eq!(first.status(), StatusCode::OK);

        let second = app
            .post_json(
                "/v1/auth/password-reset/complete",
                serde_json::json!({
                    "token": token,
                    "new_password": "YetAnotherPass000!"
                }),
            )
            .await;
        assert_eq!(second.status(), StatusCode::BAD_REQUEST);
        let body = second.text().await.expect("body");
        assert!(body.contains("invalid-password-reset-token"));

        app.cleanup_user(user.id).await;
    }

    // Test 5: password change も sessions_revoked_at を更新し他セッションを無効化
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user(false, false).await;
        app.login_session(&user.email, &user.password).await;

        let change = app
            .post_json_with_session(
                "/v1/auth/password/change",
                serde_json::json!({
                    "current_password": user.password,
                    "new_password": "ChangedPass999!"
                }),
            )
            .await;
        assert_eq!(change.status(), StatusCode::OK);

        let me_revoked = app.get_with_session("/v1/auth/me").await;
        assert_eq!(me_revoked.status(), StatusCode::UNAUTHORIZED);

        let row = backend::entities::users::Entity::find_by_id(user.id)
            .one(&app.state.db)
            .await
            .expect("load user")
            .expect("user exists");
        assert!(row.sessions_revoked_at.is_some());
        assert!(row.sessions_revoked_at.unwrap() <= Utc::now());

        app.cleanup_user(user.id).await;
    }

    // Test 6: レート制限 — 60 秒以内に同一メールで 2 回 request すると 429
    {
        let user = app.insert_user(false, false).await;

        let first = app
            .post_json(
                "/v1/auth/password-reset/request",
                serde_json::json!({ "email": user.email }),
            )
            .await;
        assert_eq!(first.status(), StatusCode::OK);

        let second = app
            .post_json(
                "/v1/auth/password-reset/request",
                serde_json::json!({ "email": user.email }),
            )
            .await;
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);

        app.cleanup_user(user.id).await;
    }

    // Test 7: password/change — 現在パスワード不一致は 400
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user(false, false).await;
        app.login_session(&user.email, &user.password).await;

        let resp = app
            .post_json_with_session(
                "/v1/auth/password/change",
                serde_json::json!({
                    "current_password": "WrongPassword999!",
                    "new_password": "NewPassword123!"
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.text().await.expect("body");
        assert!(body.contains("invalid-current-password"));

        app.cleanup_user(user.id).await;
    }

    // Test 8: password/change — PAT 認証は 401
    {
        let app = TestApp::new().await;
        let user = app.insert_user(false, false).await;
        let tenant_id = insert_tenant(&app.state.db, user.id).await;
        let pat = insert_personal_token_for_test(
            &app.state.db,
            user.id,
            tenant_id,
            &app.state.settings.personal_token_secret,
        )
        .await;

        let resp = app
            .post_json_with_bearer(
                "/v1/auth/password/change",
                serde_json::json!({
                    "current_password": user.password,
                    "new_password": "NewPassword123!"
                }),
                &pat,
            )
            .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let _ = backend::entities::tenants::Entity::delete_by_id(tenant_id)
            .exec(&app.state.db)
            .await;
        app.cleanup_user(user.id).await;
    }

    // Test 9: password/change — password_hash が NULL（OAuth ユーザー）は 400
    // 通常ユーザーとしてログインしてからDB上で password_hash を NULL に更新し、
    // その後に password/change を呼ぶことで PasswordNotSet パスを検証する。
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user(false, false).await;
        app.login_session(&user.email, &user.password).await;

        // セッション確立後に password_hash を NULL へ更新（OAuth ユーザー相当）
        users::ActiveModel {
            id: Set(user.id),
            password_hash: Set(None),
            ..Default::default()
        }
        .update(&app.state.db)
        .await
        .expect("clear password hash");

        let resp = app
            .post_json_with_session(
                "/v1/auth/password/change",
                serde_json::json!({
                    "current_password": user.password,
                    "new_password": "NewPassword123!"
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = resp.text().await.expect("body");
        assert!(body.contains("password-not-set"));

        app.cleanup_user(user.id).await;
    }
}
