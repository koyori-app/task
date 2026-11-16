// パスキーの追加と削除は、直近の本人確認（再認証）を求める。
//
// 有効期間は `reauth_window_secs`。試験では 5 秒にして、6 秒余り待って期限を切らす（1 秒では、負荷の下で再認証の直後の操作が窓を越える）。
// 印を立てる手段（パスワード・パスキー・OAuth の連携・2FA の検証）と、立てぬ手段（連携の追加）を縛る。

use crate::common::{MockGitLabUser, TestApp, is_redirect};
use crate::webauthn_integration::{soft_register_finish, webauthn_origin};
use axum::http::StatusCode;
use entity::users;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use std::time::Duration;
use webauthn_authenticator_rs::prelude::WebauthnAuthenticator;
use webauthn_authenticator_rs::softtoken::SoftToken;
use webauthn_rs::prelude::RequestChallengeResponse;

const WINDOW_SECS: u64 = 5;

/// 有効期間が確かに過ぎるまで待つ。
async fn let_the_window_pass() {
    tokio::time::sleep(Duration::from_millis(WINDOW_SECS * 1000 + 1500)).await;
}

/// OAuth で作った利用者はメール未確認なので、パスキーの登録（要メール確認）を試せるよう確認済みにする。
async fn verify_email(app: &TestApp, user_id: uuid::Uuid) {
    let model = users::Entity::find_by_id(user_id)
        .one(&app.state.db)
        .await
        .expect("load user")
        .expect("user");
    let mut active: users::ActiveModel = model.into();
    active.email_verified = Set(true);
    active.update(&app.state.db).await.expect("verify email");
}

async fn body_of(res: reqwest::Response) -> String {
    res.text().await.unwrap_or_default()
}

async fn passkey_ids(app: &TestApp) -> Vec<String> {
    let res = app.get_with_session("/v1/auth/passkeys").await;
    assert_eq!(res.status(), StatusCode::OK);
    let json: serde_json::Value = res.json().await.expect("list json");
    json["passkeys"]
        .as_array()
        .expect("passkeys array")
        .iter()
        .map(|p| p["id"].as_str().expect("id").to_string())
        .collect()
}

async fn assert_reauth_required(res: reqwest::Response, what: &str) {
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "{what}");
    let body = body_of(res).await;
    assert!(body.contains("reauth-required"), "{what}: {body}");
}

