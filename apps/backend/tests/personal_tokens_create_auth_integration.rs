mod common;

use axum::http::StatusCode;
use common::{TestApp, TestUser, insert_tenant};
use entity::project_members::ProjectRole;
use entity::tenant_members::TenantRole;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
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
