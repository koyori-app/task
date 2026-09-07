mod common;

use axum::http::StatusCode;
use common::TestApp;
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
