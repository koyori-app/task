//! テナントの 2FA 強制（`tenants.require_2fa`）は、強制前から持っている資格情報にも及ぶ。
//! 規則は docs/features/auth-2fa.md の「テナント強制ポリシー」。
//!
//! 修正前はログイン時（`establish_login_session`）にしか判定が無く、強制前に得た
//! 本認証セッション・PAT・Device Token で 2FA 未設定の利用者がテナントを読み書きできた。
//! 所属判定（`has_tenant_access`）が通すと決めた直後に 2FA を確かめ、
//! 満たさなければ 403（`2fa-setup-required`）を返す。所属判定を通らない口
//! （ファイル本文の配信・オーナー専用の口）も同じ判定を通し、通知の一覧は強制下のテナントの
//! 通知を視界から外す。

use crate::common::{
    TestApp, TestUser, current_totp_code, ensure_tenant_member_for_project, file_id_of,
    insert_extra_project, insert_passkey, insert_tenant, secret_from_otpauth_uri,
    upload_drive_file,
};
use axum::http::StatusCode;
use chrono::Utc;
use entity::scopes::Scope;
use entity::{drive_folders, project_members, tenants};
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

async fn put_as(app: &TestApp, client: &Client, path: &str, body: serde_json::Value) -> Response {
    client
        .put(format!("{}{path}", app.base_url()))
        .json(&body)
        .send()
        .await
        .expect("put")
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

    // プロジェクトのファイル。本文の配信は所属判定を通らずに通す口なので別に確かめる
    let folder_id = Uuid::new_v4();
    drive_folders::ActiveModel {
        id: Set(folder_id),
        name: Set("project-folder".into()),
        parent_id: Set(None),
        tenant_id: Set(tp.tenant_id),
        project_id: Set(Some(tp.project_id)),
        created_by: Set(owner.id),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert project folder");
    app.login_session_no_content(&member.email, &member.password)
        .await;
    let file = upload_drive_file(
        &app,
        tp.tenant_id,
        Some(folder_id),
        "secret.txt",
        "text/plain",
        b"secret",
    )
    .await;
    let content = format!("/v1/drive/files/{}/content", file_id_of(&file));

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
        get_as(&app, &member_session, &content).await.status(),
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

    for path in [&projects, &tasks, &content] {
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
    for path in [&projects, &tasks, &content] {
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

    // オーナーを先に消す（テナントごとメンバーが上げたファイルも消え、外部キーに掛からない）
    for user in [&owner, &member, &member_2fa, &passkey_member, &guest] {
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

/// 強制下では、オーナー専用の口（テナント更新・GitHub 連携など）も 2FA を満たさないオーナーを
/// 403 で止める。強制の無効化だけは救済経路として通す。
#[tokio::test]
async fn owner_routes_require_2fa_except_disabling() {
    let mut app = TestApp::new_with_github().await;
    let owner = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    let session = login_client(&mut app, &owner).await;
    let tenant_path = format!("/v1/tenants/{}", tp.tenant_id);
    let github_path = format!(
        "/v1/tenants/{}/projects/{}/github/integration",
        tp.tenant_id, tp.project_id
    );
    let rename = serde_json::json!({ "name": "renamed" });

    // 強制下に 2FA の無いオーナーが残る状態（強制の有効化ガードより前のデータ・パスキーの削除後）を
    // DB で作る
    let mut active: tenants::ActiveModel = tenants::Entity::find_by_id(tp.tenant_id)
        .one(&app.state.db)
        .await
        .expect("find tenant")
        .expect("tenant exists")
        .into();
    active.require_2fa = Set(true);
    active
        .update(&app.state.db)
        .await
        .expect("enable require_2fa");

    assert_2fa_required(
        put_as(&app, &session, &tenant_path, rename.clone()).await,
        "テナント更新",
    )
    .await;
    assert_2fa_required(get_as(&app, &session, &github_path).await, "GitHub 連携").await;

    let res = post_as(
        &app,
        &session,
        &format!("{tenant_path}/require-2fa"),
        serde_json::json!({ "enabled": false }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "無効化は 2FA 無しでも通る");
    assert!(!require_2fa_of(&app, tp.tenant_id).await);

    // 強制が外れれば通る（以上の 403 が強制によるものである証明）
    assert_eq!(
        put_as(&app, &session, &tenant_path, rename).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        get_as(&app, &session, &github_path).await.status(),
        StatusCode::OK
    );

    app.cleanup_user(owner.id).await;
}

/// 通知を 1 件直に挿す（視界の判定は `project_id` だけを見る）。
async fn insert_notification(app: &TestApp, user_id: Uuid, project_id: Uuid) {
    entity::notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        task_id: Set(None),
        project_id: Set(Some(project_id)),
        notification_type: Set("review_round_created".into()),
        payload: Set(serde_json::json!({ "pr_number": 1, "round": 1 })),
        read_at: Set(None),
        created_at: Set(Utc::now().into()),
        email_queued_at: Set(None),
        emailed_at: Set(None),
        email_attempts: Set(0),
    }
    .insert(&app.state.db)
    .await
    .expect("insert notification");
}

/// 通知の一覧の（未読数, 件数）。
async fn notification_counts(res: Response) -> (u64, usize) {
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

/// 通知の一覧は所属判定を通らずに視界を組むが、強制下のテナントの通知は 2FA を満たすまで
/// 一覧にも未読数にも出さない。強制していないテナントの通知は出続ける。
#[tokio::test]
async fn require_2fa_hides_tenant_notifications() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let member_2fa = app.insert_user_default().await;

    let enforced = app.insert_tenant_project(owner.id).await;
    for user in [&member, &member_2fa] {
        ensure_tenant_member_for_project(&app.state.db, enforced.project_id, user.id).await;
    }
    // member が自分で持つ、強制していないテナント
    let own = app.insert_tenant_project(member.id).await;
    let member_2fa_codes = app.enable_2fa(&member_2fa).await;
    insert_notification(&app, member.id, enforced.project_id).await;
    insert_notification(&app, member.id, own.project_id).await;
    insert_notification(&app, member_2fa.id, enforced.project_id).await;

    let notifications = "/v1/users/me/notifications";
    let member_session = login_client(&mut app, &member).await;
    let member_pat = app
        .insert_pat(member.id, enforced.tenant_id, vec![Scope::ReadReview], None)
        .await;

    // 陽性対照: 強制前は強制するテナントの通知も見える
    assert_eq!(
        notification_counts(get_as(&app, &member_session, notifications).await).await,
        (2, 2)
    );
    assert_eq!(
        notification_counts(app.get_with_bearer(notifications, &member_pat).await).await,
        (1, 1)
    );

    let tenant = tenants::Entity::find_by_id(enforced.tenant_id)
        .one(&app.state.db)
        .await
        .expect("find tenant")
        .expect("tenant exists");
    let mut tenant: tenants::ActiveModel = tenant.into();
    tenant.require_2fa = Set(true);
    tenant
        .update(&app.state.db)
        .await
        .expect("enable require_2fa");

    assert_eq!(
        notification_counts(get_as(&app, &member_session, notifications).await).await,
        (1, 1),
        "既存セッション: 強制していないテナントの通知だけ"
    );
    assert_eq!(
        notification_counts(app.get_with_bearer(notifications, &member_pat).await).await,
        (0, 0),
        "強制するテナントの PAT"
    );

    // 対照: 2FA 済みのメンバーには強制下のテナントの通知も出る
    let member_2fa_session =
        login_client_with_2fa(&mut app, &member_2fa, &member_2fa_codes.recovery_codes[0]).await;
    assert_eq!(
        notification_counts(get_as(&app, &member_2fa_session, notifications).await).await,
        (1, 1)
    );

    for user in [&owner, &member, &member_2fa] {
        app.cleanup_user(user.id).await;
    }
}
