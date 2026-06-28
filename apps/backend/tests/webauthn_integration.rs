mod common;

use axum::http::StatusCode;
use backend::entities::{passkeys, users};
use backend::utils::auth::AuthError;
use backend::utils::passkeys::{
    MAX_PASSKEYS_PER_USER, count_user_passkeys, insert_passkey_under_user_lock,
};
use chrono::Utc;
use common::{TestApp, insert_passkey_user};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use url::Url;
use uuid::Uuid;
use webauthn_authenticator_rs::prelude::WebauthnAuthenticator;
use webauthn_authenticator_rs::softtoken::SoftToken;
use webauthn_rs::prelude::{CreationChallengeResponse, RequestChallengeResponse};

fn webauthn_origin(app_url: &str) -> Url {
    let parsed = Url::parse(app_url.trim()).expect("app url");
    let origin = format!(
        "{}://{}{}",
        parsed.scheme(),
        parsed.host_str().expect("host"),
        parsed.port().map(|p| format!(":{p}")).unwrap_or_default()
    );
    Url::parse(&origin).expect("origin url")
}

async fn soft_register_finish(
    app: &TestApp,
    wa: &mut WebauthnAuthenticator<SoftToken>,
    name: &str,
) {
    let start = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/start",
            serde_json::json!({}),
        )
        .await;
    assert_eq!(start.status(), StatusCode::OK, "registration start");
    let start_body = start.text().await.expect("start body");
    let ccr: CreationChallengeResponse = serde_json::from_str(&start_body).unwrap_or_else(|e| {
        panic!("parse ccr failed: {e}\nbody: {start_body}");
    });
    assert!(
        !ccr.public_key.pub_key_cred_params.is_empty(),
        "pub_key_cred_params empty in challenge"
    );

    let origin = webauthn_origin(&app.state.settings.email_verification_app_url);
    let reg_cred = wa
        .do_registration(origin, ccr)
        .expect("softtoken registration");

    let finish = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/finish",
            serde_json::json!({
                "name": name,
                "credential": reg_cred,
            }),
        )
        .await;
    assert_eq!(
        finish.status(),
        StatusCode::CREATED,
        "registration finish: {}",
        finish.text().await.unwrap_or_default()
    );
}

fn dummy_passkey_model(user_id: Uuid, index: u8) -> passkeys::ActiveModel {
    let now = Utc::now().into();
    passkeys::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        credential_id: Set(vec![index; 32]),
        public_key: Set(vec![1, 2, 3, index]),
        aaguid: Set(None),
        sign_count: Set(0),
        name: Set(format!("dummy-{index}")),
        last_used_at: Set(None),
        created_at: Set(now),
    }
}

