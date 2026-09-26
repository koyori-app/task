//! Koyori Desktop の認証（Authorization Code + PKCE → Device Token）と端末管理。
//! 規則は apps/backend/docs/personal-access-tokens-authz.md の「Desktop 認証」。

use crate::common::{TestApp, TestUser, insert_tenant};
use axum::http::StatusCode;
use backend::utils::desktop_auth::{TOKEN_RATE_LIMIT, code_key, s256_challenge};
use chrono::Utc;
use entity::scopes::Scope;
use entity::{device_tokens, users};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use uuid::Uuid;

const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

/// ログイン済みセッションで認可コードを発行する。
async fn issue_code(app: &TestApp, verifier: &str) -> String {
    let res = app
        .post_json_with_session(
            "/v1/desktop/auth/codes",
            serde_json::json!({ "code_challenge": s256_challenge(verifier), "name": "laptop" }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "セッションで発行できる");
    let body: serde_json::Value = res.json().await.expect("code body");
    body["code"].as_str().expect("code").to_string()
}

/// 未認証のクライアントで交換する。レート制限が他のテストと混ざらないよう接続元を毎回変える。
async fn exchange(app: &TestApp, code: &str, verifier: &str) -> reqwest::Response {
    exchange_from(app, code, verifier, &Uuid::new_v4().to_string()).await
}

async fn exchange_from(
    app: &TestApp,
    code: &str,
    verifier: &str,
    client_ip: &str,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{}/v1/desktop/auth/token", app.base_url()))
        .header("x-forwarded-for", client_ip)
        .json(&serde_json::json!({ "code": code, "code_verifier": verifier }))
        .send()
        .await
        .expect("token request")
}

/// ログインから交換まで通し、(平文トークン, 端末 ID) を返す。
async fn login_and_get_device_token(app: &mut TestApp, user: &TestUser) -> (String, Uuid) {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let code = issue_code(app, VERIFIER).await;
    let res = exchange(app, &code, VERIFIER).await;
    assert_eq!(res.status(), StatusCode::CREATED, "交換できる");
    let body: serde_json::Value = res.json().await.expect("token body");
    (
        body["token"].as_str().expect("token").to_string(),
        body["device_id"]
            .as_str()
            .expect("device_id")
            .parse()
            .expect("uuid"),
    )
}

async fn get_bearer(app: &TestApp, path: &str, token: &str) -> StatusCode {
    app.get_with_bearer(path, token).await.status()
}

/// 発行 → 交換 → Bearer でアカウント単位の通知一覧とテナント配下の API に届く。
#[tokio::test]
async fn full_flow_reaches_account_and_tenant_apis() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let other = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let other_tenant_id = insert_tenant(&app.state.db, other.id).await;

    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let code = issue_code(&app, VERIFIER).await;
    let res = exchange(&app, &code, VERIFIER).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = res.json().await.expect("token body");
    let token = body["token"].as_str().expect("token");
    assert!(token.starts_with("kdt_"), "接頭辞で PAT と区別する");
    let expires_at: chrono::DateTime<Utc> = body["expires_at"]
        .as_str()
        .expect("expires_at")
        .parse()
        .expect("rfc3339");
    let days = (expires_at - Utc::now()).num_days();
    assert!((89..=90).contains(&days), "期限は発行から 90 日: {days}");

    assert_eq!(
        get_bearer(&app, "/v1/users/me/notifications", token).await,
        StatusCode::OK
    );
    assert_eq!(
        get_bearer(&app, "/v1/users/me/devices", token).await,
        StatusCode::OK
    );
    // テナント束縛は無く、所属判定はセッションと同じ
    assert_eq!(
        get_bearer(&app, &format!("/v1/tenants/{tenant_id}/projects"), token).await,
        StatusCode::OK
    );
    assert_eq!(
        get_bearer(
            &app,
            &format!("/v1/tenants/{other_tenant_id}/projects"),
            token
        )
        .await,
        StatusCode::FORBIDDEN,
        "所属していないテナントは 403"
    );

    let row = device_tokens::Entity::find_by_id(
        body["device_id"]
            .as_str()
            .expect("device_id")
            .parse::<Uuid>()
            .expect("uuid"),
    )
    .one(&app.state.db)
    .await
    .expect("select")
    .expect("row");
    assert_eq!(row.name, "laptop");
    assert!(row.last_used_at.is_some(), "使うと last_used_at が入る");
    assert_ne!(row.token_hash, token, "平文は保存しない");

    app.cleanup_user(user.id).await;
    app.cleanup_user(other.id).await;
}

