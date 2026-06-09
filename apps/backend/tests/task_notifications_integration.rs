mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::Value;

#[tokio::test]
async fn task_notifications_integration_suite() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    app.login_session_no_content(&owner.email, &owner.password).await;
    let tp = app.insert_tenant_project(owner.id).await;

    let status_path = format!("/v1/tenants/{}/projects/{}/statuses", tp.tenant_id, tp.project_id);
    let status_resp = app.post_json_with_session(&status_path, serde_json::json!({"name":"Backlog","color":"#336699","position":0,"is_default":true})).await;
    assert_eq!(status_resp.status(), StatusCode::CREATED);
    let status_id = status_resp.json::<serde_json::Value>().await.expect("json")["id"].as_str().unwrap().to_string();

    let tasks_path = format!("/v1/tenants/{}/projects/{}/tasks", tp.tenant_id, tp.project_id);
    let task_resp = app.post_json_with_session(&tasks_path, serde_json::json!({"title":"Notify task","status_id":status_id})).await;
    assert_eq!(task_resp.status(), StatusCode::CREATED);
    let task_id = task_resp.json::<serde_json::Value>().await.expect("json")["id"].as_str().unwrap().to_string();

    let assignee = app.insert_user(false, false).await;
    let assignee_username = format!("test_{}", &assignee.id.to_string()[..8]);

    let member_resp = app.post_json_with_session(
        &format!("/v1/tenants/{}/projects/{}/members", tp.tenant_id, tp.project_id),
        serde_json::json!({"user_id": assignee.id, "role": "Member"}),
    ).await;
    assert_eq!(member_resp.status(), StatusCode::CREATED);

    let task_base = format!("/v1/tenants/{}/projects/{}/tasks/{}", tp.tenant_id, tp.project_id, task_id);
    let assign_resp = app.post_json_with_session(&format!("{task_base}/assignees"), serde_json::json!({"user_id": assignee.id, "role": "primary"})).await;
    assert_eq!(assign_resp.status(), StatusCode::CREATED);

    let watchers = app.get_with_session(&format!("{task_base}/watchers")).await;
    assert_eq!(watchers.status(), StatusCode::OK);
    assert_eq!(watchers.json::<serde_json::Value>().await.expect("json")["watchers"].as_array().unwrap().len(), 1);

    app.reset_session_client();
    app.login_session_no_content(&assignee.email, &assignee.password).await;
    let notif = app.get_with_session("/v1/users/me/notifications").await;
    assert_eq!(notif.status(), StatusCode::OK);
    let body: Value = notif.json::<serde_json::Value>().await.expect("json");
    assert_eq!(body["unread_count"].as_u64(), Some(1));

    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password).await;
    assert_eq!(app.post_json_with_session(&format!("{task_base}/watch"), serde_json::json!({})).await.status(), StatusCode::CREATED);
    let comment = app.post_json_with_session(&format!("{task_base}/comments"), serde_json::json!({"body": format!("@{assignee_username} review")})).await;
    assert_eq!(comment.status(), StatusCode::CREATED);

    app.reset_session_client();
    app.login_session_no_content(&assignee.email, &assignee.password).await;
    let unread = app.get_with_session("/v1/users/me/notifications?unread=true").await;
    let unread_body: Value = unread.json::<Value>().await.expect("json");
    let types: Vec<&str> = unread_body["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["notification_type"].as_str())
        .collect();
    assert!(types.contains(&"mentioned"));
}