#[tokio::test]
async fn webauthn_integration_suite() {
    // 登録ロック中の重複 start → 409
    {
        let mut app = TestApp::new().await;
        let user = app
            .insert_passkey_user(true, Some("TestPassword123!"))
            .await;
        app.login_session(&user.email, &user.password).await;

        let first = app
            .post_json_with_session(
                "/v1/auth/passkeys/registration/start",
                serde_json::json!({}),
            )
            .await;
        assert_eq!(first.status(), StatusCode::OK);

        let second = app
            .post_json_with_session(
                "/v1/auth/passkeys/registration/start",
                serde_json::json!({}),
            )
            .await;
        assert_eq!(second.status(), StatusCode::CONFLICT);
        let body = second.text().await.expect("body");
        assert!(body.contains("passkey-registration-in-progress"));

        app.cleanup_user(user.id).await;
    }

    // LastAuthMethod: 最後の認証手段削除 → 403
    {
        let mut app = TestApp::new().await;
        let user = app
            .insert_passkey_user(true, Some("TestPassword123!"))
            .await;
        app.login_session(&user.email, &user.password).await;
        insert_passkey_under_user_lock(&app.state.db, user.id, dummy_passkey_model(user.id, 1))
            .await
            .expect("seed passkey");

        let model = users::Entity::find_by_id(user.id)
            .one(&app.state.db)
            .await
            .expect("load")
            .expect("user");
        let mut active: users::ActiveModel = model.into();
        active.password_hash = Set(None);
        active.update(&app.state.db).await.expect("clear password");

        let passkey = passkeys::Entity::find()
            .filter(passkeys::Column::UserId.eq(user.id))
            .one(&app.state.db)
            .await
            .expect("find")
            .expect("passkey");

        let delete = app
            .delete_with_session(&format!("/v1/auth/passkeys/{}", passkey.id))
            .await;
        assert_eq!(delete.status(), StatusCode::FORBIDDEN);
        let body = delete.text().await.expect("body");
        assert!(body.contains("last-auth-method"));

        app.cleanup_user(user.id).await;
    }

    // 正常な登録フロー（start → finish）→ 201
    {
        let mut app = TestApp::new().await;
        let user = app
            .insert_passkey_user(true, Some("TestPassword123!"))
            .await;
        app.login_session(&user.email, &user.password).await;

        let (soft, _) = SoftToken::new(true).expect("softtoken");
        let mut wa = WebauthnAuthenticator::new(soft);
        soft_register_finish(&app, &mut wa, "integration-key").await;

        app.cleanup_user(user.id).await;
    }

    // 正常な認証フロー（start → finish）→ 204
    {
        let mut app = TestApp::new().await;
        let user = app
            .insert_passkey_user(true, Some("TestPassword123!"))
            .await;
        app.login_session(&user.email, &user.password).await;

        let (soft, _) = SoftToken::new(true).expect("softtoken");
        let mut wa = WebauthnAuthenticator::new(soft);
        soft_register_finish(&app, &mut wa, "login-key").await;

        app.reset_session_client();

        let auth_start = app
            .post_json_with_session(
                "/v1/auth/passkeys/authentication/start",
                serde_json::json!({ "email": user.email }),
            )
            .await;
        assert_eq!(auth_start.status(), StatusCode::OK);
        let auth_json: serde_json::Value = auth_start.json().await.expect("auth json");
        let challenge_id = auth_json["challenge_id"]
            .as_str()
            .expect("challenge_id")
            .to_string();
        let mut options = auth_json.clone();
        options
            .as_object_mut()
            .expect("object")
            .remove("challenge_id");
        let rcr: RequestChallengeResponse = serde_json::from_value(options).expect("parse rcr");

        let origin = webauthn_origin(&app.state.settings.email_verification_app_url);
        let auth_cred = wa
            .do_authentication(origin, rcr)
            .expect("softtoken authentication");

        let finish = app
            .post_json_with_session(
                "/v1/auth/passkeys/authentication/finish",
                serde_json::json!({
                    "challenge_id": challenge_id,
                    "credential": auth_cred,
                }),
            )
            .await;
        assert_eq!(
            finish.status(),
            StatusCode::NO_CONTENT,
            "auth finish: {}",
            finish.text().await.unwrap_or_default()
        );

        app.cleanup_user(user.id).await;
    }

    // 存在しない challenge_id → 400
    {
        let mut app = TestApp::new().await;
        let user = app
            .insert_passkey_user(true, Some("TestPassword123!"))
            .await;
        app.login_session(&user.email, &user.password).await;

        let (soft, _) = SoftToken::new(true).expect("softtoken");
        let mut wa = WebauthnAuthenticator::new(soft);
        soft_register_finish(&app, &mut wa, "invalid-challenge-key").await;

        app.reset_session_client();

        let auth_start = app
            .post_json_with_session(
                "/v1/auth/passkeys/authentication/start",
                serde_json::json!({ "email": user.email }),
            )
            .await;
        assert_eq!(auth_start.status(), StatusCode::OK);
        let auth_json: serde_json::Value = auth_start.json().await.expect("auth json");
        let mut options = auth_json.clone();
        options
            .as_object_mut()
            .expect("object")
            .remove("challenge_id");
        let rcr: RequestChallengeResponse = serde_json::from_value(options).expect("parse rcr");

        let origin = webauthn_origin(&app.state.settings.email_verification_app_url);
        let auth_cred = wa
            .do_authentication(origin, rcr)
            .expect("softtoken authentication");

        let bogus_challenge_id = Uuid::new_v4();

        let finish = app
            .post_json_with_session(
                "/v1/auth/passkeys/authentication/finish",
                serde_json::json!({
                    "challenge_id": bogus_challenge_id,
                    "credential": auth_cred,
                }),
            )
            .await;
        assert_eq!(
            finish.status(),
            StatusCode::BAD_REQUEST,
            "invalid challenge_id: {}",
            finish.text().await.unwrap_or_default()
        );

        app.cleanup_user(user.id).await;
    }

    // 登録上限: 並行 INSERT で 21 件目を防ぐ
    {
        let app = TestApp::new().await;
        let user = insert_passkey_user(&app.state.db, true, Some("TestPassword123!")).await;
        for i in 0..19u8 {
            insert_passkey_under_user_lock(&app.state.db, user.id, dummy_passkey_model(user.id, i))
                .await
                .expect("seed passkey");
        }
        assert_eq!(
            count_user_passkeys(&app.state.db, user.id)
                .await
                .expect("count"),
            19
        );

        let db = app.state.db.clone();
        let uid = user.id;
        let (r1, r2) = tokio::join!(
            insert_passkey_under_user_lock(&db, uid, dummy_passkey_model(uid, 100)),
            insert_passkey_under_user_lock(&db, uid, dummy_passkey_model(uid, 101)),
        );

        let results = [r1, r2];
        let ok_count = results.iter().filter(|r| r.is_ok()).count();
        let limit_count = results
            .iter()
            .filter(|r| matches!(r, Err(AuthError::PasskeyLimitExceeded)))
            .count();
        assert_eq!(ok_count, 1, "exactly one insert should succeed");
        assert_eq!(limit_count, 1, "second insert should hit limit");

        let final_count = count_user_passkeys(&app.state.db, user.id)
            .await
            .expect("final count");
        assert_eq!(final_count, MAX_PASSKEYS_PER_USER);

        app.cleanup_user(user.id).await;
    }
}
