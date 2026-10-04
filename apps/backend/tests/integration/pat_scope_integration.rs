use crate::common::TestApp;
use axum::http::StatusCode;
use entity::scopes::Scope;
use entity::{personal_tokens, projects};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter,
};
use uuid::Uuid;

/// PAT のスコープ判定（`api` / `read_api` / 動詞ごとのスコープ）の固定。
///
/// ここで固定する契約（apps/backend/docs/personal-access-tokens-authz.md の含意の規則）:
/// 1. `api` は tenant 層の口（テナント取得 = `read:tenant`）も project 層の読み書きも通る
/// 2. `read_api` は読みの口だけ通り、書き込みの口（プロジェクト作成 = `write:project`）は 403
/// 3. `read:tenant` だけの鍵は tenant 層の読みを通り、project 層の口は 403
/// 4. 旧 `admin:project` を展開した project 層の write:* は、tenant 層の口を通らない
/// 5. `allowed_project_ids` 束縛は `api` でも効く（束縛外とテナント全体の口は 403）
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

fn new_project_body() -> serde_json::Value {
    serde_json::json!({
        "name": "scope-check",
        // project key の制約 ^[A-Z][A-Z0-9]{1,9}$ を満たす一意な値
        "key": format!("P{}", Uuid::new_v4().to_string()[..8].to_uppercase()),
    })
}

#[tokio::test]
async fn api_read_api_and_verb_scopes_for_session_and_pat() {
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

    // api: tenant 層・project 層の読み書きが通る
    let api_key = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::Api], None)
        .await;
    for path in [&tenant_path, &projects_path, &project_path] {
        assert_eq!(
            app.get_with_bearer(path, &api_key).await.status(),
            StatusCode::OK,
            "api 鍵は {path} を通る"
        );
    }
    assert_eq!(
        app.post_json_with_bearer(&projects_path, new_project_body(), &api_key)
            .await
            .status(),
        StatusCode::CREATED,
        "api 鍵は書き込みの口を通る"
    );

    // read_api: 読みの口は通り、書き込みの口は 403
    let read_api_key = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadApi], None)
        .await;
    for path in [&tenant_path, &projects_path, &project_path] {
        assert_eq!(
            app.get_with_bearer(path, &read_api_key).await.status(),
            StatusCode::OK,
            "read_api 鍵は {path} を通る"
        );
    }
    assert_eq!(
        app.post_json_with_bearer(&projects_path, new_project_body(), &read_api_key)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "read_api 鍵は書き込みの口を通らない"
    );

    // read:tenant だけ: tenant 層の読みは通り、project 層の口は 403
    let tenant_reader = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadTenant], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&tenant_path, &tenant_reader)
            .await
            .status(),
        StatusCode::OK,
        "read:tenant 鍵はテナント取得を通る"
    );
    assert_eq!(
        app.get_with_bearer(&projects_path, &tenant_reader)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "read:tenant 鍵は project 層の口を通らない"
    );

    // 旧 admin:project を展開した鍵（project 層の write:*）: project 層は通り、tenant 層は 403
    let project_writer = app
        .insert_pat(owner.id, tp.tenant_id, project_layer_writes(), None)
        .await;
    assert_eq!(
        app.get_with_bearer(&project_path, &project_writer)
            .await
            .status(),
        StatusCode::OK,
        "project 層の write:* 鍵は project 層の口を通る"
    );
    assert_eq!(
        app.get_with_bearer(&tenant_path, &project_writer)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "project 層の write:* 鍵は tenant 層の口を通らない"
    );

    // 陰性対照: その口が要求するスコープを持たない鍵は通らない。
    // 所属も束縛も満たすオーナーの鍵なので、403 の出所はスコープ判定だけ
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
        app.get_with_bearer(&tenant_path, &unrelated_key)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "read:task だけの鍵は tenant 層の口を通らない"
    );
}

