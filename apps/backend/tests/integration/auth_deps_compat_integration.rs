//! 認証まわりの依存（webauthn-rs・axum_session）を上げても、既に在る物がそのまま使えることを縛る。
//!
//! 試料は上げる前の版（webauthn-rs 0.5.5・axum_session 0.20.1）で作って `tests/fixtures/auth_deps/`
//! に固定した。DB の passkeys 行（`Passkey` の直列化）、Redis の登録と認証の途中の状態、ブラウザが
//! 返す応答、ログイン中のセッション（Redis の値と cookie）に当たる。上げた後の版が読めなくなったり、
//! 書く形が変わったりすれば、ここが落ちる。argon2 の PHC 文字列の試料は service の単体試験にある。
//!
//! 試料の日付や id は試験の中で据え直す（期限切れや主キーの衝突で落ちないように）。署名の対象でない
//! `userHandle` と、利用者の id だけを差し替え、他の中身には触れない。

use crate::common::TestApp;
use axum::http::StatusCode;
use chrono::Utc;
use entity::passkeys;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use uuid::Uuid;

fn fixture(name: &str) -> Value {
    let text = match name {
        "registration" => {
            include_str!("../fixtures/auth_deps/passkey_registration_webauthn_rs_0_5_5.json")
        }
        "authentication" => {
            include_str!("../fixtures/auth_deps/passkey_authentication_webauthn_rs_0_5_5.json")
        }
        "discoverable" => {
            include_str!("../fixtures/auth_deps/passkey_discoverable_webauthn_rs_0_5_5.json")
        }
        "reauth" => include_str!("../fixtures/auth_deps/passkey_reauth_webauthn_rs_0_5_5.json"),
        "session" => include_str!("../fixtures/auth_deps/session_axum_session_0_20_1.json"),
        other => panic!("no fixture {other}"),
    };
    serde_json::from_str(text).expect("fixture json")
}

async fn redis_set(app: &TestApp, key: &str, value: &str, ex_secs: u64) {
    let mut conn = app
        .state
        .redis_client
        .conn
        .acquire()
        .await
        .expect("redis acquire");
    let _: () = redis::cmd("SET")
        .arg(key)
        .arg(value)
        .arg("EX")
        .arg(ex_secs)
        .query_async(&mut conn)
        .await
        .expect("redis SET");
}

async fn redis_get(app: &TestApp, key: &str) -> Option<String> {
    let mut conn = app
        .state
        .redis_client
        .conn
        .acquire()
        .await
        .expect("redis acquire");
    redis::cmd("GET")
        .arg(key)
        .query_async(&mut conn)
        .await
        .expect("redis GET")
}

async fn redis_ttl(app: &TestApp, key: &str) -> i64 {
    let mut conn = app
        .state
        .redis_client
        .conn
        .acquire()
        .await
        .expect("redis acquire");
    redis::cmd("TTL")
        .arg(key)
        .query_async(&mut conn)
        .await
        .expect("redis TTL")
}

fn bytes_of(value: &Value) -> Vec<u8> {
    serde_json::from_value(value.clone()).expect("byte array")
}

/// 試料の「保存してあった passkeys 行」を、この利用者の行として入れる。
async fn insert_stored_passkey(app: &TestApp, user_id: Uuid, stored: &Value) {
    passkeys::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        credential_id: Set(bytes_of(&stored["credential_id"])),
        public_key: Set(serde_json::to_vec(&stored["public_key"]).expect("passkey json")),
        aaguid: Set(if stored["aaguid"].is_null() {
            None
        } else {
            Some(bytes_of(&stored["aaguid"]))
        }),
        sign_count: Set(stored["sign_count"].as_i64().expect("sign_count")),
        name: Set("fixture".into()),
        last_used_at: Set(None),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert stored passkey");
}

async fn stored_row(app: &TestApp, user_id: Uuid) -> passkeys::Model {
    passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user_id))
        .one(&app.state.db)
        .await
        .expect("query passkey")
        .expect("passkey row")
}

