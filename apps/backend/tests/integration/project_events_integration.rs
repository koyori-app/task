use std::time::Duration;

use axum::http::StatusCode;
use reqwest::Response;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::common::{TestApp, TestTenantProject, TestUser, insert_extra_project, json_body};

// タスクの変更を購読者へ合図する `/events` の統合テスト。
// 規則は docs/dev/design/project-events.md。

const EVENT_TIMEOUT: Duration = Duration::from_secs(5);
/// 「合図が来ない」を確かめる待ち時間。Redis の配信は同じ機械の中で数ミリ秒で済む。
const SILENCE: Duration = Duration::from_millis(500);

fn project_path(tp: &TestTenantProject) -> String {
    format!("/v1/tenants/{}/projects/{}", tp.tenant_id, tp.project_id)
}

/// 所有者としてログインした状態のアプリとプロジェクトを作る。
async fn setup() -> (TestApp, TestUser, TestTenantProject) {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    (app, owner, tp)
}

async fn create_status(app: &TestApp, project_path: &str) -> Value {
    let response = app
        .post_json_with_session(
            &format!("{project_path}/statuses"),
            json!({ "name": "Backlog", "color": "#336699", "position": 0, "is_default": true }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    json_body(response).await
}

async fn create_task(app: &TestApp, project_path: &str, status: &Value) -> Uuid {
    let response = app
        .post_json_with_session(
            &format!("{project_path}/tasks"),
            json!({ "title": "t", "status_id": status["id"] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    json_body(response).await["id"]
        .as_str()
        .expect("task id")
        .parse()
        .expect("task uuid")
}

async fn connect(app: &TestApp, project_path: &str) -> Response {
    let stream = app
        .get_with_session(&format!("{project_path}/events"))
        .await;
    assert_eq!(stream.status(), StatusCode::OK);
    assert_eq!(
        stream.headers()["content-type"].to_str().unwrap(),
        "text/event-stream"
    );
    stream
}

/// 次の合図を読み、その本文を返す。キープアライブのコメントは読み飛ばす。
async fn next_event(stream: &mut Response) -> String {
    tokio::time::timeout(EVENT_TIMEOUT, async {
        loop {
            let chunk = stream
                .chunk()
                .await
                .expect("read stream")
                .expect("stream ended before an event");
            let text = String::from_utf8(chunk.to_vec()).expect("utf-8 event");
            if text.contains("event: tasks") {
                return text;
            }
        }
    })
    .await
    .expect("event did not arrive in time")
}

async fn assert_silent(stream: &mut Response) {
    if let Ok(chunk) = tokio::time::timeout(SILENCE, stream.chunk()).await {
        panic!("unexpected event: {chunk:?}");
    }
}

#[tokio::test]
async fn every_kind_of_task_change_notifies_without_task_contents() {
    let (app, owner, tp) = setup().await;
    let path = project_path(&tp);
    let status = create_status(&app, &path).await;
    let mut stream = connect(&app, &path).await;

    let response = app
        .post_json_with_session(
            &format!("{path}/tasks"),
            json!({ "title": "secret-title", "status_id": status["id"] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let task_id = json_body(response).await["id"]
        .as_str()
        .expect("task id")
        .to_string();
    let event = next_event(&mut stream).await;
    assert_eq!(
        event,
        "event: tasks\ndata: {\"kind\":\"tasks_changed\"}\n\n"
    );

    let task_path = format!("{path}/tasks/{task_id}");
    let response = app
        .put_json_with_session(&task_path, json!({ "title": "renamed" }))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    next_event(&mut stream).await;

    let response = app
        .post_json_with_session(
            &format!("{task_path}/assignees"),
            json!({ "user_id": owner.id, "role": "assignee" }),
        )
        .await;
    assert!(response.status().is_success(), "{}", response.status());
    next_event(&mut stream).await;

    let response = app
        .post_json_with_session(
            &format!("{path}/tasks/bulk"),
            json!({ "task_ids": [task_id], "update": { "priority": "high" } }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    next_event(&mut stream).await;

    let response = app
        .post_json_with_session(&format!("{task_path}/archive"), json!({}))
        .await;
    assert!(response.status().is_success(), "{}", response.status());
    next_event(&mut stream).await;

    let response = app.delete_with_session(&task_path).await;
    assert!(response.status().is_success(), "{}", response.status());
    next_event(&mut stream).await;
}

#[tokio::test]
async fn reads_failures_and_other_projects_do_not_notify() {
    let (app, _owner, tp) = setup().await;
    let path = project_path(&tp);
    let status = create_status(&app, &path).await;
    let mut stream = connect(&app, &path).await;

    // 読み取り
    let response = app.get_with_session(&format!("{path}/tasks")).await;
    assert_eq!(response.status(), StatusCode::OK);
    // 失敗した更新
    let response = app
        .put_json_with_session(
            &format!("{path}/tasks/{}", Uuid::new_v4()),
            json!({ "title": "x" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    // 別プロジェクトの更新
    let other_id = insert_extra_project(&app, tp.tenant_id).await;
    let other_path = format!("/v1/tenants/{}/projects/{other_id}", tp.tenant_id);
    let other_status = create_status(&app, &other_path).await;
    create_task(&app, &other_path, &other_status).await;
    assert_silent(&mut stream).await;

    // 対照: 同じ接続が生きていて、このプロジェクトの更新なら届く
    create_task(&app, &path, &status).await;
    next_event(&mut stream).await;
}

#[tokio::test]
async fn non_member_is_rejected() {
    let (mut app, _owner, tp) = setup().await;
    let path = project_path(&tp);
    // 対照: 所有者は通る
    connect(&app, &path).await;

    let outsider = app.insert_user_default().await;
    let other = app.insert_tenant_project(outsider.id).await;
    app.reset_session_client();
    app.login_session_no_content(&outsider.email, &outsider.password)
        .await;
    let response = app.get_with_session(&format!("{path}/events")).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // 自分のテナントの URL に他テナントのプロジェクトを入れても通らない
    let response = app
        .get_with_session(&format!(
            "/v1/tenants/{}/projects/{}/events",
            other.tenant_id, tp.project_id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // 未ログインは 401
    app.reset_session_client();
    let response = app.get_with_session(&format!("{path}/events")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn server_shutdown_closes_the_stream() {
    let (app, _owner, tp) = setup().await;
    let mut stream = connect(&app, &project_path(&tp)).await;

    app.shutdown_tx.send(true).expect("send shutdown");

    let closed = tokio::time::timeout(EVENT_TIMEOUT, async {
        while stream.chunk().await.expect("read stream").is_some() {}
    })
    .await;
    assert!(closed.is_ok(), "stream stayed open after shutdown");
}
