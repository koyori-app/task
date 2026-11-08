use crate::common::{TestApp, TestUser, insert_tenant};
use axum::http::StatusCode;
use entity::project_members::ProjectRole;
use entity::tenant_members::TenantRole;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use uuid::Uuid;

/// tenant_members に指定ロールで一行挿す。
async fn insert_tenant_member(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    role: TenantRole,
) {
    entity::tenant_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        tenant_id: Set(tenant_id),
        user_id: Set(user_id),
        role: Set(role),
        department: Set(None),
        position: Set(None),
        team: Set(None),
    }
    .insert(db)
    .await
    .expect("insert tenant member");
}

/// その利用者の session で鍵の発行を試み、status を返す。
async fn try_create_token(app: &mut TestApp, user: &TestUser, tenant_id: Uuid) -> StatusCode {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let res = app
        .post_json_with_session(
            "/v1/personal_tokens",
            serde_json::json!({
                "name": "boundary-check",
                "tenant_id": tenant_id,
                "scopes": ["read:task"],
            }),
        )
        .await;
    res.status()
}

/// テナントの 2FA を強制にする（強制前に取った session は残したまま）。
async fn enforce_2fa(db: &DatabaseConnection, tenant_id: Uuid) {
    let tenant = entity::tenants::Entity::find_by_id(tenant_id)
        .one(db)
        .await
        .expect("find tenant")
        .expect("tenant exists");
    let mut active: entity::tenants::ActiveModel = tenant.into();
    active.require_2fa = Set(true);
    active.update(db).await.expect("enforce 2fa");
}

fn token_body(tenant_id: Uuid) -> serde_json::Value {
    serde_json::json!({
        "name": "boundary-check",
        "tenant_id": tenant_id,
        "scopes": ["read:task"],
    })
}

/// 2FA を強制するテナントでは、2FA を設定していない Admin は強制前に取った session でも
/// 鍵を作れない（メンバー管理の口と同じく 403 の 2fa-setup-required）。修正前は 201 で通っていた。
#[tokio::test]
async fn admin_without_2fa_cannot_create_token_when_tenant_requires_2fa() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, admin.id, TenantRole::Admin).await;

    // 強制の前に本認証の session を取る
    app.reset_session_client();
    app.login_session_no_content(&admin.email, &admin.password)
        .await;
    enforce_2fa(&app.state.db, tenant_id).await;

    let res = app
        .post_json_with_session("/v1/personal_tokens", token_body(tenant_id))
        .await;
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "強制の下で 2FA が無ければ 403"
    );
    let body: serde_json::Value = res.json().await.expect("error body");
    assert_eq!(body["message"], "2fa-setup-required", "理由を名指しする");
}

/// 対照: 2FA を設定済みの Admin は、強制するテナントでも鍵を作れる。
#[tokio::test]
async fn admin_with_2fa_can_create_token_when_tenant_requires_2fa() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, admin.id, TenantRole::Admin).await;
    let admin_2fa = app.enable_2fa(&admin).await;
    enforce_2fa(&app.state.db, tenant_id).await;

    app.reset_session_client();
    app.login_half_authed(&admin).await;
    let verified = app
        .post_json_with_session(
            "/v1/auth/2fa/verify",
            serde_json::json!({ "recovery_code": admin_2fa.recovery_codes[0] }),
        )
        .await;
    assert_eq!(
        verified.status(),
        StatusCode::NO_CONTENT,
        "2FA 検証で本認証になる"
    );

    let res = app
        .post_json_with_session("/v1/personal_tokens", token_body(tenant_id))
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "2FA 済みなら 201");
}

/// 主は鍵を作れる。
#[tokio::test]
async fn owner_can_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    assert_eq!(
        try_create_token(&mut app, &owner, tenant_id).await,
        StatusCode::CREATED,
        "主は 201"
    );
}

/// role が Admin の member は鍵を作れる（この境目がこの司令で広がった当のもの）。
#[tokio::test]
async fn admin_member_can_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, admin.id, TenantRole::Admin).await;
    assert_eq!(
        try_create_token(&mut app, &admin, tenant_id).await,
        StatusCode::CREATED,
        "テナント Admin は 201"
    );
}

/// role が Member の member は作れぬ。
#[tokio::test]
async fn plain_member_cannot_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, member.id, TenantRole::Member).await;
    assert_eq!(
        try_create_token(&mut app, &member, tenant_id).await,
        StatusCode::FORBIDDEN,
        "Member は 403"
    );
}

/// role が Viewer の member は作れぬ。
#[tokio::test]
async fn viewer_member_cannot_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let viewer = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, viewer.id, TenantRole::Viewer).await;
    assert_eq!(
        try_create_token(&mut app, &viewer, tenant_id).await,
        StatusCode::FORBIDDEN,
        "Viewer は 403"
    );
}

/// テナントに属さぬ者は作れぬ。
#[tokio::test]
async fn outsider_cannot_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let outsider = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    assert_eq!(
        try_create_token(&mut app, &outsider, tenant_id).await,
        StatusCode::FORBIDDEN,
        "非所属は 403"
    );
}

/// project-only の客分（project_members のみ、tenant_members 無し）は作れぬ。
#[tokio::test]
async fn project_only_guest_cannot_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let guest = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    entity::project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(tp.project_id),
        user_id: Set(guest.id),
        role: Set(ProjectRole::Member),
    }
    .insert(&app.state.db)
    .await
    .expect("insert project member");
    assert_eq!(
        try_create_token(&mut app, &guest, tp.tenant_id).await,
        StatusCode::FORBIDDEN,
        "客分は 403"
    );
}
