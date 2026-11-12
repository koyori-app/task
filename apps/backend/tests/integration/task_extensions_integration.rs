// Drive の行を作るテストは、backfill のテスト（`drive_project_id_backfill_integration`）が
// 実行前に `TRUNCATE drive_files, drive_folders CASCADE` を流すのと同じ鍵で直列化する。
// backfill の SQL はテナントを跨いで全行を見るので、あちらは Drive を空にしてからでないと
// 他のファイルの残骸で落ちる。鍵を共有しないと、その TRUNCATE がこちらの実行中の行を
// 巻き添えにする（CASCADE は drive_folder_shares と task_attachments にも及ぶ）。

use crate::common::TestApp;
use axum::http::StatusCode;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::Value;
use uuid::Uuid;

use entity::{drive_files, project_members};

struct TaskFixture {
    tenant_id: Uuid,
    project_id: Uuid,
    task_id: String,
    status_done_id: String,
    label_id: String,
}

async fn setup_task(app: &mut TestApp) -> TaskFixture {
    let user = app.insert_user(true, false).await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
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
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        content_hash: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert drive file");
    file_id
}

#[tokio::test]
#[serial_test::file_serial(drive)]
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
                    "add_label_ids": [fx.label_id]
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
#[serial_test::file_serial(drive)]
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

async fn add_project_member(app: &TestApp, project_id: Uuid, user_id: Uuid) {
    crate::common::ensure_tenant_member_for_project(&app.state.db, project_id, user_id).await;
    project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        user_id: Set(user_id),
        role: Set(project_members::ProjectRole::Member),
    }
    .insert(&app.state.db)
    .await
    .expect("add project member");
}

