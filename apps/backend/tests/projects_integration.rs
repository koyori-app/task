mod common;

use axum::http::StatusCode;
use common::{TestApp, insert_tenant};

/// create_project が既定ステータスを seed することの回帰テスト（#368）。
/// 修正前は statuses が空配列で fail する。
#[tokio::test]
async fn create_project_seeds_default_statuses() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;

    let response = app
        .post_json_with_session(
            &format!("/v1/tenants/{tenant_id}/projects"),
            serde_json::json!({"name": "Seeded Project", "key": "SEED"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let project: serde_json::Value = response.json().await.unwrap();
    let project_id = project["id"].as_str().unwrap();

    let statuses: Vec<serde_json::Value> = app
        .get_with_session(&format!(
            "/v1/tenants/{tenant_id}/projects/{project_id}/statuses"
        ))
        .await
        .json()
        .await
        .unwrap();

    let names: Vec<&str> = statuses.iter().filter_map(|s| s["name"].as_str()).collect();
    assert_eq!(names, vec!["Backlog", "Todo", "In Progress", "Done"]);

    let default: Vec<&str> = statuses
        .iter()
        .filter(|s| s["is_default"] == true)
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(default, vec!["Todo"]);

    let done: Vec<&str> = statuses
        .iter()
        .filter(|s| s["is_done_state"] == true)
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(done, vec!["Done"]);
}
