mod common;

use axum::http::StatusCode;
use common::TestApp;
use entity::projects;
use entity::scopes::{Scope, ScopeList};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use uuid::Uuid;

/// `admin:project`（project 層の wildcard）のスコープ判定の固定。
///
/// ここで固定する契約（apps/backend/docs/personal-access-tokens-authz.md の層の割り振り表）:
/// 1. project 層の口（プロジェクト一覧・取得）: 列挙鍵（read:project）・admin:tenant 鍵・
///    admin:project 鍵のいずれも通る。セッションも通る
/// 2. tenant 層の口（テナント取得 = admin:tenant 要求）: admin:tenant 鍵とセッションは通り、
///    admin:project 鍵・列挙鍵は 403（スコープの層を越えない）
/// 3. `allowed_project_ids` 束縛つきの admin:project 鍵は、束縛内 project で通り、
///    束縛外 project とテナント全体の口で 403（束縛はスコープと独立に効く）
async fn insert_second_project(db: &DatabaseConnection, tenant_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    let suffix = &id.to_string()[..8];
    projects::ActiveModel {
        id: Set(id),
        name: Set("scope-other".into()),
        description: Set(String::new()),
        tenant_id: Set(tenant_id),
        icon_emoji: Set(None),
        icon_url: Set(None),
        // テナントごとに一意なキー。project key 制約 ^[A-Z][A-Z0-9]{1,9}$ を満たす
        key: Set(format!("S{}", suffix.to_uppercase())),
        is_personal: Set(false),
        personal_owner_id: Set(None),
    }
    .insert(db)
    .await
    .expect("insert second project");
    id
}

/// scopes と allowed_project_ids を指定して PAT を挿す
/// （`insert_personal_token_for_test` は admin:tenant 固定のため）。
async fn insert_pat(
    app: &TestApp,
    user_id: Uuid,
    tenant_id: Uuid,
    scopes: Vec<Scope>,
    allowed_project_ids: Option<Vec<Uuid>>,
) -> String {
    let (token, token_hash) =
        backend::utils::auth::generate_personal_token(&app.state.settings.personal_token_secret)
            .expect("generate pat");
    entity::personal_tokens::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set("admin-project-scope-test".into()),
        token_last_four: Set(token[token.len().saturating_sub(4)..].to_string()),
        token_hash: Set(token_hash),
        expires_at: Set(None),
        last_used_at: Set(None),
        revoked: Set(false),
        user_id: Set(user_id),
        scopes: Set(ScopeList(scopes)),
        tenant_id: Set(tenant_id),
        allowed_project_ids: Set(allowed_project_ids.map(|ids| serde_json::json!(ids))),
    }
    .insert(&app.state.db)
    .await
    .expect("insert pat");
    token
}

#[tokio::test]
async fn admin_project_scope_layers_for_session_and_pat() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;

    let tenant_path = format!("/v1/tenants/{}", tp.tenant_id);
    let projects_path = format!("/v1/tenants/{}/projects", tp.tenant_id);
    let project_path = format!("/v1/tenants/{}/projects/{}", tp.tenant_id, tp.project_id);

    // セッション経路: 両層の口が通る（require_scope はセッションを常に通す）
    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    assert_eq!(
        app.get_with_session(&tenant_path).await.status(),
        StatusCode::OK,
        "セッションは tenant 層の口を通る"
    );
    assert_eq!(
        app.get_with_session(&projects_path).await.status(),
        StatusCode::OK,
        "セッションは project 層の口を通る"
    );

    // admin:tenant 鍵: 両層の口が通る（既存 wildcard の対照）
    let tenant_key = insert_pat(&app, owner.id, tp.tenant_id, vec![Scope::AdminTenant], None).await;
    assert_eq!(
        app.get_with_bearer(&tenant_path, &tenant_key)
            .await
            .status(),
        StatusCode::OK,
        "admin:tenant 鍵は tenant 層の口を通る"
    );
    assert_eq!(
        app.get_with_bearer(&projects_path, &tenant_key)
            .await
            .status(),
        StatusCode::OK,
        "admin:tenant 鍵は project 層の口を通る"
    );

    // admin:project 鍵: project 層の口は通り、tenant 層の口は 403
    let project_key = insert_pat(
        &app,
        owner.id,
        tp.tenant_id,
        vec![Scope::AdminProject],
        None,
    )
    .await;
    assert_eq!(
        app.get_with_bearer(&projects_path, &project_key)
            .await
            .status(),
        StatusCode::OK,
        "admin:project 鍵は project 層の口（一覧）を通る"
    );
    assert_eq!(
        app.get_with_bearer(&project_path, &project_key)
            .await
            .status(),
        StatusCode::OK,
        "admin:project 鍵は project 層の口（名指し）を通る"
    );
    assert_eq!(
        app.get_with_bearer(&tenant_path, &project_key)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "admin:project 鍵は tenant 層の口を通らない"
    );

    // 列挙鍵（read:project）: project 層の該当する口は通り、tenant 層の口は 403
    let enumerated_key =
        insert_pat(&app, owner.id, tp.tenant_id, vec![Scope::ReadProject], None).await;
    assert_eq!(
        app.get_with_bearer(&projects_path, &enumerated_key)
            .await
            .status(),
        StatusCode::OK,
        "列挙鍵は自分のスコープの口を通る"
    );
    assert_eq!(
        app.get_with_bearer(&tenant_path, &enumerated_key)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "列挙鍵は tenant 層の口を通らない"
    );
}

#[tokio::test]
async fn admin_project_key_respects_allowed_project_ids_binding() {
    let app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let other_project_id = insert_second_project(&app.state.db, tp.tenant_id).await;

    let bound_key = insert_pat(
        &app,
        owner.id,
        tp.tenant_id,
        vec![Scope::AdminProject],
        Some(vec![tp.project_id]),
    )
    .await;

    let bound_path = format!("/v1/tenants/{}/projects/{}", tp.tenant_id, tp.project_id);
    let unbound_path = format!("/v1/tenants/{}/projects/{}", tp.tenant_id, other_project_id);
    let tenant_wide_path = format!("/v1/tenants/{}/projects", tp.tenant_id);

    assert_eq!(
        app.get_with_bearer(&bound_path, &bound_key).await.status(),
        StatusCode::OK,
        "束縛内の project は通る"
    );
    assert_eq!(
        app.get_with_bearer(&unbound_path, &bound_key)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "束縛外の project は 403"
    );
    assert_eq!(
        app.get_with_bearer(&tenant_wide_path, &bound_key)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "束縛つき鍵はテナント全体の口を通らない（既存規則の対照）"
    );
}