#[tokio::test]
#[serial_test::file_serial(drive)]
async fn task_extensions_acl_cases() {
    let mut app = TestApp::new().await;
    let fx = setup_task(&mut app).await; // owner としてログイン済み

    let views_base = format!(
        "/v1/tenants/{}/projects/{}/task-views",
        fx.tenant_id, fx.project_id
    );

    // owner が非共有ビューと共有ビューを作成
    let private = app
        .post_json_with_session(
            &views_base,
            serde_json::json!({ "name": "Owner private", "is_shared": false }),
        )
        .await;
    assert_eq!(private.status(), StatusCode::CREATED);
    let private_body: Value = private.json().await.expect("private view json");
    let private_view_id = private_body["id"]
        .as_str()
        .expect("private view id")
        .to_string();

    let shared = app
        .post_json_with_session(
            &views_base,
            serde_json::json!({ "name": "Owner shared", "is_shared": true }),
        )
        .await;
    assert_eq!(shared.status(), StatusCode::CREATED);
    let shared_body: Value = shared.json().await.expect("shared view json");
    let shared_view_id = shared_body["id"]
        .as_str()
        .expect("shared view id")
        .to_string();

    // プロジェクトメンバー B と、メンバーでない outsider を用意
    let member_b = app.insert_user(false, false).await;
    add_project_member(&app, fx.project_id, member_b.id).await;
    let outsider = app.insert_user(false, false).await;

    // ケース1: 非メンバーは search / bulk で 403
    app.login_session_no_content(&outsider.email, &outsider.password)
        .await;
    let search_path = format!(
        "/v1/tenants/{}/projects/{}/tasks/search?q=OAuth",
        fx.tenant_id, fx.project_id
    );
    let outsider_search = app.get_with_session(&search_path).await;
    assert_eq!(outsider_search.status(), StatusCode::FORBIDDEN);

    let bulk_path = format!(
        "/v1/tenants/{}/projects/{}/tasks/bulk",
        fx.tenant_id, fx.project_id
    );
    let outsider_bulk = app
        .post_json_with_session(
            &bulk_path,
            serde_json::json!({
                "task_ids": [Uuid::new_v4()],
                "update": { "status_id": fx.status_done_id }
            }),
        )
        .await;
    assert_eq!(outsider_bulk.status(), StatusCode::FORBIDDEN);

    // ケース2: メンバー B のビュー一覧には共有ビューのみ表示され、owner の非共有ビューは見えない
    app.login_session_no_content(&member_b.email, &member_b.password)
        .await;
    let list = app.get_with_session(&views_base).await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_body: Value = list.json().await.expect("list views json");
    let ids: Vec<&str> = list_body["views"]
        .as_array()
        .expect("views array")
        .iter()
        .filter_map(|v| v["id"].as_str())
        .collect();
    assert!(
        ids.contains(&shared_view_id.as_str()),
        "共有ビューはメンバーに見える"
    );
    assert!(
        !ids.contains(&private_view_id.as_str()),
        "owner の非共有ビューはメンバーに見えない"
    );

    // ケース3: メンバー B は owner のビューを更新できない（403）
    let patch = app
        .patch_json_with_session(
            &format!("{views_base}/{private_view_id}"),
            serde_json::json!({ "name": "hijack" }),
        )
        .await;
    assert_eq!(patch.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial_test::file_serial(drive)]
async fn task_detail_embeds_attachments_and_list_does_not() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;

    let status_resp = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/statuses",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({
                "name": "Backlog",
                "color": "#336699",
                "position": 0,
                "is_default": true
            }),
        )
        .await;
    assert_eq!(status_resp.status(), StatusCode::CREATED);
    let status_body: Value = status_resp.json().await.expect("status json");
    let status_id = status_body["id"].as_str().expect("status id").to_string();

    let tasks_path = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let created = app
        .post_json_with_session(
            &tasks_path,
            serde_json::json!({ "title": "attachment task", "status_id": status_id }),
        )
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_body: Value = created.json().await.expect("task json");
    let task_id = created_body["id"].as_str().expect("task id").to_string();
    assert_eq!(
        created_body["attachments"],
        serde_json::json!([]),
        "作成直後の応答も attachments を持つ"
    );

    let task_path = format!("{tasks_path}/{task_id}");
    let attachments_path = format!("{task_path}/attachments");

    // 添付が無いときは空配列（キーごと省かない）
    let empty = app.get_with_session(&task_path).await;
    assert_eq!(empty.status(), StatusCode::OK);
    let empty_body: Value = empty.json().await.expect("detail json");
    assert_eq!(empty_body["attachments"], serde_json::json!([]));

    // 2 件添付する（2 件目のほうが新しい）
    let mut attachment_ids = Vec::new();
    for _ in 0..2 {
        let file_id = insert_drive_file(&app, tp.tenant_id, owner.id).await;
        let attach = app
            .post_json_with_session(
                &attachments_path,
                serde_json::json!({ "drive_file_id": file_id }),
            )
            .await;
        assert_eq!(attach.status(), StatusCode::CREATED);
        let body: Value = attach.json().await.expect("attach json");
        attachment_ids.push(body["id"].as_str().expect("attachment id").to_string());
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let list = app.get_with_session(&attachments_path).await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_body: Value = list.json().await.expect("attachments json");

    let detail = app.get_with_session(&task_path).await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_body: Value = detail.json().await.expect("detail json");
    let embedded = detail_body["attachments"].as_array().expect("attachments");
    assert_eq!(embedded.len(), 2);
    assert_eq!(
        detail_body["attachments"], list_body["attachments"],
        "詳細の attachments は添付一覧 API と同じ要素・同じ並び"
    );
    for item in embedded {
        for key in [
            "id",
            "drive_file_id",
            "name",
            "mime_type",
            "size",
            "url",
            "created_at",
            "source",
        ] {
            assert!(item.get(key).is_some(), "attachments[].{key} がある");
        }
        assert_eq!(item["source"], "manual");
        assert_eq!(item["name"], "spec.pdf");
    }

    // 一覧には載せない
    let tasks_list = app.get_with_session(&tasks_path).await;
    assert_eq!(tasks_list.status(), StatusCode::OK);
    let tasks_list_body: Value = tasks_list.json().await.expect("tasks list json");
    let listed = tasks_list_body["tasks"].as_array().expect("tasks array");
    assert!(!listed.is_empty());
    for task in listed {
        assert!(
            task.get("attachments").is_none(),
            "一覧に attachments は無い"
        );
    }

    // 更新の応答も詳細と同じ形
    let updated = app
        .put_json_with_session(&task_path, serde_json::json!({ "title": "renamed" }))
        .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated_body: Value = updated.json().await.expect("update json");
    assert_eq!(
        updated_body["attachments"].as_array().map(Vec::len),
        Some(2)
    );

    // 解除すると詳細からも消える
    let removed = app
        .delete_with_session(&format!("{attachments_path}/{}", attachment_ids[0]))
        .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let after: Value = app
        .get_with_session(&task_path)
        .await
        .json()
        .await
        .expect("detail json");
    assert_eq!(after["attachments"].as_array().map(Vec::len), Some(1));

    // 認可: メンバーは読める（過剰拒否でない）、非メンバーは 403（添付一覧と同じ）
    let member = app.insert_user(false, false).await;
    add_project_member(&app, tp.project_id, member.id).await;
    let outsider = app.insert_user(false, false).await;

    app.login_session_no_content(&member.email, &member.password)
        .await;
    let member_detail = app.get_with_session(&task_path).await;
    assert_eq!(member_detail.status(), StatusCode::OK);
    let member_body: Value = member_detail.json().await.expect("member detail json");
    assert_eq!(member_body["attachments"].as_array().map(Vec::len), Some(1));

    app.login_session_no_content(&outsider.email, &outsider.password)
        .await;
    assert_eq!(
        app.get_with_session(&task_path).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.get_with_session(&attachments_path).await.status(),
        StatusCode::FORBIDDEN
    );
}
