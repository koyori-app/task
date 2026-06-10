mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::Value;

#[tokio::test]
async fn custom_fields_integration_suite() {
    let mut app = TestApp::new().await;
    let user = app.insert_user(true, false).await;
    app.login_session_no_content(&user.email, &user.password).await;
    let tp = app.insert_tenant_project(user.id).await;

    let status_path = format!("/v1/tenants/{}/projects/{}/statuses", tp.tenant_id, tp.project_id);
    let status_resp = app.post_json_with_session(&status_path, serde_json::json!({
        "name": "Todo", "color": "#336699", "position": 0, "is_default": true
    })).await;
    assert_eq!(status_resp.status(), StatusCode::CREATED);
    let status_id = status_resp.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    let fields_base = format!("/v1/tenants/{}/projects/{}/custom-fields", tp.tenant_id, tp.project_id);
    let tasks_base = format!("/v1/tenants/{}/projects/{}/tasks", tp.tenant_id, tp.project_id);

    let number = app.post_json_with_session(&fields_base, serde_json::json!({
        "name": "Points", "field_type": "number", "is_required": true
    })).await;
    assert_eq!(number.status(), StatusCode::CREATED);
    let number_id = number.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    let select = app.post_json_with_session(&fields_base, serde_json::json!({
        "name": "Size", "field_type": "select",
        "options": [{"label": "M", "value": "m"}]
    })).await;
    assert_eq!(select.status(), StatusCode::CREATED);
    let select_id = select.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    let bad = app.post_json_with_session(&tasks_base, serde_json::json!({
        "title": "bad", "status_id": status_id,
        "custom_field_values": [{"field_id": select_id, "value": "xl"}]
    })).await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    let task = app.post_json_with_session(&tasks_base, serde_json::json!({
        "title": "ok", "status_id": status_id,
        "custom_field_values": [
            {"field_id": number_id, "value": "5"},
            {"field_id": select_id, "value": "m"}
        ]
    })).await;
    assert_eq!(task.status(), StatusCode::CREATED);
    let task_id = task.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    let get = app.get_with_session(&format!("{tasks_base}/{task_id}")).await;
    assert_eq!(get.status(), StatusCode::OK);
    assert!(get.json::<Value>().await.expect("json")["custom_field_values"].is_array());
}
