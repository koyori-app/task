use crate::common::TestApp;
use axum::http::StatusCode;
use entity::{totp_credentials, users};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, PaginatorTrait, QueryFilter,
    Statement, TransactionTrait,
};

#[tokio::test]
async fn auth_2fa_integration_suite() {
    // Test 1: AuthUser rejects half_authed session on /me
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user_default().await;
        let _enabled = app.enable_2fa(&user).await;
        app.login_half_authed(&user).await;

        let me = app.get_with_session("/v1/auth/me").await;
        assert_eq!(me.status(), StatusCode::FORBIDDEN);

        app.cleanup_user(user.id).await;
    }

    // Test 2: HalfAuthedUser accepts POST /2fa/verify
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user_default().await;
        let enabled = app.enable_2fa(&user).await;
        app.login_half_authed(&user).await;

        let recovery = &enabled.recovery_codes[0];
        let verify = app
            .post_json_with_session(
                "/v1/auth/2fa/verify",
                serde_json::json!({ "recovery_code": recovery }),
            )
            .await;
        assert_eq!(verify.status(), StatusCode::NO_CONTENT);

        let me = app.get_with_session("/v1/auth/me").await;
        assert_eq!(me.status(), StatusCode::OK);

        app.cleanup_user(user.id).await;
    }

    // Test 3: Recovery code single-use
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user_default().await;
        let enabled = app.enable_2fa(&user).await;
        let code = enabled.recovery_codes[1].clone();

        app.login_half_authed(&user).await;
        let first = app
            .post_json_with_session(
                "/v1/auth/2fa/verify",
                serde_json::json!({ "recovery_code": code }),
            )
            .await;
        assert_eq!(first.status(), StatusCode::NO_CONTENT);

        app.reset_session_client();
        app.login_half_authed(&user).await;
        let second = app
            .post_json_with_session(
                "/v1/auth/2fa/verify",
                serde_json::json!({ "recovery_code": code }),
            )
            .await;
        assert_eq!(second.status(), StatusCode::UNAUTHORIZED);

        app.cleanup_user(user.id).await;
    }

    // Test 4: Concurrent recovery consume — exactly one succeeds
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user_default().await;
        let enabled = app.enable_2fa(&user).await;
        let recovery = enabled.recovery_codes[2].clone();
        app.login_half_authed(&user).await;

        let base = app.base_url.clone();
        let client = app.client().clone();
        let body = serde_json::json!({ "recovery_code": recovery });

        let (r1, r2) = tokio::join!(
            client
                .post(format!("{base}/v1/auth/2fa/verify"))
                .json(&body)
                .send(),
            client
                .post(format!("{base}/v1/auth/2fa/verify"))
                .json(&body)
                .send(),
        );
        let s1 = r1.expect("req1").status();
        let s2 = r2.expect("req2").status();
        let successes = [s1, s2]
            .iter()
            .filter(|s| **s == StatusCode::NO_CONTENT)
            .count();
        assert_eq!(successes, 1, "only one concurrent consume may succeed");

        app.cleanup_user(user.id).await;
    }

    // Test 5: Lockout after repeated invalid codes
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user_default().await;
        let _enabled = app.enable_2fa(&user).await;
        app.login_half_authed(&user).await;

        for attempt in 1..=4 {
            let bad = app
                .post_json_with_session(
                    "/v1/auth/2fa/verify",
                    serde_json::json!({ "code": "000000" }),
                )
                .await;
            assert_eq!(
                bad.status(),
                StatusCode::UNAUTHORIZED,
                "attempt {attempt} should be 401"
            );
        }

        let fifth = app
            .post_json_with_session(
                "/v1/auth/2fa/verify",
                serde_json::json!({ "code": "000000" }),
            )
            .await;
        assert_eq!(
            fifth.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "5th failure must return 429 immediately"
        );

        app.cleanup_user(user.id).await;
    }

    // Test 6: Tenant require_2fa blocks DELETE /2fa/totp
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user_default().await;
        let enabled = app.enable_2fa(&user).await;

        app.login_half_authed(&user).await;
        let verify = app
            .post_json_with_session(
                "/v1/auth/2fa/verify",
                serde_json::json!({ "recovery_code": &enabled.recovery_codes[0] }),
            )
            .await;
        assert_eq!(verify.status(), StatusCode::NO_CONTENT);

        let display_id = format!("req2fa-{}", &user.id.to_string()[..8]);
        let create = app
            .post_json_with_session(
                "/v1/tenants",
                serde_json::json!({
                    "display_id": display_id,
                    "name": "Require 2FA Tenant",
                    "description": "",
                    "icon_url": ""
                }),
            )
            .await;
        assert_eq!(create.status(), StatusCode::CREATED);
        let tenant: serde_json::Value = create.json().await.expect("tenant json");
        let tenant_id = tenant["id"].as_str().expect("tenant id");

        let policy = app
            .post_json_with_session(
                &format!("/v1/tenants/{tenant_id}/require-2fa"),
                serde_json::json!({ "enabled": true }),
            )
            .await;
        assert_eq!(policy.status(), StatusCode::OK);

        let delete = app
            .delete_json_with_session(
                "/v1/auth/2fa/totp",
                serde_json::json!({ "recovery_code": &enabled.recovery_codes[1] }),
            )
            .await;
        assert_eq!(delete.status(), StatusCode::FORBIDDEN);

        let user_row = users::Entity::find_by_id(user.id)
            .one(&app.state.db)
            .await
            .expect("load user")
            .expect("user exists");
        assert!(
            user_row.totp_enabled,
            "totp_enabled must remain true after forbidden delete"
        );

        let cred_count = totp_credentials::Entity::find()
            .filter(totp_credentials::Column::UserId.eq(user.id))
            .count(&app.state.db)
            .await
            .expect("count totp credentials");
        assert!(
            cred_count > 0,
            "totp_credentials must not be deleted on forbidden delete"
        );

        app.cleanup_user(user.id).await;
    }

    // Test 7: Suspended user cannot POST /2fa/verify with existing half_authed session
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user_default().await;
        let _enabled = app.enable_2fa(&user).await;
        app.login_half_authed(&user).await;
        let half_authed_client = app.session_client();

        let admin = app.insert_user(true, false).await;
        app.reset_session_client();
        app.login_session_no_content(&admin.email, &admin.password)
            .await;

        let suspend = app
            .patch_json_with_session(
                &format!("/v1/admin/users/{}", user.id),
                serde_json::json!({ "is_suspended": true }),
            )
            .await;
        assert_eq!(suspend.status(), StatusCode::OK);

        let verify = half_authed_client
            .post(format!("{}/v1/auth/2fa/verify", app.base_url()))
            .json(&serde_json::json!({ "code": "000000" }))
            .send()
            .await
            .expect("verify request");
        assert_eq!(verify.status(), StatusCode::FORBIDDEN);
        let body = verify.text().await.expect("verify body");
        assert!(
            body.contains("account-suspended"),
            "expected account-suspended in body, got: {body}"
        );

        app.cleanup_user(user.id).await;
        app.cleanup_user(admin.id).await;
    }

    // Test 8: Concurrent attempts cannot exceed the attempt limit
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user_default().await;
        let enabled = app.enable_2fa(&user).await;
        app.login_half_authed(&user).await;

        // 有効なコードは失敗にならず修正前は回数が増えないので、検証まで進んだ件数が成功数に出る。
        // 成功は回数を消すため、recovery_codes をロックして全件が検証前に揃うまで成功させない
        let lock = app.state.db.begin().await.expect("begin lock");
        lock.execute_unprepared("LOCK TABLE recovery_codes IN ACCESS EXCLUSIVE MODE")
            .await
            .expect("lock recovery_codes");

        // 共有プール（既定 10 本）でロック待ちの要求が接続を持つので 7 件に抑える
        let requests: Vec<_> = enabled.recovery_codes[..7]
            .iter()
            .map(|code| {
                let request = app
                    .client()
                    .post(format!("{}/v1/auth/2fa/verify", app.base_url))
                    .json(&serde_json::json!({ "recovery_code": code }));
                tokio::spawn(async move { request.send().await.expect("verify request").status() })
            })
            .collect();
        let mut settled = false;
        for _ in 0..500 {
            let waiting: i64 = lock
                .query_one_raw(Statement::from_string(
                    DatabaseBackend::Postgres,
                    "SELECT count(*) AS n FROM pg_locks \
                     WHERE NOT granted AND relation = 'recovery_codes'::regclass",
                ))
                .await
                .expect("count lock waiters")
                .expect("count row")
                .try_get("", "n")
                .expect("waiter count");
            let finished = requests.iter().filter(|r| r.is_finished()).count();
            if waiting as usize + finished == requests.len() {
                settled = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(settled, "requests must reach the lock or finish");
        lock.commit().await.expect("release lock");

        let statuses: Vec<StatusCode> = futures::future::join_all(requests)
            .await
            .into_iter()
            .map(|r| r.expect("verify task"))
            .collect();
        let verified = statuses
            .iter()
            .filter(|s| **s == StatusCode::NO_CONTENT)
            .count();
        assert!(
            verified <= 5,
            "only 5 attempts may reach verification, got {statuses:?}"
        );
        assert!(
            statuses
                .iter()
                .all(|s| *s == StatusCode::NO_CONTENT || *s == StatusCode::TOO_MANY_REQUESTS),
            "attempts beyond the limit must be 429, got {statuses:?}"
        );

        app.cleanup_user(user.id).await;
    }

    // Test 9: A valid code within the limit passes and clears the counter
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user_default().await;
        let enabled = app.enable_2fa(&user).await;
        app.login_half_authed(&user).await;

        for attempt in 1..=4 {
            let bad = app
                .post_json_with_session(
                    "/v1/auth/2fa/verify",
                    serde_json::json!({ "code": "000000" }),
                )
                .await;
            assert_eq!(
                bad.status(),
                StatusCode::UNAUTHORIZED,
                "attempt {attempt} should be 401"
            );
        }
        let fifth = app
            .post_json_with_session(
                "/v1/auth/2fa/verify",
                serde_json::json!({ "recovery_code": &enabled.recovery_codes[0] }),
            )
            .await;
        assert_eq!(
            fifth.status(),
            StatusCode::NO_CONTENT,
            "valid 5th attempt must pass"
        );

        app.reset_session_client();
        app.login_half_authed(&user).await;
        for attempt in 1..=4 {
            let bad = app
                .post_json_with_session(
                    "/v1/auth/2fa/verify",
                    serde_json::json!({ "code": "000000" }),
                )
                .await;
            assert_eq!(
                bad.status(),
                StatusCode::UNAUTHORIZED,
                "attempt {attempt} after success should be 401"
            );
        }

        app.cleanup_user(user.id).await;
    }
}
