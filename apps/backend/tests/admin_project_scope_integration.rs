mod common;

use axum::http::StatusCode;
use common::TestApp;
use entity::projects;
use entity::scopes::Scope;
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
    let tenant_key = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::AdminTenant], None)
        .await;
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
    let project_key = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::AdminProject], None)
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
    let enumerated_key = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadProject], None)
        .await;
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

    // 陰性対照: 同じ project 層でも、その口が要求するスコープを持たぬ鍵は通らぬ。
    // 所属も束縛も満たすオーナーの鍵ゆえ、403 の出所はスコープ判定だけである
    // （この対照が無いと、口から read:project の要求が消えても気づけない）
    let unrelated_key = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&projects_path, &unrelated_key)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "read:task だけの鍵は project の口を通らない"
    );
    assert_eq!(
        app.get_with_bearer(&project_path, &unrelated_key)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "名指しの取得も同じく通らない"
    );
}

#[tokio::test]
async fn admin_project_key_respects_allowed_project_ids_binding() {
    let app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let other_project_id = insert_second_project(&app.state.db, tp.tenant_id).await;

    let bound_key = app
        .insert_pat(
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

/// `write:project` は `read:project` を含意する（他の write/read 対と同じ扱い）。
///
/// 修正前は project だけがこの含意を欠き、書き込みを許した鍵で一覧が読めなかった。
/// project に PATCH の口は無いため、書き込み側の対照は作成（POST）で取る。
#[tokio::test]
async fn write_project_implies_read_project() {
    let app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;

    let write_only = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::WriteProject], None)
        .await;

    let projects_path = format!("/v1/tenants/{}/projects", tp.tenant_id);

    // 対照: 同じ鍵で書き込みの口は通る（鍵そのものは生きている）
    let created = app
        .post_json_with_bearer(
            &projects_path,
            serde_json::json!({
                "name": "write-only key",
                // project key の制約 ^[A-Z][A-Z0-9]{1,9}$ を満たす一意な値
                "key": format!("W{}", Uuid::new_v4().to_string()[..8].to_uppercase()),
            }),
            &write_only,
        )
        .await;
    assert_eq!(
        created.status(),
        StatusCode::CREATED,
        "write:project 鍵は作成の口を通る"
    );

    // 本題: 読みの口も通る
    assert_eq!(
        app.get_with_bearer(&projects_path, &write_only)
            .await
            .status(),
        StatusCode::OK,
        "write:project は read:project を含意するはず"
    );
    assert_eq!(
        app.get_with_bearer(
            &format!("/v1/tenants/{}/projects/{}", tp.tenant_id, tp.project_id),
            &write_only
        )
        .await
        .status(),
        StatusCode::OK,
        "名指しの取得も同じく通るはず"
    );
}
