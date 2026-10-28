//! テナントの 2FA 強制（`tenants.require_2fa`）は、強制前から持っている資格情報にも及ぶ。
//! 規則は docs/features/auth-2fa.md の「テナント強制ポリシー」。
//!
//! 修正前はログイン時（`establish_login_session`）にしか判定が無く、強制前に得た
//! 本認証セッション・PAT・Device Token で 2FA 未設定の利用者がテナントを読み書きできた。
//! 所属判定（`has_tenant_access`）が通すと決めた直後に 2FA を確かめ、
//! 満たさなければ 403（`2fa-setup-required`）を返す。

use crate::common::{
    TestApp, TestUser, current_totp_code, ensure_tenant_member_for_project, insert_extra_project,
    insert_passkey, insert_tenant, secret_from_otpauth_uri,
};
use axum::http::StatusCode;
use chrono::Utc;
use entity::scopes::Scope;
use entity::{project_members, tenants};
use reqwest::{Client, Response};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use uuid::Uuid;

const DETAIL: &str = "2fa-setup-required";

/// 利用者で本認証ログインし、そのセッションのクライアントを切り離して返す
/// （次の利用者のログインで上書きされないよう、TestApp 側は新しいクライアントに替える）。
async fn login_client(app: &mut TestApp, user: &TestUser) -> Client {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let client = app.session_client();
    app.reset_session_client();
    client
}

/// 2FA 済みの利用者でログインし、リカバリーコードで本認証まで通したクライアントを返す。
async fn login_client_with_2fa(app: &mut TestApp, user: &TestUser, recovery_code: &str) -> Client {
    app.reset_session_client();
    app.login_half_authed(user).await;
    let res = app
        .post_json_with_session(
            "/v1/auth/2fa/verify",
            serde_json::json!({ "recovery_code": recovery_code }),
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "2FA 検証で本認証になる"
    );
    let client = app.session_client();
    app.reset_session_client();
    client
}

async fn get_as(app: &TestApp, client: &Client, path: &str) -> Response {
    client
        .get(format!("{}{path}", app.base_url()))
        .send()
        .await
        .expect("get")
}

async fn post_as(app: &TestApp, client: &Client, path: &str, body: serde_json::Value) -> Response {
    client
        .post(format!("{}{path}", app.base_url()))
        .json(&body)
        .send()
        .await
        .expect("post")
}

async fn assert_2fa_required(res: Response, what: &str) {
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "{what}");
    let body: serde_json::Value = res.json().await.expect("error body");
    assert_eq!(body["message"], DETAIL, "{what}: 理由を名指しする");
}

async fn require_2fa_of(app: &TestApp, tenant_id: Uuid) -> bool {
    tenants::Entity::find_by_id(tenant_id)
        .one(&app.state.db)
        .await
        .expect("find tenant")
        .expect("tenant exists")
        .require_2fa
}