/// 旧 `admin:project` と同じ範囲（project 層の write:*。write は対の read を含む）。
fn project_layer_writes() -> Vec<Scope> {
    vec![
        Scope::WriteProject,
        Scope::WriteDrive,
        Scope::WriteTask,
        Scope::WriteMilestone,
        Scope::WriteSprint,
        Scope::WriteReview,
    ]
}

#[tokio::test]
async fn api_key_respects_allowed_project_ids_binding() {
    let app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let other_project_id = insert_second_project(&app.state.db, tp.tenant_id).await;

    let bound_key = app
        .insert_pat(
            owner.id,
            tp.tenant_id,
            vec![Scope::Api],
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

/// 既存トークンの `admin:tenant` / `admin:project` を移すマイグレーション
/// （`m20261004000000_pat_scopes_api`）の SQL。テストハーネスはマイグレーションを流さないので、
/// 旧い値を直接書いてから SQL 本体を当てる（`drive_project_id_backfill_integration` と同じ手）。
const MIGRATE_SQL: &str = include_str!("../../sql/migrate_pat_scopes_api.sql");

async fn set_raw_scopes(app: &TestApp, token: &str, scopes: &str) {
    app.state
        .db
        .execute_unprepared(&format!(
            "UPDATE personal_tokens SET scopes = '{scopes}'::jsonb WHERE token_hash = '{}'",
            backend::utils::auth::create_personal_token_hash(
                token,
                &app.state.settings.personal_token_secret
            )
            .expect("hash token")
        ))
        .await
        .expect("write raw scopes");
}

async fn stored_scopes(app: &TestApp, token: &str) -> Vec<Scope> {
    let hash = backend::utils::auth::create_personal_token_hash(
        token,
        &app.state.settings.personal_token_secret,
    )
    .expect("hash token");
    personal_tokens::Entity::find()
        .filter(personal_tokens::Column::TokenHash.eq(hash))
        .one(&app.state.db)
        .await
        .expect("find token")
        .expect("token exists")
        .scopes
        .0
}

#[tokio::test]
async fn migration_moves_retired_admin_scopes_without_widening_them() {
    let app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let tenant_path = format!("/v1/tenants/{}", tp.tenant_id);
    let project_path = format!("/v1/tenants/{}/projects/{}", tp.tenant_id, tp.project_id);

    let was_tenant = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let was_project = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let untouched = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    set_raw_scopes(&app, &was_tenant, r#"["admin:tenant", "read:task"]"#).await;
    set_raw_scopes(&app, &was_project, r#"["admin:project", "write:task"]"#).await;

    // 2 回流しても同じ結果になる
    app.state
        .db
        .execute_unprepared(MIGRATE_SQL)
        .await
        .expect("migrate scopes");
    app.state
        .db
        .execute_unprepared(MIGRATE_SQL)
        .await
        .expect("migrate scopes again");

    let mut tenant_scopes = stored_scopes(&app, &was_tenant).await;
    tenant_scopes.sort_by_key(|s| s.as_str());
    assert_eq!(tenant_scopes, vec![Scope::Api, Scope::ReadTask]);

    let mut project_scopes = stored_scopes(&app, &was_project).await;
    project_scopes.sort_by_key(|s| s.as_str());
    let mut expected = project_layer_writes();
    expected.sort_by_key(|s| s.as_str());
    assert_eq!(
        project_scopes, expected,
        "重複した write:task は 1 つにまとまる"
    );

    assert_eq!(stored_scopes(&app, &untouched).await, vec![Scope::ReadTask]);

    // 移した後の鍵の通り方: 旧 admin:tenant は tenant 層も通り、旧 admin:project は通らない
    assert_eq!(
        app.get_with_bearer(&tenant_path, &was_tenant)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.get_with_bearer(&project_path, &was_project)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.get_with_bearer(&tenant_path, &was_project)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "旧 admin:project は移行後もテナント層まで広がらない"
    );
}