#[tokio::test]
async fn code_cannot_be_reused() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;

    let code = issue_code(&app, VERIFIER).await;
    assert_eq!(
        exchange(&app, &code, VERIFIER).await.status(),
        StatusCode::CREATED
    );
    assert_eq!(
        exchange(&app, &code, VERIFIER).await.status(),
        StatusCode::UNAUTHORIZED,
        "2 回目は 401"
    );

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn verifier_mismatch_is_401_and_consumes_the_code() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;

    let code = issue_code(&app, VERIFIER).await;
    let wrong = "x".repeat(43);
    assert_eq!(
        exchange(&app, &code, &wrong).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        exchange(&app, &code, VERIFIER).await.status(),
        StatusCode::UNAUTHORIZED,
        "不一致の時点で code は消費済み"
    );

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn expired_code_is_401() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;

    let code = issue_code(&app, VERIFIER).await;
    let mut conn = app.state.redis_client.conn.acquire().await.expect("redis");
    let ttl: i64 = redis::cmd("TTL")
        .arg(code_key(&code))
        .query_async(&mut conn)
        .await
        .expect("TTL");
    assert!((1..=300).contains(&ttl), "TTL は 5 分以内: {ttl}");
    let _: i64 = redis::cmd("PEXPIRE")
        .arg(code_key(&code))
        .arg(1)
        .query_async(&mut conn)
        .await
        .expect("PEXPIRE");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert_eq!(
        exchange(&app, &code, VERIFIER).await.status(),
        StatusCode::UNAUTHORIZED
    );

    app.cleanup_user(user.id).await;
}

/// 交換口は接続元ごとに上限まで通し、越えたら 429。
#[tokio::test]
async fn token_exchange_is_rate_limited() {
    let app = TestApp::new().await;
    let ip = Uuid::new_v4().to_string();

    for _ in 0..TOKEN_RATE_LIMIT {
        assert_eq!(
            exchange_from(&app, "no-such-code", VERIFIER, &ip)
                .await
                .status(),
            StatusCode::UNAUTHORIZED,
            "上限までは通常の判定"
        );
    }
    assert_eq!(
        exchange_from(&app, "no-such-code", VERIFIER, &ip)
            .await
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        exchange(&app, "no-such-code", VERIFIER).await.status(),
        StatusCode::UNAUTHORIZED,
        "別の接続元は影響を受けない"
    );
}

