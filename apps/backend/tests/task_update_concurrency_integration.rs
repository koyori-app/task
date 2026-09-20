mod common;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::put,
};
use common::{TestApp, TestTenantProject, TestUser};
use sea_orm::{ConnectOptions, ConnectionTrait, DatabaseBackend, Statement, TransactionTrait};
use serde_json::{Value, json};
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

async fn request(
    router: &Router,
    token: &str,
    method: &str,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn assignee_writes_reuse_the_transaction_connection() {
    let (app, tp, owner, task_id, path) = setup().await;
    let member = app.insert_user(false, false).await;
    let outsider = app.insert_user(false, false).await;
    let r = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/members", tp.tenant_id),
            json!({"user_id": member.id, "role": "Member"}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let r = app.post_json_with_session("/v1/personal_tokens", json!({"name": "single-connection", "tenant_id": tp.tenant_id, "scopes": ["read:task", "write:task"]})).await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let token: Value = r.json().await.unwrap();
    let token = token["token"].as_str().unwrap();
    let mut options = ConnectOptions::new(std::env::var("DATABASE_URL").unwrap());
    options
        .max_connections(1)
        .min_connections(1)
        .acquire_timeout(Duration::from_millis(300));
    let mut state = app.state.clone();
    state.db = sea_orm::Database::connect(options).await.unwrap();
    let router = Router::new()
        .route(
            "/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}",
            put(backend::handlers::tasks::update_task),
        )
        .route(
            "/v1/tenants/{tenant_id}/projects/{project_id}/tasks",
            axum::routing::post(backend::handlers::tasks::create_task),
        )
        .route(
            "/v1/tenants/{tenant_id}/projects/{project_id}/tasks/bulk",
            axum::routing::post(backend::handlers::task_extensions::bulk_update_tasks),
        )
        .with_state(state);
    let task_path = format!("{path}/tasks/{task_id}");
    // Exercise both the owner fast path and all membership checks with only one connection.
    for user_id in [owner.id, member.id] {
        let (status, task) = request(
            &router,
            token,
            "PUT",
            &task_path,
            json!({"title": "updated", "assignees": [{"user_id": user_id, "role": "assignee"}]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(task["title"], "updated");
        assert_eq!(task["assignees"].as_array().unwrap().len(), 1);
        assert_eq!(task["assignees"][0]["user"]["id"], user_id.to_string());
    }
    let (status, _) = request(
        &router,
        token,
        "PUT",
        &task_path,
        json!({"title": "rejected", "assignees": [{"user_id": outsider.id, "role": "assignee"}]}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let task: Value = app.get_with_session(&task_path).await.json().await.unwrap();
    assert_eq!(task["title"], "updated");
    assert_eq!(task["assignees"][0]["user"]["id"], member.id.to_string());
    let (status, cleared) =
        request(&router, token, "PUT", &task_path, json!({"assignees": []})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cleared["assignees"], json!([]));
    let (status, created) = request(&router, token, "POST", &format!("{path}/tasks"), json!({"title": "created", "status_id": task["status_id"], "assignees": [{"user_id": member.id, "role": "assignee"}]})).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["assignees"][0]["user"]["id"], member.id.to_string());
    let (status, bulk) = request(
        &router,
        token,
        "POST",
        &format!("{path}/tasks/bulk"),
        json!({"task_ids": [task_id], "update": {"assignee_id": member.id}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bulk["updated"], 1);
    assert_eq!(bulk["failed"], json!([]));
    let task: Value = app.get_with_session(&task_path).await.json().await.unwrap();
    assert_eq!(task["assignees"][0]["user"]["id"], member.id.to_string());
}

async fn setup() -> (TestApp, TestTenantProject, TestUser, Uuid, String) {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tp = app.insert_tenant_project(user.id).await;
    let path = format!("/v1/tenants/{}/projects/{}", tp.tenant_id, tp.project_id);
    let r = app
        .post_json_with_session(
            &format!("{path}/statuses"),
            json!({"name":"Todo","color":"#336699","position":0,"is_default":true}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let status: Value = r.json().await.unwrap();
    let r = app
        .post_json_with_session(
            &format!("{path}/tasks"),
            json!({"title":"original","status_id":status["id"]}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let task: Value = r.json().await.unwrap();
    (
        app,
        tp,
        user,
        task["id"].as_str().unwrap().parse().unwrap(),
        path,
    )
}
async fn wait_blocked(app: &TestApp, pid: i32, n: i64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let row = app
                .state
                .db
                .query_one_raw(Statement::from_sql_and_values(
                    DatabaseBackend::Postgres,
                    "WITH RECURSIVE blocked(pid) AS (
                    SELECT pid FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))
                    UNION
                    SELECT a.pid FROM pg_stat_activity a
                    JOIN blocked b ON b.pid = ANY(pg_blocking_pids(a.pid))
                ) SELECT count(*)::bigint AS n FROM blocked",
                    [pid.into()],
                ))
                .await
                .unwrap()
                .unwrap();
            if row.try_get::<i64>("", "n").unwrap() >= n {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("requests blocked on task row");
}

#[tokio::test]
async fn task_update_and_sprint_operations_use_the_same_lock_order() {
    for complete in [false, true] {
        let (app, _tp, _user, task_id, path) = setup().await;
        let r=app.post_json_with_session(&format!("{path}/sprints"),json!({"name":"planning","goal":"test","start_date":"2026-09-01","end_date":"2026-09-14"})).await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let sprint: Value = r.json().await.unwrap();
        let sprint_id = sprint["id"].as_str().unwrap().to_string();
        if complete {
            let r = app
                .post_json_with_session(&format!("{path}/sprints/{sprint_id}/start"), json!({}))
                .await;
            assert_eq!(r.status(), StatusCode::OK);
            let r = app
                .put_json_with_session(
                    &format!("{path}/tasks/{task_id}"),
                    json!({"sprint_id": sprint_id}),
                )
                .await;
            assert_eq!(r.status(), StatusCode::OK);
        }
        // Block the task so both requests overlap deterministically, including indirect waiters.
        let blocker = app.state.db.begin().await.unwrap();
        blocker
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT id FROM tasks WHERE id=$1 FOR UPDATE",
                [task_id.into()],
            ))
            .await
            .unwrap();
        let pid: i32 = blocker
            .query_one_raw(Statement::from_string(
                DatabaseBackend::Postgres,
                "SELECT pg_backend_pid() AS pid",
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get("", "pid")
            .unwrap();
        let client = app.session_client();
        let url = format!("{}{path}/tasks/{task_id}", app.base_url());
        let sid = sprint_id.clone();
        let first = tokio::spawn(async move {
            client
                .put(url)
                .json(&json!({"sprint_id":sid}))
                .send()
                .await
                .unwrap()
        });
        wait_blocked(&app, pid, 1).await;
        let client = app.session_client();
        let action = if complete { "complete" } else { "tasks" };
        let url = format!("{}{path}/sprints/{sprint_id}/{action}", app.base_url());
        let body = if complete {
            json!({})
        } else {
            json!({"task_ids": [task_id]})
        };
        let second =
            tokio::spawn(async move { client.post(url).json(&body).send().await.unwrap() });
        wait_blocked(&app, pid, 2).await;
        blocker.commit().await.unwrap();
        let (a, b) = tokio::time::timeout(Duration::from_secs(10), async {
            (first.await.unwrap(), second.await.unwrap())
        })
        .await
        .unwrap();
        assert_eq!(a.status(), StatusCode::OK);
        assert_eq!(b.status(), StatusCode::OK);
        let task: Value = app
            .get_with_session(&format!("{path}/tasks/{task_id}"))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(
            task["sprint_id"],
            if complete {
                Value::Null
            } else {
                json!(sprint_id)
            }
        );
        if complete {
            let r = app
                .put_json_with_session(
                    &format!("{path}/tasks/{task_id}"),
                    json!({"title": "rejected", "sprint_id": sprint_id}),
                )
                .await;
            assert_eq!(r.status(), StatusCode::CONFLICT);
            let task: Value = app
                .get_with_session(&format!("{path}/tasks/{task_id}"))
                .await
                .json()
                .await
                .unwrap();
            assert_eq!(task["title"], "original");
            // Clear takes precedence even when the supplied sprint is completed or missing.
            for sid in [sprint_id, Uuid::new_v4().to_string()] {
                let r = app
                    .put_json_with_session(
                        &format!("{path}/tasks/{task_id}"),
                        json!({"clear_sprint_id": true, "sprint_id": sid}),
                    )
                    .await;
                assert_eq!(r.status(), StatusCode::OK);
            }
        }
    }
}
