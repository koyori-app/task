use crate::common::{TestApp, insert_passkey_user};
use axum::http::StatusCode;
use backend::utils::auth::AuthError;
use backend::utils::passkeys::{
    MAX_PASSKEYS_PER_USER, count_user_passkeys, insert_passkey_under_user_lock,
};
use chrono::Utc;
use entity::{oauth_connections, passkeys, users};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use url::Url;
use uuid::Uuid;
use webauthn_authenticator_rs::prelude::WebauthnAuthenticator;
use webauthn_authenticator_rs::softtoken::SoftToken;
use webauthn_rs::prelude::{CreationChallengeResponse, RequestChallengeResponse};

pub(super) fn webauthn_origin(app_url: &str) -> Url {
    let parsed = Url::parse(app_url.trim()).expect("app url");
    let origin = format!(
        "{}://{}{}",
        parsed.scheme(),
        parsed.host_str().expect("host"),
        parsed.port().map(|p| format!(":{p}")).unwrap_or_default()
    );
    Url::parse(&origin).expect("origin url")
}

pub(super) async fn soft_register_finish(
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

    // PR 承認用の GitHub 連携はログイン手段に数えない: それだけ残る削除は 403、
    // ログイン用の連携があれば 204（対照）
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

        let insert_connection = |provider: &str| oauth_connections::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(user.id),
            provider: Set(provider.into()),
            provider_user_id: Set(Uuid::new_v4().to_string()),
            created_at: Set(Utc::now().into()),
            updated_at: Set(Utc::now().into()),
            ..Default::default()
        };
        insert_connection("github_app")
            .insert(&app.state.db)
            .await
            .expect("insert github_app connection");

        let passkey = passkeys::Entity::find()
            .filter(passkeys::Column::UserId.eq(user.id))
            .one(&app.state.db)
            .await
            .expect("find")
            .expect("passkey");
        let path = format!("/v1/auth/passkeys/{}", passkey.id);

        let blocked = app.delete_with_session(&path).await;
        assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
        let body = blocked.text().await.expect("body");
        assert!(body.contains("last-auth-method"), "{body}");

        insert_connection("github")
            .insert(&app.state.db)
            .await
            .expect("insert github connection");
        let deleted = app.delete_with_session(&path).await;
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

        app.cleanup_user(user.id).await;
    }

    // 正常な登録フロー（start → finish）→ 201 は soft_register_finish が見ている

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

// ---- clientDataJSON の crossOrigin・topOrigin と、credential ID の長さを、handler の経路で縛る ----
//
// 単体試験（service/src/passkeys.rs）は判定の関数だけを縛る。ここでは、登録と認証の finish が
// 実際にその関数を呼び、400 を返す並びになっていることを縛る（配線が外れても気づけるように）。

fn b64url_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = match chunk.len() {
            3 => u32::from(chunk[0]) << 16 | u32::from(chunk[1]) << 8 | u32::from(chunk[2]),
            2 => u32::from(chunk[0]) << 16 | u32::from(chunk[1]) << 8,
            _ => u32::from(chunk[0]) << 16,
        };
        for i in 0..=chunk.len() {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn b64url_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'-' => 62,
            b'_' => 63,
            _ => panic!("not base64url: {c}"),
        }
    };
    let mut out = Vec::new();
    for chunk in text.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= value(*c) << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    out
}

/// 応答の `clientDataJSON` に項目を足す（署名は壊れるが、検めの順序を見るのが目的）。
fn with_client_data_field(
    credential: &serde_json::Value,
    key: &str,
    value: serde_json::Value,
) -> serde_json::Value {
    let mut tampered = credential.clone();
    let encoded = tampered["response"]["clientDataJSON"]
        .as_str()
        .expect("clientDataJSON")
        .to_string();
    let mut client_data: serde_json::Value =
        serde_json::from_slice(&b64url_decode(&encoded)).expect("clientData json");
    client_data[key] = value;
    tampered["response"]["clientDataJSON"] = serde_json::Value::String(b64url_encode(
        &serde_json::to_vec(&client_data).expect("clientData bytes"),
    ));
    tampered
}

/// 認証器を使わずに、attestation `none` の登録応答を組む。署名が要らぬので、credential ID の長さや
/// clientData の項目を自由に変えられる。公開鍵は P-256 の生成元（曲線上の有効な点）。
fn crafted_registration(
    creation_options: &serde_json::Value,
    rp_id: &str,
    origin: &str,
    credential_id: &[u8],
    client_data_extra: Option<(&str, serde_json::Value)>,
) -> serde_json::Value {
    use sha2::{Digest, Sha256};

    let x =
        hex::decode("6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296").expect("x");
    let y =
        hex::decode("4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5").expect("y");
    let mut cose_key = vec![0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20];
    cose_key.extend_from_slice(&x);
    cose_key.extend_from_slice(&[0x22, 0x58, 0x20]);
    cose_key.extend_from_slice(&y);

    let mut auth_data = Sha256::digest(rp_id.as_bytes()).to_vec();
    auth_data.push(0x45); // UP | UV | AT
    auth_data.extend_from_slice(&[0, 0, 0, 0]); // counter
    auth_data.extend_from_slice(&[0u8; 16]); // aaguid
    auth_data.extend_from_slice(&(credential_id.len() as u16).to_be_bytes());
    auth_data.extend_from_slice(credential_id);
    auth_data.extend_from_slice(&cose_key);

    // {"fmt": "none", "attStmt": {}, "authData": h'...'}
    let mut attestation = vec![0xa3, 0x63];
    attestation.extend_from_slice(b"fmt");
    attestation.push(0x64);
    attestation.extend_from_slice(b"none");
    attestation.push(0x67);
    attestation.extend_from_slice(b"attStmt");
    attestation.push(0xa0);
    attestation.push(0x68);
    attestation.extend_from_slice(b"authData");
    attestation.push(0x59);
    attestation.extend_from_slice(&(auth_data.len() as u16).to_be_bytes());
    attestation.extend_from_slice(&auth_data);

    let mut client_data = serde_json::json!({
        "type": "webauthn.create",
        "challenge": creation_options["publicKey"]["challenge"],
        "origin": origin,
        "crossOrigin": false,
    });
    if let Some((key, value)) = client_data_extra {
        client_data[key] = value;
    }
    serde_json::json!({
        "id": b64url_encode(credential_id),
        "rawId": b64url_encode(credential_id),
        "type": "public-key",
        "extensions": {},
        "response": {
            "attestationObject": b64url_encode(&attestation),
            "clientDataJSON": b64url_encode(&serde_json::to_vec(&client_data).expect("bytes")),
        },
    })
}