#[tokio::test]
async fn code_request_is_validated() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;

    let post = |challenge: String, name: String| {
        let app = &app;
        async move {
            app.post_json_with_session(
                "/v1/desktop/auth/codes",
                serde_json::json!({ "code_challenge": challenge, "name": name }),
            )
            .await
            .status()
        }
    };
    assert_eq!(
        post("a".repeat(42), "pc".into()).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(post("a".repeat(43), "pc".into()).await, StatusCode::CREATED);
    assert_eq!(
        post("a".repeat(129), "pc".into()).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(format!("{}+", "a".repeat(42)), "pc".into()).await,
        StatusCode::BAD_REQUEST,
        "base64url 以外の文字"
    );
    assert_eq!(
        post("a".repeat(43), "n".repeat(100)).await,
        StatusCode::CREATED
    );
    assert_eq!(
        post("a".repeat(43), "n".repeat(101)).await,
        StatusCode::BAD_REQUEST
    );

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn half_authed_session_cannot_issue_code() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.enable_2fa(&user).await;
    app.login_half_authed(&user).await;

    let res = app
        .post_json_with_session(
            "/v1/desktop/auth/codes",
            serde_json::json!({ "code_challenge": s256_challenge(VERIFIER), "name": "pc" }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    app.cleanup_user(user.id).await;
}

/// 発行口はセッション専用。PAT も Device Token も 403。
#[tokio::test]
async fn bearer_cannot_issue_code() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let pat = app
        .insert_pat(user.id, tenant_id, vec![Scope::AdminTenant], None)
        .await;
    let (device, _) = login_and_get_device_token(&mut app, &user).await;

    let body = serde_json::json!({ "code_challenge": s256_challenge(VERIFIER), "name": "pc" });
    for token in [&pat, &device] {
        let res = reqwest::Client::new()
            .post(format!("{}/v1/desktop/auth/codes", app.base_url()))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .expect("codes request");
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    app.cleanup_user(user.id).await;
}

/// セッション専用の口（PAT 管理・テナント作成）は Device Token では通らない。
/// 同じ利用者のセッションなら通る（対照）。
#[tokio::test]
async fn device_token_cannot_use_session_only_endpoints() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let (device, _) = login_and_get_device_token(&mut app, &user).await;
    let client = reqwest::Client::new();

    assert_eq!(
        get_bearer(&app, "/v1/personal_tokens", &device).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.get_with_session("/v1/personal_tokens").await.status(),
        StatusCode::OK
    );

    let tenant = |display_id: &str| serde_json::json!({ "display_id": display_id, "name": "Desk" });
    let res = client
        .post(format!("{}/v1/tenants", app.base_url()))
        .bearer_auth(&device)
        .json(&tenant(&format!(
            "dt{}",
            &Uuid::new_v4().simple().to_string()[..8]
        )))
        .send()
        .await
        .expect("create tenant");
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let res = app
        .post_json_with_session(
            "/v1/tenants",
            tenant(&format!("ss{}", &Uuid::new_v4().simple().to_string()[..8])),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    app.cleanup_user(user.id).await;
}

/// 失効は Web のセッションからも、その Device Token 自身からもできる。失効後は 401。
#[tokio::test]
async fn revoked_device_token_is_401() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;

    let (by_web, by_web_id) = login_and_get_device_token(&mut app, &user).await;
    let (by_self, by_self_id) = login_and_get_device_token(&mut app, &user).await;

    let devices: Vec<serde_json::Value> = app
        .get_with_bearer("/v1/users/me/devices", &by_self)
        .await
        .json()
        .await
        .expect("devices");
    assert_eq!(devices.len(), 2);
    assert!(devices.iter().all(|d| d.get("token").is_none()));

    let res = app
        .delete_with_session(&format!("/v1/users/me/devices/{by_web_id}"))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        get_bearer(&app, "/v1/users/me/notifications", &by_web).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get_bearer(&app, "/v1/users/me/notifications", &by_self).await,
        StatusCode::OK,
        "他の端末は影響を受けない"
    );

    let res = reqwest::Client::new()
        .delete(format!(
            "{}/v1/users/me/devices/{by_self_id}",
            app.base_url()
        ))
        .bearer_auth(&by_self)
        .send()
        .await
        .expect("self revoke");
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        get_bearer(&app, "/v1/users/me/notifications", &by_self).await,
        StatusCode::UNAUTHORIZED
    );

    let row = device_tokens::Entity::find_by_id(by_web_id)
        .one(&app.state.db)
        .await
        .expect("select")
        .expect("失効しても行は残す");
    assert!(row.revoked_at.is_some());
    let listed: Vec<serde_json::Value> = app
        .get_with_session("/v1/users/me/devices")
        .await
        .json()
        .await
        .expect("devices");
    assert!(listed.is_empty(), "失効済みは一覧に出ない");

    app.cleanup_user(user.id).await;
}