#[tokio::test]
async fn password_login_satisfies_reauth_and_it_expires() {
    let mut app = TestApp::new_with_reauth_window(WINDOW_SECS).await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    app.login_session_no_content(&user.email, &user.password)
        .await;

    // ログイン直後は、追加（start と finish）も削除も通る
    let (soft, _) = SoftToken::new(true).expect("softtoken");
    let mut wa = WebauthnAuthenticator::new(soft);
    soft_register_finish(&app, &mut wa, "first").await;
    soft_register_finish(&app, &mut wa, "second").await;
    let ids = passkey_ids(&app).await;
    assert_eq!(ids.len(), 2);

    let_the_window_pass().await;

    // 期限が切れたら、cookie だけでは追加も削除もできない
    let start = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/start",
            serde_json::json!({}),
        )
        .await;
    assert_reauth_required(start, "registration start after the window").await;
    let delete = app
        .delete_with_session(&format!("/v1/auth/passkeys/{}", ids[0]))
        .await;
    assert_reauth_required(delete, "delete after the window").await;
    assert_eq!(passkey_ids(&app).await.len(), 2, "nothing was deleted");
    // 一覧と名前の変更は対象外
    let rename = app
        .patch_json_with_session(
            &format!("/v1/auth/passkeys/{}", ids[0]),
            serde_json::json!({ "name": "renamed" }),
        )
        .await;
    assert_eq!(rename.status(), StatusCode::NO_CONTENT);

    // 誤ったパスワードでは印が立たない
    let wrong = app
        .post_json_with_session(
            "/v1/auth/reauth/password",
            serde_json::json!({ "password": "WrongPassword123!" }),
        )
        .await;
    assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);
    assert!(body_of(wrong).await.contains("invalid-current-password"));
    let still = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/start",
            serde_json::json!({}),
        )
        .await;
    assert_reauth_required(still, "registration start after a failed reauth").await;

    // 正しいパスワードで印が立ち、追加も削除も通る
    let ok = app
        .post_json_with_session(
            "/v1/auth/reauth/password",
            serde_json::json!({ "password": user.password }),
        )
        .await;
    assert_eq!(ok.status(), StatusCode::NO_CONTENT);
    soft_register_finish(&app, &mut wa, "third").await;
    let delete = app
        .delete_with_session(&format!("/v1/auth/passkeys/{}", ids[0]))
        .await;
    assert_eq!(delete.status(), StatusCode::NO_CONTENT);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn registration_finish_after_the_window_is_refused_and_frees_the_lock() {
    let mut app = TestApp::new_with_reauth_window(WINDOW_SECS).await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    app.login_session_no_content(&user.email, &user.password)
        .await;

    let start = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/start",
            serde_json::json!({}),
        )
        .await;
    assert_eq!(start.status(), StatusCode::OK);

    let_the_window_pass().await;

    let finish = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/finish",
            serde_json::json!({ "name": "late", "credential": {} }),
        )
        .await;
    assert_reauth_required(finish, "registration finish after the window").await;

    // ロックは放してあるので、再認証すれば、すぐ新しく始められる（409 にならぬ）
    let ok = app
        .post_json_with_session(
            "/v1/auth/reauth/password",
            serde_json::json!({ "password": user.password }),
        )
        .await;
    assert_eq!(ok.status(), StatusCode::NO_CONTENT);
    let again = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/start",
            serde_json::json!({}),
        )
        .await;
    assert_eq!(again.status(), StatusCode::OK);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn passkey_reauth_satisfies_the_requirement() {
    let mut app = TestApp::new_with_reauth_window(WINDOW_SECS).await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    app.login_session_no_content(&user.email, &user.password)
        .await;

    // パスキーが無いうちは、パスキーでの再認証は始められない
    let none = app
        .post_json_with_session("/v1/auth/reauth/passkey/start", serde_json::json!({}))
        .await;
    assert_eq!(none.status(), StatusCode::NOT_FOUND);

    let (soft, _) = SoftToken::new(true).expect("softtoken");
    let mut wa = WebauthnAuthenticator::new(soft);
    soft_register_finish(&app, &mut wa, "key").await;

    let_the_window_pass().await;
    let start = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/start",
            serde_json::json!({}),
        )
        .await;
    assert_reauth_required(start, "registration start before the passkey reauth").await;

    // start を呼ばずに finish は出せない
    let orphan = app
        .post_json_with_session(
            "/v1/auth/reauth/passkey/finish",
            serde_json::json!({ "credential": {} }),
        )
        .await;
    assert_eq!(orphan.status(), StatusCode::BAD_REQUEST);

    let start = app
        .post_json_with_session("/v1/auth/reauth/passkey/start", serde_json::json!({}))
        .await;
    assert_eq!(start.status(), StatusCode::OK);
    let rcr: RequestChallengeResponse = start.json().await.expect("parse rcr");
    let origin = webauthn_origin(&app.state.settings.email_verification_app_url);
    let cred = wa
        .do_authentication(origin, rcr)
        .expect("softtoken authentication");
    let finish = app
        .post_json_with_session(
            "/v1/auth/reauth/passkey/finish",
            serde_json::json!({ "credential": cred }),
        )
        .await;
    assert_eq!(
        finish.status(),
        StatusCode::NO_CONTENT,
        "{}",
        body_of(finish).await
    );
    soft_register_finish(&app, &mut wa, "second").await;

    // 使った challenge は使い捨て。同じ応答をもう一度出しても通らぬ
    let replay = app
        .post_json_with_session(
            "/v1/auth/reauth/passkey/finish",
            serde_json::json!({ "credential": cred }),
        )
        .await;
    assert_eq!(replay.status(), StatusCode::BAD_REQUEST);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn totp_verification_at_login_satisfies_reauth() {
    let mut app = TestApp::new_with_reauth_window(WINDOW_SECS).await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    let enabled = app.enable_2fa(&user).await;

    app.login_half_authed(&user).await;
    let_the_window_pass().await;
    // パスワードの確認から時間が経ってから、リカバリーコードで 2FA を通す。この検証が印を立てる
    let verify = app
        .post_json_with_session(
            "/v1/auth/2fa/verify",
            serde_json::json!({ "recovery_code": enabled.recovery_codes[0] }),
        )
        .await;
    assert_eq!(verify.status(), StatusCode::NO_CONTENT);
    let (soft, _) = SoftToken::new(true).expect("softtoken");
    let mut wa = WebauthnAuthenticator::new(soft);
    soft_register_finish(&app, &mut wa, "after-2fa").await;

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn oauth_login_satisfies_reauth_but_linking_does_not() {
    let mut app = TestApp::new_with_reauth_window(WINDOW_SECS).await;
    let unique = uuid::Uuid::new_v4();
    app.set_mock_user(MockGitLabUser {
        id: 410_001,
        username: format!("reauth_{unique}"),
        email: Some(format!("reauth-{unique}@example.com")),
    });

    // OAuth のログインは、印を立てる
    app.reset_session_client();
    let start = app.oauth_start(false).await;
    let callback = app.follow_oauth_start(start).await;
    assert!(is_redirect(callback.status()), "oauth login redirect");
    let me: serde_json::Value = app.get_me().await.json().await.expect("me json");
    let user_id: uuid::Uuid = me["id"].as_str().expect("id").parse().expect("uuid");
    assert_eq!(me["has_password"], serde_json::json!(false));
    verify_email(&app, user_id).await;
    let (soft, _) = SoftToken::new(true).expect("softtoken");
    let mut wa = WebauthnAuthenticator::new(soft);
    soft_register_finish(&app, &mut wa, "first").await;

    let_the_window_pass().await;
    let start = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/start",
            serde_json::json!({}),
        )
        .await;
    assert_reauth_required(start, "registration start after the window").await;

    // パスワードを持たぬ利用者は、パスワードでは再認証できぬ
    let by_password = app
        .post_json_with_session(
            "/v1/auth/reauth/password",
            serde_json::json!({ "password": "Whatever123!" }),
        )
        .await;
    assert_eq!(by_password.status(), StatusCode::BAD_REQUEST);
    assert!(body_of(by_password).await.contains("password-not-set"));

    // 連携の追加（link）は印を立てぬ。盗まれたセッションで自分の連携を足しても、再認証にならぬ
    let link = app.oauth_start(true).await;
    let callback = app.follow_oauth_start(link).await;
    assert!(is_redirect(callback.status()), "oauth link redirect");
    let start = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/start",
            serde_json::json!({}),
        )
        .await;
    assert_reauth_required(start, "registration start after a link flow").await;

    // 既に結んだ連携での再認証は、印を立てる
    let reauth = app.oauth_start_reauth().await;
    let callback = app.follow_oauth_start(reauth).await;
    assert!(is_redirect(callback.status()), "oauth reauth redirect");
    soft_register_finish(&app, &mut wa, "second").await;

    app.cleanup_user(user_id).await;
}