/// 乱数や利用者ごとの値を伏せる。残りは、上げる前と一字も違わぬことを縛る。
fn masked(mut value: Value, paths: &[&[&str]]) -> Value {
    for path in paths {
        let mut at = &mut value;
        for (i, key) in path.iter().enumerate() {
            match at.get_mut(*key) {
                Some(next) if i + 1 == path.len() => {
                    *next = json!("<masked>");
                    break;
                }
                Some(next) => at = next,
                None => break,
            }
        }
    }
    value
}

#[tokio::test]
async fn registration_options_keep_their_shape() {
    let mut app = TestApp::new().await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    app.login_session_no_content(&user.email, &user.password)
        .await;

    let start = app
        .post_json_with_session("/v1/auth/passkeys/registration/start", json!({}))
        .await;
    assert_eq!(start.status(), StatusCode::OK);
    let now: Value = start.json().await.expect("ccr json");
    let before = fixture("registration")["ccr"].clone();

    let mask: &[&[&str]] = &[
        &["publicKey", "challenge"],
        &["publicKey", "user", "id"],
        &["publicKey", "user", "name"],
        &["publicKey", "user", "displayName"],
    ];
    assert_eq!(masked(now, mask), masked(before, mask));
    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn registration_of_the_previous_version_still_finishes() {
    let mut app = TestApp::new().await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let sample = fixture("registration");

    // 登録の途中の状態（Redis）と、登録の排他ロックを、上げる前の形で据える。
    redis_set(
        &app,
        &format!("webauthn:reg:{}", user.id),
        &sample["state"].to_string(),
        300,
    )
    .await;
    redis_set(&app, &format!("webauthn:reg:lock:{}", user.id), "1", 300).await;

    let finish = app
        .post_json_with_session(
            "/v1/auth/passkeys/registration/finish",
            json!({ "name": "fixture", "credential": sample["credential"] }),
        )
        .await;
    assert_eq!(
        finish.status(),
        StatusCode::CREATED,
        "{}",
        finish.text().await.unwrap_or_default()
    );

    // 書く形も上げる前と同じ（巻き戻しても読める）。
    let row = stored_row(&app, user.id).await;
    let written: Value = serde_json::from_slice(&row.public_key).expect("written passkey json");
    assert_eq!(written, sample["stored"]["public_key"]);
    assert_eq!(
        row.credential_id,
        bytes_of(&sample["stored"]["credential_id"])
    );
    assert_eq!(
        row.sign_count,
        sample["stored"]["sign_count"].as_i64().unwrap()
    );
    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn stored_passkey_of_the_previous_version_still_authenticates() {
    let mut app = TestApp::new().await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    let sample = fixture("authentication");
    insert_stored_passkey(&app, user.id, &sample["stored_before"]).await;

    // start の応答の形（allowCredentials・rpId・userVerification など）が変わらぬこと。
    app.reset_session_client();
    let start = app
        .post_json_with_session(
            "/v1/auth/passkeys/authentication/start",
            json!({ "email": user.email }),
        )
        .await;
    assert_eq!(start.status(), StatusCode::OK);
    let mut options: Value = start.json().await.expect("options json");
    options.as_object_mut().unwrap().remove("challenge_id");
    let mask: &[&[&str]] = &[&["publicKey", "challenge"]];
    assert_eq!(
        masked(options, mask),
        masked(sample["options"].clone(), mask)
    );

    // 上げる前の状態と応答で、認証が通る。
    let challenge_id = Uuid::new_v4();
    redis_set(
        &app,
        &format!("webauthn:auth:{challenge_id}"),
        &sample["state"].to_string(),
        300,
    )
    .await;
    let finish = app
        .post_json_with_session(
            "/v1/auth/passkeys/authentication/finish",
            json!({ "challenge_id": challenge_id, "credential": sample["credential"] }),
        )
        .await;
    assert_eq!(
        finish.status(),
        StatusCode::NO_CONTENT,
        "{}",
        finish.text().await.unwrap_or_default()
    );
    assert_eq!(
        app.get_me().await.status(),
        StatusCode::OK,
        "ログインできている"
    );

    // 認証の後に DB へ書く形（カウンタの更新後の Passkey）も上げる前と同じ。
    let row = stored_row(&app, user.id).await;
    let written: Value = serde_json::from_slice(&row.public_key).expect("written passkey json");
    assert_eq!(written, sample["stored_after"]["public_key"]);
    assert_eq!(
        row.sign_count,
        sample["stored_after"]["sign_count"].as_i64().unwrap()
    );
    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn discoverable_authentication_of_the_previous_version_still_finishes() {
    let mut app = TestApp::new().await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    let sample = fixture("discoverable");
    insert_stored_passkey(&app, user.id, &sample["stored_before"]).await;

    // userHandle は署名の対象でない。試料の利用者の id を、この利用者の id に据える。
    let mut credential = sample["credential"].clone();
    credential["response"]["userHandle"] = json!(super::webauthn_integration::b64url_encode(
        user.id.as_bytes()
    ));

    let challenge_id = Uuid::new_v4();
    redis_set(
        &app,
        &format!("webauthn:auth:disc:{challenge_id}"),
        &sample["state"].to_string(),
        300,
    )
    .await;
    app.reset_session_client();
    let finish = app
        .post_json_with_session(
            "/v1/auth/passkeys/authentication/finish",
            json!({ "challenge_id": challenge_id, "credential": credential }),
        )
        .await;
    assert_eq!(
        finish.status(),
        StatusCode::NO_CONTENT,
        "{}",
        finish.text().await.unwrap_or_default()
    );
    assert_eq!(
        app.get_me().await.status(),
        StatusCode::OK,
        "ログインできている"
    );
    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn reauthentication_of_the_previous_version_still_finishes() {
    let mut app = TestApp::new().await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    let sample = fixture("reauth");
    insert_stored_passkey(&app, user.id, &sample["stored_before"]).await;
    app.login_session_no_content(&user.email, &user.password)
        .await;

    redis_set(
        &app,
        &format!("webauthn:reauth:{}", user.id),
        &sample["state"].to_string(),
        300,
    )
    .await;
    let finish = app
        .post_json_with_session(
            "/v1/auth/reauth/passkey/finish",
            json!({ "credential": sample["credential"] }),
        )
        .await;
    assert_eq!(
        finish.status(),
        StatusCode::NO_CONTENT,
        "{}",
        finish.text().await.unwrap_or_default()
    );
    let status: Value = app
        .get_with_session("/v1/auth/reauth")
        .await
        .json()
        .await
        .expect("reauth json");
    assert_eq!(status["fresh"], json!(true), "再認証が済んだ印が立つ");
    app.cleanup_user(user.id).await;
}

// ---- axum_session: ログイン中のセッション（Redis の値と cookie）----

/// cookie の行から、id と日付を伏せる（試料と同じ形にして比べる）。
fn normalized_cookie(raw: &str) -> String {
    let mut out = Vec::new();
    for part in raw.split("; ") {
        if let Some(rest) = part.strip_prefix("session=") {
            assert!(
                Uuid::parse_str(rest).is_ok(),
                "session cookie は素の uuid: {rest}"
            );
            out.push("session=<id>".to_string());
        } else if part.starts_with("Expires=") {
            out.push("Expires=<date>".to_string());
        } else {
            out.push(part.to_string());
        }
    }
    out.join("; ")
}

fn set_cookies(res: &reqwest::Response) -> Vec<String> {
    res.headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().expect("set-cookie").to_string())
        .collect()
}

async fn login_cookies_and_ttl(is_prod: bool) -> (Vec<String>, i64, Vec<String>) {
    let app = TestApp::new_with_session_config(backend::server::session_config(is_prod)).await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    let login = app
        .client()
        .post(format!("{}/v1/auth/login", app.base_url()))
        .json(&json!({ "email": user.email, "password": user.password }))
        .send()
        .await
        .expect("login");
    assert_eq!(login.status(), StatusCode::NO_CONTENT);
    let cookies = set_cookies(&login);
    let session_cookie = cookies
        .iter()
        .find(|c| c.starts_with("session="))
        .expect("session cookie");
    let id = session_cookie
        .trim_start_matches("session=")
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let key = format!(
        "{}{id}",
        fixture("session")["redis_key_prefix"].as_str().unwrap()
    );
    let ttl = redis_ttl(&app, &key).await;
    let raw = redis_get(&app, &key).await.expect("session value in redis");
    let value: Value = serde_json::from_str(&raw).expect("session value json");
    let mut keys: Vec<String> = value["data"]
        .as_object()
        .expect("data")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    assert_eq!(value["longterm"], json!(false));
    app.cleanup_user(user.id).await;
    (cookies, ttl, keys)
}

#[tokio::test]
async fn login_cookie_and_stored_session_keep_their_shape() {
    let sample = fixture("session");
    for (label, is_prod) in [("dev", false), ("prod", true)] {
        let (cookies, ttl, keys) = login_cookies_and_ttl(is_prod).await;
        // Set-Cookie の行の並びは保証がないので、並びは問わない。
        let mut normalized: Vec<String> = cookies.iter().map(|c| normalized_cookie(c)).collect();
        let mut expected: Vec<String> =
            serde_json::from_value(sample["login_set_cookies"][label].clone()).unwrap();
        normalized.sort();
        expected.sort();
        assert_eq!(normalized, expected, "{label}: Set-Cookie の名と属性");

        // 期限は Redis の TTL に持つ（既定の 6 時間）。
        let lifespan = sample["redis_ttl_secs"].as_i64().unwrap();
        assert!(
            (lifespan - 60..=lifespan).contains(&ttl),
            "{label}: TTL {ttl}"
        );

        let expected_keys: Vec<String> =
            serde_json::from_value(sample["redis_data_keys"].clone()).unwrap();
        assert_eq!(keys, expected_keys, "{label}: セッションに入る鍵");
    }
}

#[tokio::test]
async fn session_of_the_previous_version_stays_logged_in() {
    let app = TestApp::new_with_session_config(backend::server::session_config(false)).await;
    let user = app
        .insert_passkey_user(true, Some("TestPassword123!"))
        .await;
    let sample = fixture("session");

    // 上げる前の版が Redis に書いた形の値を、利用者の id と今の時刻だけ据えて入れる。
    let session_id = Uuid::new_v4().to_string();
    let now_ms = Utc::now().timestamp_millis().to_string();
    let value = sample["redis_value_template"]
        .as_str()
        .unwrap()
        .replace("__USER_ID__", &user.id.to_string())
        .replace("__NOW_MS__", &now_ms);
    let key = format!(
        "{}{session_id}",
        sample["redis_key_prefix"].as_str().unwrap()
    );
    redis_set(&app, &key, &value, 3600).await;

    let client = reqwest::Client::new();
    let get = |path: &'static str| {
        client
            .get(format!("{}{path}", app.base_url()))
            .header(reqwest::header::COOKIE, format!("session={session_id}"))
            .send()
    };
    let me = get("/v1/auth/me").await.expect("me");
    assert_eq!(
        me.status(),
        StatusCode::OK,
        "cookie の id だけで、今ログイン中のセッションが引ける"
    );
    let me: Value = me.json().await.expect("me json");
    assert_eq!(me["id"], json!(user.id.to_string()));

    // 再認証の印（reauthed_at_ms）も、そのまま読める。
    let reauth: Value = get("/v1/auth/reauth")
        .await
        .expect("reauth")
        .json()
        .await
        .expect("reauth json");
    assert_eq!(reauth["fresh"], json!(true));

    // 読んだ後も同じ id で使い続けられ、書き戻された値の形も同じ。
    assert_eq!(
        get("/v1/auth/me").await.expect("me again").status(),
        StatusCode::OK
    );
    let after: Value =
        serde_json::from_str(&redis_get(&app, &key).await.expect("value kept")).unwrap();
    assert_eq!(after["longterm"], json!(false));
    assert_eq!(after["data"]["user_id"], json!(format!("\"{}\"", user.id)));
    app.cleanup_user(user.id).await;
}
