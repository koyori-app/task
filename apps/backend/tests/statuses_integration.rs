mod common;

use axum::http::StatusCode;
use common::{TestApp, TestTenantProject};
use entity::{project_statuses, tasks};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

async fn create_status(
    app: &TestApp,
    tp: &TestTenantProject,
    name: &str,
    is_default: bool,
    is_done_state: bool,
) -> Uuid {
    let response = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/statuses",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({
                "name": name,
                "color": "#336699",
                "position": if is_done_state { 0 } else { 1 },
                "is_default": is_default,
                "is_done_state": is_done_state,
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

async fn create_task(app: &TestApp, tp: &TestTenantProject, status_id: Uuid, title: &str) -> Uuid {
    let response = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/tasks",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({ "title": title, "status_id": status_id }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn setup() -> (TestApp, TestTenantProject, Uuid, Uuid, Uuid, Uuid) {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tp = app.insert_tenant_project(user.id).await;
    let old_done_id = create_status(&app, &tp, "Done", false, true).await;
    let next_done_id = create_status(&app, &tp, "Reviewed", true, false).await;
    let old_task_id = create_task(&app, &tp, old_done_id, "Already done").await;
    let next_task_id = create_task(&app, &tp, next_done_id, "Ready to finish").await;
    (
        app,
        tp,
        old_done_id,
        next_done_id,
        old_task_id,
        next_task_id,
    )
}

async fn switch_done(app: &TestApp, tp: &TestTenantProject, status_id: Uuid) -> StatusCode {
    app.put_json_with_session(
        &format!(
            "/v1/tenants/{}/projects/{}/statuses/{status_id}",
            tp.tenant_id, tp.project_id
        ),
        serde_json::json!({ "is_done_state": true }),
    )
    .await
    .status()
}

async fn update_status(
    app: &TestApp,
    tp: &TestTenantProject,
    status_id: Uuid,
    payload: serde_json::Value,
) -> StatusCode {
    app.put_json_with_session(
        &format!(
            "/v1/tenants/{}/projects/{}/statuses/{status_id}",
            tp.tenant_id, tp.project_id
        ),
        payload,
    )
    .await
    .status()
}

async fn delete_status(app: &TestApp, tp: &TestTenantProject, status_id: Uuid) -> StatusCode {
    app.delete_with_session(&format!(
        "/v1/tenants/{}/projects/{}/statuses/{status_id}",
        tp.tenant_id, tp.project_id
    ))
    .await
    .status()
}

async fn project_statuses(app: &TestApp, tp: &TestTenantProject) -> Vec<project_statuses::Model> {
    project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(tp.project_id))
        .all(&app.state.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn creating_done_status_replaces_the_existing_done_status() {
    let (app, tp, old_done_id, _default_id, _old_task_id, _next_task_id) = setup().await;

    let new_done_id = create_status(&app, &tp, "Released", false, true).await;
    let statuses = project_statuses(&app, &tp).await;
    let done_statuses: Vec<_> = statuses
        .iter()
        .filter(|status| status.is_done_state)
        .collect();

    assert_eq!(done_statuses.len(), 1);
    assert_eq!(done_statuses[0].id, new_done_id);
    assert!(
        !statuses
            .iter()
            .find(|status| status.id == old_done_id)
            .unwrap()
            .is_done_state
    );
}

#[tokio::test]
async fn repeated_create_status_api_calls_cannot_create_multiple_done_statuses() {
    let (app, tp, _old_done_id, _default_id, _old_task_id, _next_task_id) = setup().await;

    let first_created_id = create_status(&app, &tp, "Released", false, true).await;
    let second_created_id = create_status(&app, &tp, "Archived", false, true).await;
    let statuses = project_statuses(&app, &tp).await;
    let done_statuses: Vec<_> = statuses
        .iter()
        .filter(|status| status.is_done_state)
        .collect();

    assert_eq!(done_statuses.len(), 1);
    assert_eq!(done_statuses[0].id, second_created_id);
    assert!(
        !statuses
            .iter()
            .find(|status| status.id == first_created_id)
            .unwrap()
            .is_done_state
    );
}

#[tokio::test]
async fn creating_default_status_still_replaces_the_existing_default() {
    let (app, tp, _old_done_id, old_default_id, _old_task_id, _next_task_id) = setup().await;

    let new_default_id = create_status(&app, &tp, "Backlog", true, false).await;
    let statuses = project_statuses(&app, &tp).await;
    let default_statuses: Vec<_> = statuses.iter().filter(|status| status.is_default).collect();

    assert_eq!(default_statuses.len(), 1);
    assert_eq!(default_statuses[0].id, new_default_id);
    assert!(
        !statuses
            .iter()
            .find(|status| status.id == old_default_id)
            .unwrap()
            .is_default
    );
}

#[tokio::test]
async fn current_default_cannot_be_explicitly_unset() {
    let (app, tp, _old_done_id, default_id, _old_task_id, _next_task_id) = setup().await;

    assert_eq!(
        update_status(
            &app,
            &tp,
            default_id,
            serde_json::json!({ "is_default": false }),
        )
        .await,
        StatusCode::BAD_REQUEST
    );

    let default = project_statuses::Entity::find_by_id(default_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    assert!(default.is_default);
}

#[tokio::test]
async fn current_done_cannot_be_explicitly_unset_and_completion_is_preserved() {
    let (app, tp, done_id, _next_done_id, done_task_id, _next_task_id) = setup().await;
    let completed_at_before = tasks::Entity::find_by_id(done_task_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap()
        .completed_at;
    assert!(completed_at_before.is_some());

    assert_eq!(
        update_status(
            &app,
            &tp,
            done_id,
            serde_json::json!({ "is_done_state": false }),
        )
        .await,
        StatusCode::BAD_REQUEST
    );

    let done = project_statuses::Entity::find_by_id(done_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    let done_task = tasks::Entity::find_by_id(done_task_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    assert!(done.is_done_state);
    assert_eq!(done_task.completed_at, completed_at_before);
}

#[tokio::test]
async fn only_done_status_cannot_be_deleted() {
    let (app, tp, done_id, _next_done_id, _old_task_id, _next_task_id) = setup().await;

    assert_eq!(
        delete_status(&app, &tp, done_id).await,
        StatusCode::BAD_REQUEST
    );
    assert!(
        project_statuses::Entity::find_by_id(done_id)
            .one(&app.state.db)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn default_status_cannot_be_deleted() {
    let (app, tp, _done_id, default_id, _old_task_id, _next_task_id) = setup().await;

    assert_eq!(
        delete_status(&app, &tp, default_id).await,
        StatusCode::BAD_REQUEST
    );
    assert!(
        project_statuses::Entity::find_by_id(default_id)
            .one(&app.state.db)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn done_switch_is_unique_and_migrates_task_completion_timestamps() {
    let (app, tp, old_done_id, next_done_id, old_task_id, next_task_id) = setup().await;

    assert_eq!(switch_done(&app, &tp, next_done_id).await, StatusCode::OK);

    let done_statuses = project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(tp.project_id))
        .filter(project_statuses::Column::IsDoneState.eq(true))
        .all(&app.state.db)
        .await
        .unwrap();
    assert_eq!(done_statuses.len(), 1);
    assert_eq!(done_statuses[0].id, next_done_id);

    let old_task = tasks::Entity::find_by_id(old_task_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    let next_task = tasks::Entity::find_by_id(next_task_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old_task.status_id, old_done_id);
    assert!(old_task.completed_at.is_none());
    assert!(next_task.completed_at.is_some());
}

#[tokio::test]
async fn done_switch_rolls_back_flags_and_task_timestamps_on_partial_failure() {
    let (app, tp, old_done_id, next_done_id, old_task_id, next_task_id) = setup().await;
    let suffix = Uuid::new_v4().simple().to_string();
    let function_name = format!("fail_done_switch_{suffix}");
    let trigger_name = format!("fail_done_switch_trigger_{suffix}");
    app.state
        .db
        .execute_unprepared(&format!(
            r#"
            CREATE FUNCTION {function_name}() RETURNS trigger AS $$
            BEGIN
                RAISE EXCEPTION 'forced done switch failure';
            END;
            $$ LANGUAGE plpgsql;
            CREATE TRIGGER {trigger_name}
            BEFORE UPDATE OF is_done_state ON project_statuses
            FOR EACH ROW
            WHEN (NEW.id = '{next_done_id}'::uuid AND NEW.is_done_state = true)
            EXECUTE FUNCTION {function_name}();
            "#
        ))
        .await
        .unwrap();

    let status = switch_done(&app, &tp, next_done_id).await;

    app.state
        .db
        .execute_unprepared(&format!(
            "DROP TRIGGER {trigger_name} ON project_statuses; DROP FUNCTION {function_name}();"
        ))
        .await
        .unwrap();
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let old_done = project_statuses::Entity::find_by_id(old_done_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    let next_done = project_statuses::Entity::find_by_id(next_done_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    assert!(old_done.is_done_state);
    assert!(!next_done.is_done_state);

    let old_task = tasks::Entity::find_by_id(old_task_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    let next_task = tasks::Entity::find_by_id(next_task_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    assert!(old_task.completed_at.is_some());
    assert!(next_task.completed_at.is_none());
}