#[tokio::test]
async fn oauth_reauth_needs_a_session_and_the_users_own_connection() {
    let mut app = TestApp::new_with_reauth_window(WINDOW_SECS).await;
    let unique = uuid::Uuid::new_v4();

    // 先に、別の利用者 B を作る。B は自分の連携を持つ（再認証に他人の連携を使わせぬことを見るため）
    let other = MockGitLabUser {
        id: 410_003,
        username: format!("other_{unique}"),
        email: Some(format!("other-{unique}@example.com")),
    };
    app.set_mock_user(other.clone());
    app.reset_session_client();
    let start = app.oauth_start(false).await;
    let callback = app.follow_oauth_start(start).await;
    assert!(
        is_redirect(callback.status()),
        "oauth login redirect (other)"
    );
    let me: serde_json::Value = app.get_me().await.json().await.expect("me json");
    let other_id: uuid::Uuid = me["id"].as_str().expect("id").parse().expect("uuid");

    // 利用者 A としてログインする
    app.set_mock_user(MockGitLabUser {
        id: 410_002,
        username: format!("owner_{unique}"),
        email: Some(format!("owner-{unique}@example.com")),
    });
    app.reset_session_client();
    let start = app.oauth_start(false).await;
    let callback = app.follow_oauth_start(start).await;
    assert!(
        is_redirect(callback.status()),
        "oauth login redirect (owner)"
    );
    let me: serde_json::Value = app.get_me().await.json().await.expect("me json");
    let user_id: uuid::Uuid = me["id"].as_str().expect("id").parse().expect("uuid");
    assert_ne!(user_id, other_id);
    verify_email(&app, user_id).await;

    let_the_window_pass().await;

    // A の session で、B の連携で再認証しようとしても、印は立たず、連携も増えぬ
    app.set_mock_user(other);
    let reauth = app.oauth_start_reauth().await;
    let callback = app.follow_oauth_start(reauth).await;
    assert_eq!(callback.status(), StatusCode::FORBIDDEN);
    assert!(body_of(callback).await.contains("oauth-reauth-mismatch"));
    assert_eq!(app.count_connections_for_user(user_id).await, 1);
    assert_eq!(app.count_connections_for_user(other_id).await, 1);
    let start = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/start",
            serde_json::json!({}),
        )
        .await;
    assert_reauth_required(start, "registration start after a wrong-identity reauth").await;

    // どの利用者でもない提供元の利用者でも、同じく断る（連携も利用者も作らぬ）
    app.set_mock_user(MockGitLabUser {
        id: 410_004,
        username: format!("stranger_{unique}"),
        email: Some(format!("stranger-{unique}@example.com")),
    });
    let reauth = app.oauth_start_reauth().await;
    let callback = app.follow_oauth_start(reauth).await;
    assert_eq!(callback.status(), StatusCode::FORBIDDEN);
    assert_eq!(app.count_connections_for_user(user_id).await, 1);

    // セッションが無ければ、再認証は始められぬ
    app.reset_session_client();
    let anonymous = app.oauth_start_reauth().await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    app.cleanup_user(user_id).await;
    app.cleanup_user(other_id).await;
}