/// `sessions_revoked_at` より前に発行した端末は 401、後に発行した端末は通る。
#[tokio::test]
async fn sessions_revoked_at_invalidates_older_device_tokens() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let (old, _) = login_and_get_device_token(&mut app, &user).await;

    let mut active: users::ActiveModel = users::Entity::find_by_id(user.id)
        .one(&app.state.db)
        .await
        .expect("select")
        .expect("user")
        .into();
    active.sessions_revoked_at = Set(Some(Utc::now().into()));
    active.update(&app.state.db).await.expect("revoke sessions");

    assert_eq!(
        get_bearer(&app, "/v1/users/me/notifications", &old).await,
        StatusCode::UNAUTHORIZED
    );

    let (new, _) = login_and_get_device_token(&mut app, &user).await;
    assert_eq!(
        get_bearer(&app, "/v1/users/me/notifications", &new).await,
        StatusCode::OK,
        "失効時刻より後に発行した端末は通る"
    );

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn sessions_revoked_at_also_invalidates_unexchanged_codes() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    for offset_ms in [0, 1] {
        app.reset_session_client();
        app.login_session_no_content(&user.email, &user.password)
            .await;
        let code = issue_code(&app, VERIFIER).await;
        let issued_at_ms = {
            let mut conn = app.state.redis_client.conn.acquire().await.unwrap();
            let raw: String = redis::cmd("GET")
                .arg(code_key(&code))
                .query_async(&mut conn)
                .await
                .unwrap();
            serde_json::from_str::<serde_json::Value>(&raw).unwrap()["issued_at_ms"]
                .as_i64()
                .unwrap()
        };
        let mut active: users::ActiveModel = users::Entity::find_by_id(user.id)
            .one(&app.state.db)
            .await
            .unwrap()
            .unwrap()
            .into();
        active.sessions_revoked_at = Set(Some(
            chrono::DateTime::from_timestamp_millis(issued_at_ms + offset_ms)
                .unwrap()
                .into(),
        ));
        active.update(&app.state.db).await.unwrap();
        assert_eq!(
            exchange(&app, &code, VERIFIER).await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            exchange(&app, &code, VERIFIER).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
    // 交換時の利用者検査を通過した直後に全失効した場合も、承認時刻で失効させる。
    let (token, _) = service::desktop_auth::create_device_token(
        &app.state.db,
        &app.state.settings.personal_token_secret,
        user.id,
        "in-flight exchange".into(),
        Utc::now() - chrono::Duration::hours(1),
    )
    .await
    .unwrap();
    assert_eq!(
        get_bearer(&app, "/v1/users/me/notifications", &token).await,
        StatusCode::UNAUTHORIZED
    );
    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn suspension_rejects_and_consumes_unexchanged_codes() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let code = issue_code(&app, VERIFIER).await;
    let mut active: users::ActiveModel = users::Entity::find_by_id(user.id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    active.is_suspended = Set(true);
    let row = active.update(&app.state.db).await.unwrap();
    assert_eq!(
        exchange(&app, &code, VERIFIER).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let mut active: users::ActiveModel = row.into();
    active.is_suspended = Set(false);
    active.update(&app.state.db).await.unwrap();
    assert_eq!(
        exchange(&app, &code, VERIFIER).await.status(),
        StatusCode::UNAUTHORIZED
    );
    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn codes_without_an_authorization_timestamp_are_rejected() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let code = issue_code(&app, VERIFIER).await;
    {
        let mut conn = app.state.redis_client.conn.acquire().await.unwrap();
        let _: () = redis::cmd("SET").arg(code_key(&code))
            .arg(serde_json::json!({
                "user_id": user.id, "code_challenge": s256_challenge(VERIFIER), "name": "legacy",
            }).to_string()).arg("EX").arg(60)
            .query_async(&mut conn).await.unwrap();
    }
    assert_eq!(
        exchange(&app, &code, VERIFIER).await.status(),
        StatusCode::UNAUTHORIZED
    );
    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn other_users_device_is_404() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let other = app.insert_user_default().await;
    let (token, device_id) = login_and_get_device_token(&mut app, &owner).await;

    app.reset_session_client();
    app.login_session_no_content(&other.email, &other.password)
        .await;
    let res = app
        .delete_with_session(&format!("/v1/users/me/devices/{device_id}"))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        get_bearer(&app, "/v1/users/me/notifications", &token).await,
        StatusCode::OK,
        "他人の失効要求では落ちない"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(other.id).await;
}

#[tokio::test]
async fn expired_device_token_is_401() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let (token, device_id) = login_and_get_device_token(&mut app, &user).await;
    assert_eq!(
        get_bearer(&app, "/v1/users/me/notifications", &token).await,
        StatusCode::OK
    );

    let mut active: device_tokens::ActiveModel = device_tokens::Entity::find_by_id(device_id)
        .one(&app.state.db)
        .await
        .expect("select")
        .expect("row")
        .into();
    active.expires_at = Set((Utc::now() - chrono::Duration::seconds(1)).into());
    active.update(&app.state.db).await.expect("expire");

    assert_eq!(
        get_bearer(&app, "/v1/users/me/notifications", &token).await,
        StatusCode::UNAUTHORIZED
    );

    app.cleanup_user(user.id).await;
}

/// 通知を 1 件直に挿す（どの経路で作られたかは視界の判定に関係しない）。
/// カーソルの並びを決めるため、作成時刻は `minutes_ago` 分前にずらす。
async fn insert_notification(
    app: &TestApp,
    user_id: Uuid,
    project_id: Option<Uuid>,
    minutes_ago: i64,
) {
    entity::notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        task_id: Set(None),
        project_id: Set(project_id),
        notification_type: Set("review_round_created".into()),
        payload: Set(serde_json::json!({ "pr_number": 618, "round": 1 })),
        read_at: Set(None),
        created_at: Set((Utc::now() - chrono::Duration::minutes(minutes_ago)).into()),
        email_queued_at: Set(None),
        emailed_at: Set(None),
        email_attempts: Set(0),
    }
    .insert(&app.state.db)
    .await
    .expect("insert notification");
}

async fn unread_count(res: reqwest::Response) -> (u64, usize) {
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = res.json().await.expect("json");
    (
        body["unread_count"].as_u64().expect("unread_count"),
        body["notifications"]
            .as_array()
            .expect("notifications")
            .len(),
    )
}

/// Device Token の通知の視界はセッションと同じ。テナントをまたぐ通知も
/// `project_id` の無い古い通知も見え、PAT のテナント絞り込みは掛からない。
#[tokio::test]
async fn device_token_sees_the_same_notifications_as_the_session() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let a = app.insert_tenant_project(user.id).await;
    let b = app.insert_tenant_project(user.id).await;
    insert_notification(&app, user.id, Some(a.project_id), 3).await;
    insert_notification(&app, user.id, Some(b.project_id), 2).await;
    insert_notification(&app, user.id, None, 1).await;

    let (device, _) = login_and_get_device_token(&mut app, &user).await;
    let session = unread_count(app.get_with_session("/v1/users/me/notifications").await).await;
    assert_eq!(session, (3, 3));
    assert_eq!(
        unread_count(
            app.get_with_bearer("/v1/users/me/notifications", &device)
                .await
        )
        .await,
        session,
        "セッションと同じ件数"
    );

    // 対照: テナント A に束縛した PAT は A の 1 件だけ（絞り込みが効く経路との違い）。
    // PAT は種別に合うスコープの通知しか見えないので、レビュー通知には read:review を持たせる
    let pat = app
        .insert_pat(user.id, a.tenant_id, vec![Scope::ReadReview], None)
        .await;
    assert_eq!(
        unread_count(
            app.get_with_bearer("/v1/users/me/notifications", &pat)
                .await
        )
        .await,
        (1, 1)
    );

    // 全既読も同じ視界で効く
    let res = reqwest::Client::new()
        .patch(format!(
            "{}/v1/users/me/notifications/read-all",
            app.base_url()
        ))
        .bearer_auth(&device)
        .send()
        .await
        .expect("read-all");
    assert!(res.status().is_success(), "read-all: {}", res.status());
    assert_eq!(
        unread_count(app.get_with_session("/v1/users/me/notifications").await).await,
        (0, 3)
    );

    app.cleanup_user(user.id).await;
}

