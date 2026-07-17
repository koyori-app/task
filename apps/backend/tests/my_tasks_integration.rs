mod common;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use common::{TestApp, TestTenantProject, TestUser};
use uuid::Uuid;

async fn setup(app: &mut TestApp) -> (TestUser, TestTenantProject) {
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tp = app.insert_tenant_project(user.id).await;
    (user, tp)
}

fn my_tasks_base(tenant_id: Uuid) -> String {
    format!("/v1/tenants/{tenant_id}/users/me")
}

async fn create_status(app: &TestApp, tp: &TestTenantProject, name: &str, is_done: bool) -> Uuid {
    let path = format!(
        "/v1/tenants/{}/projects/{}/statuses",
        tp.tenant_id, tp.project_id
    );
    let response = app
        .post_json_with_session(
            &path,
            serde_json::json!({
                "name": name, "color": "#336699",
                "position": if is_done { 2 } else { 1 },
                "is_default": name == "Todo", "is_done_state": is_done,
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn personal_project_is_idempotent() {
    let mut app = TestApp::new().await;
    let (_user, tp) = setup(&mut app).await;
    let base = my_tasks_base(tp.tenant_id);
    let first: Uuid = app
        .get_with_session(&format!("{base}/personal-project"))
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let second: Uuid = app
        .get_with_session(&format!("{base}/personal-project"))
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(first, second);
}

/// quick-capture API 撤去の回帰テスト（#363）。
/// 撤去前は 201 CREATED を返していたため、撤去前のコードでは fail する。
#[tokio::test]
async fn quick_capture_endpoint_is_removed() {
    let mut app = TestApp::new().await;
    let (_user, tp) = setup(&mut app).await;
    let base = my_tasks_base(tp.tenant_id);
    assert_eq!(
        app.post_json_with_session(
            &format!("{base}/tasks"),
            serde_json::json!({"title": "Buy milk"})
        )
        .await
        .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn list_returns_only_assigned_tasks() {
    let mut app = TestApp::new().await;
    let (user, tp) = setup(&mut app).await;
    let status_id = create_status(&app, &tp, "Todo", false).await;
    let path = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let assignee = serde_json::json!([{"user_id": user.id, "role": "assignee"}]);
    // 成功系: 自分に割り当てられたタスクは一覧に載る
    assert_eq!(
        app.post_json_with_session(
            &path,
            serde_json::json!({"title": "Assigned", "status_id": status_id, "assignees": assignee}),
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    // 対照: 未割り当てのタスクは載らない
    assert_eq!(
        app.post_json_with_session(
            &path,
            serde_json::json!({"title": "Unassigned", "status_id": status_id}),
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    let body: serde_json::Value = app
        .get_with_session(&format!("{}/tasks?filter=all", my_tasks_base(tp.tenant_id)))
        .await
        .json()
        .await
        .unwrap();
    let titles: Vec<&str> = body["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["title"].as_str())
        .collect();
    assert!(titles.contains(&"Assigned"));
    assert!(!titles.contains(&"Unassigned"));
    assert_eq!(body["total"], 1);
}

#[tokio::test]
async fn list_projects_excludes_personal() {
    let mut app = TestApp::new().await;
    let (_user, tp) = setup(&mut app).await;
    app.get_with_session(&format!("{}/personal-project", my_tasks_base(tp.tenant_id)))
        .await;
    let projects: Vec<serde_json::Value> = app
        .get_with_session(&format!("/v1/tenants/{}/projects", tp.tenant_id))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(projects.len(), 1);
    assert_ne!(projects[0]["is_personal"].as_bool(), Some(true));
}

#[tokio::test]
async fn filter_overdue() {
    let mut app = TestApp::new().await;
    let (user, tp) = setup(&mut app).await;
    let status_id = create_status(&app, &tp, "Todo", false).await;
    let done_id = create_status(&app, &tp, "Done", true).await;
    let overdue = (Utc::now().date_naive() - Duration::days(2))
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc();
    let path = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let assignee = serde_json::json!([{"user_id": user.id, "role": "assignee"}]);
    app.post_json_with_session(&path, serde_json::json!({"title":"Overdue","status_id":status_id,"hard_deadline":overdue,"assignees":assignee})).await;
    app.post_json_with_session(&path, serde_json::json!({"title":"Done overdue","status_id":done_id,"hard_deadline":overdue,"assignees":assignee})).await;
    let titles: Vec<String> = app
        .get_with_session(&format!(
            "{}/tasks?filter=overdue",
            my_tasks_base(tp.tenant_id)
        ))
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["title"].as_str().map(str::to_string))
        .collect();
    assert!(titles.contains(&"Overdue".to_string()));
    assert!(!titles.contains(&"Done overdue".to_string()));
}