/// 登録の start → 組んだ応答の finish。応答の状態と本文を返す。
async fn crafted_register(
    app: &TestApp,
    credential_id: &[u8],
    client_data_extra: Option<(&str, serde_json::Value)>,
) -> (StatusCode, String) {
    let start = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/start",
            serde_json::json!({}),
        )
        .await;
    assert_eq!(start.status(), StatusCode::OK, "registration start");
    let options: serde_json::Value = start.json().await.expect("start json");
    let rp_id = options["publicKey"]["rp"]["id"].as_str().expect("rp id");
    let origin = webauthn_origin(&app.state.settings.email_verification_app_url).to_string();
    let origin = origin.trim_end_matches('/');
    let credential =
        crafted_registration(&options, rp_id, origin, credential_id, client_data_extra);
    let finish = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/finish",
            serde_json::json!({ "name": "crafted", "credential": credential }),
        )
        .await;
    let status = finish.status();
    (status, finish.text().await.unwrap_or_default())
}

#[tokio::test]
async fn registration_finish_rejects_cross_origin_marks_and_long_credential_ids() {
    let mut app = TestApp::new().await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let count = || async {
        count_user_passkeys(&app.state.db, user.id)
            .await
            .expect("count passkeys")
    };

    // 対照: 組んだ応答の形が正しければ通る（以下の 400 が、形の誤りでないことの裏づけ）
    let (status, body) = crafted_register(&app, &[7u8; 32], None).await;
    assert_eq!(status, StatusCode::CREATED, "control: {body}");
    // 仕様の上限ちょうど（1023 byte）は通る
    let (status, body) = crafted_register(&app, &[8u8; 1023], None).await;
    assert_eq!(status, StatusCode::CREATED, "1023 bytes: {body}");
    assert_eq!(count().await, 2);

    // 1024 byte は 400 で、保存されぬ
    let (status, body) = crafted_register(&app, &[9u8; 1024], None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "1024 bytes: {body}");
    assert_eq!(count().await, 2, "an oversized credential ID is not stored");

    // topOrigin（別の frame から呼ばれた印）は、webauthn-rs が登録では見ぬので、handler が断る
    let (status, body) = crafted_register(
        &app,
        &[10u8; 32],
        Some(("topOrigin", serde_json::json!("http://evil.example"))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "topOrigin: {body}");
    assert!(body.contains("webauthn-error"), "{body}");
    assert_eq!(count().await, 2);

    // crossOrigin: true も断る
    let (status, body) = crafted_register(
        &app,
        &[11u8; 32],
        Some(("crossOrigin", serde_json::json!(true))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "crossOrigin: {body}");
    assert_eq!(count().await, 2);

    // 断った後も、正しい形はまた通る（ロックを放し、状態を壊しておらぬ）
    let (status, body) = crafted_register(&app, &[12u8; 32], None).await;
    assert_eq!(status, StatusCode::CREATED, "after the rejections: {body}");
    assert_eq!(count().await, 3);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn authentication_finish_rejects_cross_origin_marks_before_consuming_the_challenge() {
    let mut app = TestApp::new().await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let (soft, _) = SoftToken::new(true).expect("softtoken");
    let mut wa = WebauthnAuthenticator::new(soft);
    soft_register_finish(&app, &mut wa, "key").await;
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
    let genuine = serde_json::to_value(
        wa.do_authentication(origin, rcr)
            .expect("softtoken authentication"),
    )
    .expect("credential json");

    // crossOrigin: true と topOrigin を足した応答は 400。署名も壊れるが、handler が先に断るので、
    // challenge の状態は消えぬ（webauthn-rs まで進んでから失敗すると、状態が先に消える）
    for (title, tampered) in [
        (
            "crossOrigin",
            with_client_data_field(&genuine, "crossOrigin", serde_json::json!(true)),
        ),
        (
            "topOrigin",
            with_client_data_field(
                &genuine,
                "topOrigin",
                serde_json::json!("http://evil.example"),
            ),
        ),
    ] {
        let res = app
            .post_json_with_session(
                "/v1/auth/passkeys/authentication/finish",
                serde_json::json!({ "challenge_id": challenge_id, "credential": tampered }),
            )
            .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{title}");
        let body = res.text().await.unwrap_or_default();
        assert!(body.contains("webauthn-error"), "{title}: {body}");
    }

    // 同じ challenge_id で、正しい応答が通る（状態が残っておった）。session もここで張られる
    let finish = app
        .post_json_with_session(
            "/v1/auth/passkeys/authentication/finish",
            serde_json::json!({ "challenge_id": challenge_id, "credential": genuine }),
        )
        .await;
    assert_eq!(
        finish.status(),
        StatusCode::NO_CONTENT,
        "genuine assertion after the rejected ones: {}",
        finish.text().await.unwrap_or_default()
    );

    app.cleanup_user(user.id).await;
}