/// Device Token でもカーソルのページ送り（`cursor`）と catch-up（`after`）が通り、
/// テナントをまたぐ通知を取り切れる。
#[tokio::test]
async fn device_token_pages_notifications_with_cursor_and_after() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let a = app.insert_tenant_project(user.id).await;
    let b = app.insert_tenant_project(user.id).await;
    insert_notification(&app, user.id, Some(a.project_id), 3).await;
    insert_notification(&app, user.id, Some(b.project_id), 2).await;
    insert_notification(&app, user.id, None, 1).await;
    let (device, _) = login_and_get_device_token(&mut app, &user).await;

    let page = |path: String| {
        let app = &app;
        let device = device.clone();
        async move {
            let res = app.get_with_bearer(&path, &device).await;
            assert_eq!(res.status(), StatusCode::OK, "{path}");
            res.json::<serde_json::Value>().await.expect("json")
        }
    };
    let cursors = |body: &serde_json::Value| -> Vec<String> {
        body["notifications"]
            .as_array()
            .expect("notifications")
            .iter()
            .map(|n| n["cursor"].as_str().expect("cursor").to_string())
            .collect()
    };

    // 新しい順に 2 件 → cursor で残り 1 件
    let first = page("/v1/users/me/notifications?limit=2".into()).await;
    assert_eq!(cursors(&first).len(), 2);
    let next = first["next_cursor"].as_str().expect("next_cursor");
    let second = page(format!("/v1/users/me/notifications?limit=2&cursor={next}")).await;
    assert_eq!(
        cursors(&second).len(),
        1,
        "3 件目（別テナント・project_id 無しを含む視界）"
    );

    // 最も古い行より新しいものを after で引くと残りの 2 件
    let oldest = &cursors(&second)[0];
    let newer = page(format!("/v1/users/me/notifications?after={oldest}")).await;
    assert_eq!(cursors(&newer).len(), 2);

    app.cleanup_user(user.id).await;
}