#[tokio::test]
async fn passkey_login_satisfies_reauth() {
    let mut app = TestApp::new_with_reauth_window(WINDOW_SECS).await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let (soft, _) = SoftToken::new(true).expect("softtoken");
    let mut wa = WebauthnAuthenticator::new(soft);
    soft_register_finish(&app, &mut wa, "key").await;

    // パスワードのログインの印が切れてから、パスキーだけでログインし直す
    let_the_window_pass().await;
    app.reset_session_client();
    let start = app
        .post_json_with_session(
            "/v1/auth/passkeys/authentication/start",
            serde_json::json!({ "email": user.email }),
        )
        .await;
    assert_eq!(start.status(), StatusCode::OK);
    let json: serde_json::Value = start.json().await.expect("auth json");
    let challenge_id = json["challenge_id"]
        .as_str()
        .expect("challenge_id")
        .to_string();
    let mut options = json.clone();
    options
        .as_object_mut()
        .expect("object")
        .remove("challenge_id");
    let rcr: RequestChallengeResponse = serde_json::from_value(options).expect("parse rcr");
    let origin = webauthn_origin(&app.state.settings.email_verification_app_url);
    let cred = wa
        .do_authentication(origin, rcr)
        .expect("softtoken authentication");
    let finish = app
        .post_json_with_session(
            "/v1/auth/passkeys/authentication/finish",
            serde_json::json!({ "challenge_id": challenge_id, "credential": cred }),
        )
        .await;
    assert_eq!(finish.status(), StatusCode::NO_CONTENT);

    // このログインが印を立てるので、すぐ追加できる
    soft_register_finish(&app, &mut wa, "second").await;

    app.cleanup_user(user.id).await;
}
