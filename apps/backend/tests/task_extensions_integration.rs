mod common;

use axum::http::StatusCode;
use common::TestApp;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::Value;
use uuid::Uuid;

use backend::entities::drive_files;

struct TaskFixture {
    tenant_id: Uuid,
    project_id: Uuid,
    task_id: String,
    status_done_id: String,
    label_id: String,
}

async fn setup_task(app: &mut TestApp) -> TaskFixture {
    let user = app.insert_user(true, false).await;
    app.login_session_no_content(&user.email, &user.password).await;
    let tp = app.insert_tenant_project(user.id).await;

    let status_path = format!(
        "/v1/tenants/{}/projects/{}/statuses",
        tp.tenant_id, tp.project_id
    );
    let backlog = app
        .post_json_with_session(
            &status_path,
            serde_json::json!({
                "name": "Backlog",
                "color": "#336699",
                "position": 0,
                "is_default": true
            }),
        )
        .await;
    assert_eq!(backlog.status(), StatusCode::CREATED);
    let backlog_body: Value = backlog.json().await.expect("backlog json");
    let backlog_id = backlog_body["id"].as_str().expect("backlog id");

    let done = app
        .post_json_with_session(
            &status_path,
            serde_json::json!({
                "name": "Done",
                "color": "#00aa00",
                "position": 1,
                "is_done_state": true
            }),
        )
        .await;
    assert_eq!(done.status(), StatusCode::CREATED);
    let done_body: Value = done.json().await.expect("done json");
    let done_id = done_body["id"].as_str().expect("done id").to_string();

    let labels_path = format!(
        "/v1/tenants/{}/projects/{}/labels",
        tp.tenant_id, tp.project_id
    );
    let label_resp = app
        .post_json_with_session(
            &labels_path,
            serde_json::json!({
                "name": "extension-label",
                "color": "#ff0000"
            }),
        )
        .await;
    assert_eq!(label_resp.status(), StatusCode::CREATED);
    let label_body: Value = label_resp.json().await.expect("label json");
    let label_id = label_body["id"].as_str().expect("label id").to_string();

    let tasks_path = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let task_resp = app
        .post_json_with_session(
            &tasks_path,
            serde_json::json!({
                "title": "OAuth integration task",
                "description": "全文検索テスト用の説明文",
                "status_id": backlog_id
            }),
        )
        .await;
    assert_eq!(task_resp.status(), StatusCode::CREATED);
    let task: Value = task_resp.json().await.expect("task json");
    let task_id = task["id"].as_str().expect("task id").to_string();

    TaskFixture {
        tenant_id: tp.tenant_id,
        project_id: tp.project_id,
        task_id,
        status_done_id: done_id,
        label_id,
    }
}

async fn insert_drive_file(app: &TestApp, tenant_id: Uuid, uploader_id: Uuid) -> Uuid {
    let file_id = Uuid::new_v4();
    drive_files::ActiveModel {
        id: Set(file_id),
        name: Set("spec.pdf".into()),
        size: Set(1024),
        mime_type: Set("application/pdf".into()),
        storage_type: Set(drive_files::StorageType::Local),
        storage_key: Set(format!("test/{file_id}")),
        tenant_id: Set(tenant_id),
        project_id: Set(None),
        uploader_id: Set(uploader_id),
        folder_id: Set(None),
        ..Default::default()
    }
    .insert(&app.state.db)
    .await
    .expect("insert drive file");
    file_id
}