/// 強制を有効にした後、2FA 未設定のメンバー・客分は強制前の資格情報でもテナントの口で 403。
/// 2FA 済み・パスキー登録済みの利用者は通り、2FA の設定口は使え、設定を終えれば回復する。
#[tokio::test]
async fn require_2fa_applies_to_existing_credentials() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let member_2fa = app.insert_user_default().await;
    let passkey_member = app.insert_user_default().await;
    let guest = app.insert_user_default().await;

    let tp = app.insert_tenant_project(owner.id).await;
    for user in [&member, &member_2fa, &passkey_member] {
        ensure_tenant_member_for_project(&app.state.db, tp.project_id, user.id).await;
    }
    // project-only の客分: テナントに行を持たず、プロジェクトの明示指定だけで関わる。
    // 明示指定のあるプロジェクトはメンバーに閉じるので、メンバーの検証とは別のプロジェクトに置く
    let guest_project_id = insert_extra_project(&app, tp.tenant_id).await;
    project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(guest_project_id),
        user_id: Set(guest.id),
        role: Set(project_members::ProjectRole::Member),
    }
    .insert(&app.state.db)
    .await
    .expect("insert guest");
    insert_passkey(&app.state.db, passkey_member.id).await;
    let owner_2fa = app.enable_2fa(&owner).await;
    let member_2fa_codes = app.enable_2fa(&member_2fa).await;

    let projects = format!("/v1/tenants/{}/projects", tp.tenant_id);
    let tasks = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let guest_tasks = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, guest_project_id
    );

    // 強制前に資格情報を得ておく
    let member_session = login_client(&mut app, &member).await;
    let guest_session = login_client(&mut app, &guest).await;
    let member_pat = app
        .insert_pat(member.id, tp.tenant_id, vec![Scope::Api], None)
        .await;
    let (member_device, _) = service::desktop_auth::create_device_token(
        &app.state.db,
        &app.state.settings.personal_token_secret,
        member.id,
        "laptop".into(),
        Utc::now(),
    )
    .await
    .expect("device token");
    let passkey_pat = app
        .insert_pat(passkey_member.id, tp.tenant_id, vec![Scope::Api], None)
        .await;

    // 陽性対照: 強制前は読める（以降の 403 が強制によるものである証明）
    assert_eq!(
        get_as(&app, &member_session, &tasks).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        get_as(&app, &guest_session, &guest_tasks).await.status(),
        StatusCode::OK
    );

    // 2FA 済みのオーナーは強制を有効にできる
    let owner_session = login_client_with_2fa(&mut app, &owner, &owner_2fa.recovery_codes[0]).await;
    let res = post_as(
        &app,
        &owner_session,
        &format!("/v1/tenants/{}/require-2fa", tp.tenant_id),
        serde_json::json!({ "enabled": true }),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "2FA 済みのオーナーは有効化できる"
    );
    assert!(require_2fa_of(&app, tp.tenant_id).await);

    for path in [&projects, &tasks] {
        assert_2fa_required(
            get_as(&app, &member_session, path).await,
            &format!("既存セッション {path}"),
        )
        .await;
        assert_2fa_required(
            app.get_with_bearer(path, &member_pat).await,
            &format!("PAT {path}"),
        )
        .await;
        assert_2fa_required(
            app.get_with_bearer(path, &member_device).await,
            &format!("Device Token {path}"),
        )
        .await;
    }
    for path in [&projects, &guest_tasks] {
        assert_2fa_required(
            get_as(&app, &guest_session, path).await,
            &format!("客分の既存セッション {path}"),
        )
        .await;
    }

    // 対照: 2FA 済みのオーナー・メンバー、パスキー登録済み（TOTP 未設定）のメンバーは通る
    let member_2fa_session =
        login_client_with_2fa(&mut app, &member_2fa, &member_2fa_codes.recovery_codes[0]).await;
    for path in [&projects, &tasks] {
        assert_eq!(
            get_as(&app, &owner_session, path).await.status(),
            StatusCode::OK,
            "2FA 済みのオーナー {path}"
        );
        assert_eq!(
            get_as(&app, &member_2fa_session, path).await.status(),
            StatusCode::OK,
            "2FA 済みのメンバー {path}"
        );
        assert_eq!(
            app.get_with_bearer(path, &passkey_pat).await.status(),
            StatusCode::OK,
            "パスキー登録済みのメンバー {path}"
        );
    }

    // 2FA の設定口はテナントの所属判定を通らないので、未設定のメンバーも使える
    assert_eq!(
        get_as(&app, &member_session, "/v1/auth/me").await.status(),
        StatusCode::OK
    );
    let setup = post_as(
        &app,
        &member_session,
        "/v1/auth/2fa/totp/setup",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(setup.status(), StatusCode::OK, "TOTP の設定を始められる");
    let setup: serde_json::Value = setup.json().await.expect("setup json");
    let secret = secret_from_otpauth_uri(setup["otpauth_uri"].as_str().expect("otpauth_uri"));
    let code = current_totp_code(&secret, &app.state.settings.totp_issuer, &member.email);
    let verify = post_as(
        &app,
        &member_session,
        "/v1/auth/2fa/totp/verify-setup",
        serde_json::json!({ "code": code }),
    )
    .await;
    assert_eq!(verify.status(), StatusCode::OK, "TOTP の設定を終えられる");

    // 設定を終えれば同じ資格情報で回復する
    assert_eq!(
        get_as(&app, &member_session, &tasks).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        app.get_with_bearer(&tasks, &member_pat).await.status(),
        StatusCode::OK
    );

    for user in [&member, &member_2fa, &passkey_member, &guest, &owner] {
        app.cleanup_user(user.id).await;
    }
}

/// 2FA 未設定のオーナーは強制を有効にできない（有効化した途端に自分も締め出される）。
/// 無効化はいつでもできる。
#[tokio::test]
async fn owner_without_2fa_cannot_enable_require_2fa() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    let path = format!("/v1/tenants/{tenant_id}/require-2fa");

    let session = login_client(&mut app, &owner).await;
    assert_2fa_required(
        post_as(
            &app,
            &session,
            &path,
            serde_json::json!({ "enabled": true }),
        )
        .await,
        "2FA 未設定のオーナーの有効化",
    )
    .await;
    assert!(
        !require_2fa_of(&app, tenant_id).await,
        "拒否したら require_2fa は変わらない"
    );

    let res = post_as(
        &app,
        &session,
        &path,
        serde_json::json!({ "enabled": false }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "無効化は拒否しない");

    // 対照: パスキーを登録すれば 2FA を満たし、有効化できる
    insert_passkey(&app.state.db, owner.id).await;
    let res = post_as(
        &app,
        &session,
        &path,
        serde_json::json!({ "enabled": true }),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "パスキー登録済みのオーナーは有効化できる"
    );
    assert!(require_2fa_of(&app, tenant_id).await);

    app.cleanup_user(owner.id).await;
}
