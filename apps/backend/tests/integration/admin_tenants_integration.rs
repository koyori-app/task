use crate::common::TestApp;
use axum::http::StatusCode;
use entity::{project_statuses, tasks};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use uuid::Uuid;

async fn insert_task(app: &TestApp, project_id: Uuid, created_by: Uuid, title: &str) -> Uuid {
    let status_id = Uuid::new_v4();
    project_statuses::ActiveModel {
        id: Set(status_id),
        project_id: Set(project_id),
        name: Set("Todo".into()),
        color: Set("#808080".into()),
        position: Set(0),
        is_default: Set(true),
        is_done_state: Set(false),
        is_default_done: Set(false),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert status");

    let task_id = Uuid::new_v4();
    tasks::ActiveModel {
        id: Set(task_id),
        project_id: Set(project_id),
        seq_id: Set(1),
        title: Set(title.into()),
        description: Set(None),
        status_id: Set(status_id),
        priority: Set(tasks::TaskPriority::Medium),
        progress_pct: Set(0),
        parent_task_id: Set(None),
        milestone_id: Set(None),
        sprint_id: Set(None),
        soft_deadline: Set(None),
        hard_deadline: Set(None),
        planned_start_on: Set(None),
        require_all_assignees_done: Set(false),
        estimated_minutes: Set(None),
        is_archived: Set(false),
        created_by: Set(created_by),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        completed_at: Set(None),
        deleted_at: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert task");
    task_id
}

/// テナント配下プロジェクト一覧・プロジェクト配下タスク一覧（管理者）が
/// ドキュメントどおりの URL で 200 を返し、内容が載る。回帰検知の対象:
/// - routes/admin.rs の nest 二重連結（登録パスがズレて常に 404 だった）
/// - information_schema 検査とタスク SELECT の `?` プレースホルダ未変換
///   （Postgres では実行時構文エラー → 500）
#[tokio::test]
async fn admin_lists_tenant_project_tasks() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let task_id = insert_task(&app, tp.project_id, owner.id, "placeholder regression").await;

    let admin = app.insert_user(true, false).await;
    app.reset_session_client();
    app.login_session(&admin.email, &admin.password).await;

    let projects_response = app
        .get_with_session(&format!("/v1/admin/tenants/{}/projects", tp.tenant_id))
        .await;
    assert_eq!(projects_response.status(), StatusCode::OK);
    let projects_body: serde_json::Value = projects_response.json().await.expect("json body");
    let projects = projects_body["projects"]
        .as_array()
        .expect("projects array");
    assert!(
        projects
            .iter()
            .any(|p| p["id"] == tp.project_id.to_string()),
        "project list must contain the seeded project"
    );

    let response = app
        .get_with_session(&format!(
            "/v1/admin/tenants/{}/projects/{}/tasks",
            tp.tenant_id, tp.project_id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body");
    let tasks = body["tasks"].as_array().expect("tasks array");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], task_id.to_string());
    assert_eq!(tasks[0]["title"], "placeholder regression");

    app.cleanup_user(owner.id).await;
    app.cleanup_user(admin.id).await;
}

/// 管理者のテナント削除は 204 で成功し、テナントが消え、監査ログが 1 件残る。
/// 回帰検知の対象: 削除後に消したテナントの ID を `audit_logs.tenant_id`（外部キー）へ
/// 入れて INSERT し、外部キー違反で 500 になっていた（テナントは消えたまま、ログも残らない）。
#[tokio::test]
async fn admin_deletes_tenant_and_records_audit_log() {
    use entity::{audit_logs, tenants};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let tenant = tenants::Entity::find_by_id(tp.tenant_id)
        .one(&app.state.db)
        .await
        .expect("find tenant")
        .expect("tenant exists");

    let admin = app.insert_user(true, false).await;
    app.reset_session_client();
    app.login_session(&admin.email, &admin.password).await;

    let response = app
        .delete_with_session(&format!("/v1/admin/tenants/{}", tp.tenant_id))
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    assert!(
        tenants::Entity::find_by_id(tp.tenant_id)
            .one(&app.state.db)
            .await
            .expect("find tenant")
            .is_none(),
        "tenant must be deleted"
    );

    let logs = audit_logs::Entity::find()
        .filter(audit_logs::Column::Action.eq("tenant.delete"))
        .filter(audit_logs::Column::ResourceId.eq(tp.tenant_id.to_string()))
        .all(&app.state.db)
        .await
        .expect("find audit logs");
    assert_eq!(logs.len(), 1, "exactly one audit log must remain");
    let log = &logs[0];
    assert_eq!(log.actor_id, Some(admin.id));
    // 外部キーは ON DELETE SET NULL なので、削除後の tenant_id は NULL になる
    assert_eq!(log.tenant_id, None);
    let metadata = log.metadata.as_ref().expect("metadata");
    assert_eq!(metadata["tenant_id"], tp.tenant_id.to_string());
    assert_eq!(metadata["tenant_display_id"], tenant.display_id);
    assert_eq!(metadata["tenant_name"], tenant.name);

    app.cleanup_user(owner.id).await;
    app.cleanup_user(admin.id).await;
}

/// 非管理者はテナントを削除できず、テナントは残る（拒否系）。存在しない ID は管理者でも 404。
#[tokio::test]
async fn non_admin_cannot_delete_tenant() {
    use entity::tenants;
    use sea_orm::EntityTrait;

    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;

    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    let response = app
        .delete_with_session(&format!("/v1/admin/tenants/{}", tp.tenant_id))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        tenants::Entity::find_by_id(tp.tenant_id)
            .one(&app.state.db)
            .await
            .expect("find tenant")
            .is_some(),
        "tenant must remain"
    );

    let admin = app.insert_user(true, false).await;
    app.reset_session_client();
    app.login_session(&admin.email, &admin.password).await;
    let response = app
        .delete_with_session(&format!("/v1/admin/tenants/{}", Uuid::new_v4()))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    app.cleanup_user(owner.id).await;
    app.cleanup_user(admin.id).await;
}