#[tokio::test]
async fn task_extensions_integration_suite() {
    let mut app = TestApp::new().await;
    let fx = setup_task(&mut app).await;

    let search_path = format!(
        "/v1/tenants/{}/projects/{}/tasks/search?q=OAuth",
        fx.tenant_id, fx.project_id
    );
    let search = app.get_with_session(&search_path).await;
    assert_eq!(search.status(), StatusCode::OK);
    let search_body: Value = search.json().await.expect("search json");
    assert!(search_body["total"].as_u64().unwrap_or(0) >= 1);

    let views_base = format!(
        "/v1/tenants/{}/projects/{}/task-views",
        fx.tenant_id, fx.project_id
    );
    let create_view = app
        .post_json_with_session(
            &views_base,
            serde_json::json!({
                "name": "High priority open",
                "is_shared": true,
                "filters": { "priority": ["high"] }
            }),
        )
        .await;
    assert_eq!(create_view.status(), StatusCode::CREATED);
    let view: Value = create_view.json().await.expect("view json");
    let view_id = view["id"].as_str().expect("view id");

    let task_uuid = Uuid::parse_str(&fx.task_id).expect("task uuid");
    let bulk_path = format!(
        "/v1/tenants/{}/projects/{}/tasks/bulk",
        fx.tenant_id, fx.project_id
    );
    let bulk = app
        .post_json_with_session(
            &bulk_path,
            serde_json::json!({
                "task_ids": [task_uuid],
                "update": {
                    "status_id": fx.status_done_id,
                    "label_ids": [fx.label_id]
                }
            }),
        )
        .await;
    assert_eq!(bulk.status(), StatusCode::OK);
    let bulk_body: Value = bulk.json().await.expect("bulk json");
    assert_eq!(bulk_body["updated"], 1);

    let over_limit: Vec<Uuid> = (0..101).map(|_| Uuid::new_v4()).collect();
    let bulk_too_many = app
        .post_json_with_session(
            &bulk_path,
            serde_json::json!({
                "task_ids": over_limit,
                "update": { "status_id": fx.status_done_id }
            }),
        )
        .await;
    assert_eq!(bulk_too_many.status(), StatusCode::BAD_REQUEST);

    let user = app.insert_user(false, false).await;
    let file_id = insert_drive_file(&app, fx.tenant_id, user.id).await;
    let attachments_path = format!(
        "/v1/tenants/{}/projects/{}/tasks/{}/attachments",
        fx.tenant_id, fx.project_id, fx.task_id
    );
    let attach = app
        .post_json_with_session(
            &attachments_path,
            serde_json::json!({ "drive_file_id": file_id }),
        )
        .await;
    assert_eq!(attach.status(), StatusCode::CREATED);
    let attach_body: Value = attach.json().await.expect("attach json");
    let attachment_id = attach_body["id"].as_str().expect("attachment id");

    let detach = app
        .delete_with_session(&format!("{attachments_path}/{attachment_id}"))
        .await;
    assert_eq!(detach.status(), StatusCode::NO_CONTENT);

    let delete_view = app
        .delete_with_session(&format!("{views_base}/{view_id}"))
        .await;
    assert_eq!(delete_view.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn task_extensions_negative_cases() {
    let mut app = TestApp::new().await;
    let fx = setup_task(&mut app).await;

    let fake_task_id = Uuid::new_v4();
    let attachments_fake_task = format!(
        "/v1/tenants/{}/projects/{}/tasks/{}/attachments",
        fx.tenant_id, fx.project_id, fake_task_id
    );
    let list_missing_task = app.get_with_session(&attachments_fake_task).await;
    assert_eq!(list_missing_task.status(), StatusCode::NOT_FOUND);

    let fake_file_id = Uuid::new_v4();
    let attachments_path = format!(
        "/v1/tenants/{}/projects/{}/tasks/{}/attachments",
        fx.tenant_id, fx.project_id, fx.task_id
    );
    let attach_missing_file = app
        .post_json_with_session(
            &attachments_path,
            serde_json::json!({ "drive_file_id": fake_file_id }),
        )
        .await;
    assert_eq!(attach_missing_file.status(), StatusCode::NOT_FOUND);

    let user = app.insert_user(false, false).await;
    let file_id = insert_drive_file(&app, fx.tenant_id, user.id).await;
    let attach_ok = app
        .post_json_with_session(
            &attachments_path,
            serde_json::json!({ "drive_file_id": file_id }),
        )
        .await;
    assert_eq!(attach_ok.status(), StatusCode::CREATED);

    let attach_duplicate = app
        .post_json_with_session(
            &attachments_path,
            serde_json::json!({ "drive_file_id": file_id }),
        )
        .await;
    assert_eq!(attach_duplicate.status(), StatusCode::CONFLICT);

    let other_user = app.insert_user(false, false).await;
    let other_tp = app.insert_tenant_project(other_user.id).await;
    let cross_tenant_attach = format!(
        "/v1/tenants/{}/projects/{}/tasks/{}/attachments",
        other_tp.tenant_id, other_tp.project_id, fx.task_id
    );
    let cross_tenant = app
        .post_json_with_session(
            &cross_tenant_attach,
            serde_json::json!({ "drive_file_id": file_id }),
        )
        .await;
    assert_eq!(cross_tenant.status(), StatusCode::FORBIDDEN);

    let views_base = format!(
        "/v1/tenants/{}/projects/{}/task-views",
        fx.tenant_id, fx.project_id
    );
    let invalid_view = app
        .post_json_with_session(
            &views_base,
            serde_json::json!({
                "name": "Invalid view",
                "view_type": "kanban"
            }),
        )
        .await;
    assert_eq!(invalid_view.status(), StatusCode::BAD_REQUEST);
}
