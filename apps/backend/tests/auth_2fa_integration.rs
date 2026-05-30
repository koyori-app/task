mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn auth_2fa_integration_suite() {
    // Test 1: AuthUser rejects half_authed session on /me
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user().await;
        let _enabled = app.enable_2fa(&user).await;
        app.login_half_authed(&user).await;

        let me = app.get_with_session("/v1/auth/me").await;
        assert_eq!(me.status(), StatusCode::FORBIDDEN);

        app.cleanup_user(user.id).await;
    }

    // Test 2: HalfAuthedUser accepts POST /2fa/verify
    {
        let mut app = TestApp::new().await;
        let user = app.insert_user().await;
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
        let user = app.insert_user().await;
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
        let user = app.insert_user().await;
        let enabled = app.enable_2fa(&user).await;
        let recovery = enabled.recovery_codes[2].clone();
        app.login_half_authed(&user).await;

        let base = app.base_url.clone();
        let client = app.client().clone();
        let body = serde_json::json!({ "recovery_code": recovery });

        let (r1, r2) = tokio::join!(
            client.post(format!("{base}/v1/auth/2fa/verify")).json(&body).send(),
            client.post(format!("{base}/v1/auth/2fa/verify")).json(&body).send(),
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
        let user = app.insert_user().await;
        let _enabled = app.enable_2fa(&user).await;
        app.login_half_authed(&user).await;

        for _ in 0..5 {
            let bad = app
                .post_json_with_session(
                    "/v1/auth/2fa/verify",
                    serde_json::json!({ "code": "000000" }),
                )
                .await;
            assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
        }

        let locked = app
            .post_json_with_session(
                "/v1/auth/2fa/verify",
                serde_json::json!({ "code": "000000" }),
            )
            .await;
        assert_eq!(locked.status(), StatusCode::TOO_MANY_REQUESTS);

        app.cleanup_user(user.id).await;
    }
}

