use crate::common::{TestApp, TestTenantProject, TestUser, ensure_tenant_member_for_project};
use axum::http::StatusCode;
use uuid::Uuid;

#[tokio::test]
async fn updated_tenant_emoji_is_returned_in_the_tenant_list() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let response = app
        .put_json_with_session(
            &format!("/v1/tenants/{}", tp.tenant_id),
            serde_json::json!({ "icon_emoji": "🦊" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = app.get_with_session("/v1/tenants").await;
    assert_eq!(response.status(), StatusCode::OK);
    let tenants: Vec<serde_json::Value> = response.json().await.unwrap();
    let tenant = tenants
        .iter()
        .find(|tenant| tenant["id"] == tp.tenant_id.to_string())
        .unwrap();
    assert_eq!(tenant["icon_emoji"], "🦊");
    assert_eq!(tenant["membership"], "Owner");
}

/// 一覧から対象のテナントの `icon_emoji` を読む。
async fn tenant_emoji_in_list(app: &TestApp, tenant_id: Uuid) -> serde_json::Value {
    let response = app.get_with_session("/v1/tenants").await;
    assert_eq!(response.status(), StatusCode::OK);
    let tenants: Vec<serde_json::Value> = response.json().await.unwrap();
    tenants
        .iter()
        .find(|tenant| tenant["id"] == tenant_id.to_string())
        .expect("tenant is listed")["icon_emoji"]
        .clone()
}

/// オーナーでログインし、絵文字 🦊 を持つテナントを用意する。
async fn app_with_fox_tenant() -> (TestApp, TestUser, TestTenantProject) {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let response = app
        .put_json_with_session(
            &format!("/v1/tenants/{}", tp.tenant_id),
            serde_json::json!({ "icon_emoji": "🦊" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    (app, owner, tp)
}

#[tokio::test]
async fn clear_icon_emoji_removes_the_tenant_emoji() {
    let (app, _owner, tp) = app_with_fox_tenant().await;
    let response = app
        .put_json_with_session(
            &format!("/v1/tenants/{}", tp.tenant_id),
            serde_json::json!({ "clear_icon_emoji": true }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["icon_emoji"], serde_json::Value::Null);
    assert_eq!(
        tenant_emoji_in_list(&app, tp.tenant_id).await,
        serde_json::Value::Null
    );
}

/// `clear_icon_emoji: true` は同じ要求の `icon_emoji` より優先する。
#[tokio::test]
async fn clear_icon_emoji_wins_over_icon_emoji() {
    let (app, _owner, tp) = app_with_fox_tenant().await;
    let response = app
        .put_json_with_session(
            &format!("/v1/tenants/{}", tp.tenant_id),
            serde_json::json!({ "icon_emoji": "🐱", "clear_icon_emoji": true }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        tenant_emoji_in_list(&app, tp.tenant_id).await,
        serde_json::Value::Null
    );
}

/// 対照: `clear_icon_emoji` を省いた更新は絵文字に触れない。
#[tokio::test]
async fn update_without_clear_keeps_the_tenant_emoji() {
    let (app, _owner, tp) = app_with_fox_tenant().await;
    let response = app
        .put_json_with_session(
            &format!("/v1/tenants/{}", tp.tenant_id),
            serde_json::json!({ "name": "Renamed" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["name"], "Renamed");
    assert_eq!(tenant_emoji_in_list(&app, tp.tenant_id).await, "🦊");
}

/// 拒否: オーナーでないメンバーは絵文字を外せない。
#[tokio::test]
async fn non_owner_cannot_clear_the_tenant_emoji() {
    let (mut app, owner, tp) = app_with_fox_tenant().await;
    let member = app.insert_user_default().await;
    ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;

    app.reset_session_client();
    app.login_session_no_content(&member.email, &member.password)
        .await;
    let response = app
        .put_json_with_session(
            &format!("/v1/tenants/{}", tp.tenant_id),
            serde_json::json!({ "clear_icon_emoji": true }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    assert_eq!(tenant_emoji_in_list(&app, tp.tenant_id).await, "🦊");
}

/// `POST /v1/tenants` が `display_id` 重複で 409 Conflict を返すこと。
///
/// この回帰テストは #336 で OpenAPI に 409 を宣言する前提となる
/// 実行時契約を固定する。`CrudErrors` に 409 が無くても実行時は 409 が
/// 返っていたが、 spec に明示されない状態ではフロントが生ステータス比較に
/// 頼るしかなく、契約が型システムで追跡できなかった。
#[tokio::test]
async fn create_tenant_duplicate_display_id_returns_409() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;

    let display_id = format!("dup-{}", &Uuid::new_v4().to_string()[..8]);
    let body = serde_json::json!({
        "display_id": display_id,
        "name": "first tenant",
        "description": "",
        "icon_url": "",
    });

    // 1 件目は 201 で作成される
    let first = app
        .post_json_with_session("/v1/tenants", body.clone())
        .await;
    assert_eq!(
        first.status(),
        StatusCode::CREATED,
        "first create should succeed"
    );

    // 2 件目は display_id 重複で 409
    let second = app.post_json_with_session("/v1/tenants", body).await;
    assert_eq!(
        second.status(),
        StatusCode::CONFLICT,
        "duplicate display_id must return 409 Conflict"
    );

    app.cleanup_user(owner.id).await;
}
