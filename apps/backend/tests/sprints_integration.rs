mod common;

use axum::http::StatusCode;
use common::{TestApp, TestTenantProject, TestUser};
use sea_orm::EntityTrait;
use uuid::Uuid;

fn sprints_base(tp: &TestTenantProject) -> String {
    format!(
        "/v1/tenants/{}/projects/{}/sprints",
        tp.tenant_id, tp.project_id
    )
}

async fn setup_project(app: &mut TestApp) -> (TestUser, TestTenantProject) {
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tp = app.insert_tenant_project(user.id).await;
    (user, tp)
}

async fn create_status(
    app: &TestApp,
    tp: &TestTenantProject,
    name: &str,
    is_done: bool,
) -> Uuid {
    let path = format!(
        "/v1/tenants/{}/projects/{}/statuses",
        tp.tenant_id, tp.project_id
    );
    let response = app
        .post_json_with_session(
            &path,
            serde_json::json!({
                "name": name,
                "color": "#336699",
                "position": if is_done { 2 } else { 1 },
                "is_default": name == "Todo",
                "is_done_state": is_done,
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED, "create status {name}");
    let body: serde_json::Value = response.json().await.expect("status json");
    body["id"]
        .as_str()
        .expect("status id")
        .parse()
        .expect("uuid")
}

async fn create_sprint(app: &TestApp, tp: &TestTenantProject, name: &str) -> Uuid {
    let response = app
        .post_json_with_session(
            &sprints_base(tp),
            serde_json::json!({
                "name": name,
                "goal": "ship it",
                "start_date": "2026-06-01",
                "end_date": "2026-06-14",
            }),
        )
        .await;
    let status = response.status();
    let text = response.text().await.expect("sprint body");
    assert_eq!(
        status,
        StatusCode::CREATED,
        "create sprint failed: {text}"
    );
    let body: serde_json::Value = serde_json::from_str(&text).expect("sprint json");
    assert_eq!(body["status"].as_str(), Some("planning"));
    body["id"]
        .as_str()
        .expect("sprint id")
        .parse()
        .expect("uuid")
}

async fn create_task(
    app: &TestApp,
    tp: &TestTenantProject,
    status_id: Uuid,
    title: &str,
) -> Uuid {
    let path = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let response = app
        .post_json_with_session(
            &path,
            serde_json::json!({
                "title": title,
                "status_id": status_id,
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: serde_json::Value = response.json().await.expect("task json");
    body["id"]
        .as_str()
        .expect("task id")
        .parse()
        .expect("uuid")
}

#[tokio::test]
async fn sprints_integration_suite() {
    let mut app = TestApp::new().await;
    let (user, tp) = setup_project(&mut app).await;
    let todo_status = create_status(&app, &tp, "Todo", false).await;
    let done_status = create_status(&app, &tp, "Done", true).await;

    let sprint_a = create_sprint(&app, &tp, "Sprint A").await;
    let sprint_b = create_sprint(&app, &tp, "Sprint B").await;

    let start_path = format!("{}/{}/start", sprints_base(&tp), sprint_a);
    let start_resp = app
        .post_json_with_session(&start_path, serde_json::json!({}))
        .await;
    assert_eq!(start_resp.status(), StatusCode::OK);
    let started: serde_json::Value = start_resp.json().await.expect("started json");
    assert_eq!(started["status"].as_str(), Some("active"));

    // second active sprint should conflict
    let start_b_path = format!("{}/{}/start", sprints_base(&tp), sprint_b);
    let conflict = app
        .post_json_with_session(&start_b_path, serde_json::json!({}))
        .await;
    assert_eq!(conflict.status(), StatusCode::CONFLICT);

    // assign tasks + burndown detail
    let task1 = create_task(&app, &tp, todo_status, "Task 1").await;
    let task2 = create_task(&app, &tp, todo_status, "Task 2").await;
    let assign_path = format!("{}/{}/tasks", sprints_base(&tp), sprint_a);
    let assign = app
        .post_json_with_session(
            &assign_path,
            serde_json::json!({ "task_ids": [task1, task2] }),
        )
        .await;
    assert_eq!(assign.status(), StatusCode::OK);
    let assigned: Vec<serde_json::Value> = assign.json().await.expect("assigned json");
    assert_eq!(assigned.len(), 2);

    let detail = app
        .get_with_session(&format!("{}/{}", sprints_base(&tp), sprint_a))
        .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_body: serde_json::Value = detail.json().await.expect("detail json");
    assert_eq!(detail_body["task_counts"]["total"].as_u64(), Some(2));
    let burndown = detail_body["burndown"]
        .as_array()
        .expect("burndown array");
    assert!(!burndown.is_empty());
    let first = &burndown[0];
    assert_eq!(first["ideal_remaining"].as_i64(), Some(2));
    let last = burndown.last().expect("last burndown point");
    assert_eq!(last["actual_remaining"].as_u64(), Some(2));

    // mark one task done and verify counts
    let task_path = format!(
        "/v1/tenants/{}/projects/{}/tasks/{}",
        tp.tenant_id, tp.project_id, task1
    );
    let update = app
        .client()
        .put(format!("{}{}", app.base_url(), task_path))
        .json(&serde_json::json!({ "status_id": done_status }))
        .send()
        .await
        .expect("put task");
    assert_eq!(update.status(), StatusCode::OK);

    let detail2 = app
        .get_with_session(&format!("{}/{}", sprints_base(&tp), sprint_a))
        .await;
    let detail2_body: serde_json::Value = detail2.json().await.expect("detail2 json");
    assert_eq!(detail2_body["task_counts"]["done"].as_u64(), Some(1));
    assert_eq!(detail2_body["task_counts"]["in_progress"].as_u64(), Some(1));

    // complete sprint moves incomplete to backlog by default
    let complete_path = format!("{}/{}/complete", sprints_base(&tp), sprint_a);
    let complete = app
        .post_json_with_session(&complete_path, serde_json::json!({}))
        .await;
    assert_eq!(complete.status(), StatusCode::OK);

    use backend::entities::tasks;
    let task2_row = tasks::Entity::find_by_id(task2)
        .one(&app.state.db)
        .await
        .expect("load task2")
        .expect("task2 exists");
    assert!(task2_row.sprint_id.is_none());

    // permission: non-member cannot create sprint
    let outsider = app.insert_user_default().await;
    app.login_session_no_content(&outsider.email, &outsider.password)
        .await;
    let forbidden = app
        .post_json_with_session(
            &sprints_base(&tp),
            serde_json::json!({
                "name": "Sprint C",
                "start_date": "2026-06-01",
                "end_date": "2026-06-14",
            }),
        )
        .await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    app.login_session_no_content(&user.email, &user.password)
        .await;

    // list filter by status
    let list = app
        .get_with_session(&format!("{}?status=completed", sprints_base(&tp)))
        .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_body: Vec<serde_json::Value> = list.json().await.expect("list json");
    assert!(list_body.iter().any(|s| s["id"].as_str() == Some(sprint_a.to_string().as_str())));
}
