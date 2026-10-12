use crate::common::{TestApp, TestTenantProject, TestUser, execute_sql};
use axum::http::StatusCode;
use chrono::{Datelike, Duration, Utc};
use entity::{project_statuses, task_assignees, tasks};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use serde_json::Value;
use uuid::Uuid;

fn url(tp: &TestTenantProject, query: &str) -> String {
    format!("/v1/tenants/{}/users/me/dashboard?{query}", tp.tenant_id)
}
async fn body(app: &TestApp, tp: &TestTenantProject, query: &str) -> Value {
    let res = app.get_with_session(&url(tp, query)).await;
    if res.status() != StatusCode::OK {
        panic!("dashboard: {} {}", res.status(), res.text().await.unwrap());
    }
    res.json().await.unwrap()
}
async fn status(app: &TestApp, project: Uuid, done: bool) -> Uuid {
    let id = Uuid::new_v4();
    project_statuses::ActiveModel {
        id: Set(id),
        project_id: Set(project),
        name: Set(if done { "Done" } else { "Todo" }.into()),
        color: Set("#123456".into()),
        position: Set(if done { 1 } else { 0 }),
        is_default: Set(!done),
        is_done_state: Set(done),
        is_default_done: Set(done),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .unwrap();
    id
}
async fn setup(app: &mut TestApp) -> (TestUser, TestTenantProject, Uuid, Uuid) {
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tp = app.insert_tenant_project(user.id).await;
    let todo = status(app, tp.project_id, false).await;
    let done = status(app, tp.project_id, true).await;
    (user, tp, todo, done)
}
async fn seed(
    app: &TestApp,
    tp: &TestTenantProject,
    user: Uuid,
    status: Uuid,
    count: i32,
) -> Vec<Uuid> {
    let now = Utc::now();
    let ids: Vec<_> = (0..count).map(|_| Uuid::new_v4()).collect();
    tasks::Entity::insert_many(ids.iter().enumerate().map(|(index, id)| {
        tasks::ActiveModel {
            id: Set(*id),
            project_id: Set(tp.project_id),
            seq_id: Set(index as i32 + 1),
            title: Set(format!("Task {}", index + 1)),
            description: Set(None),
            status_id: Set(status),
            priority: Set(tasks::TaskPriority::Medium),
            progress_pct: Set(0),
            parent_task_id: Set(None),
            milestone_id: Set(None),
            soft_deadline: Set(Some(
                now.date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
                    .into(),
            )),
            hard_deadline: Set(None),
            estimated_minutes: Set(None),
            is_archived: Set(false),
            created_by: Set(user),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
            deleted_at: Set(None),
            sprint_id: Set(None),
            completed_at: Set(None),
        }
    }))
    .exec(&app.state.db)
    .await
    .unwrap();
    task_assignees::Entity::insert_many(ids.iter().map(|id| task_assignees::ActiveModel {
        id: Set(Uuid::new_v4()),
        task_id: Set(*id),
        user_id: Set(user),
        role: Set("primary".into()),
        assigned_at: Set(now.into()),
    }))
    .exec(&app.state.db)
    .await
    .unwrap();
    ids
}

#[tokio::test]
async fn counts_are_not_truncated_and_pages_are_stable() {
    let mut app = TestApp::new().await;
    let (user, tp, todo, done) = setup(&mut app).await;
    let ids = seed(&app, &tp, user.id, todo, 60).await;
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET soft_deadline = now() - interval '2 days' WHERE id = $1",
        vec![ids[53].into()],
    )
    .await;
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET soft_deadline = NULL WHERE id = $1",
        vec![ids[54].into()],
    )
    .await;
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET status_id = $1, completed_at = now() WHERE id = $2",
        vec![done.into(), ids[55].into()],
    )
    .await;
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET status_id = $1, completed_at = now() - interval '8 days' WHERE id = $2",
        vec![done.into(), ids[56].into()],
    )
    .await;
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET is_archived = true WHERE id = $1",
        vec![ids[57].into()],
    )
    .await;
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET deleted_at = now() WHERE id = $1",
        vec![ids[58].into()],
    )
    .await;
    execute_sql(
        &app.state.db,
        "DELETE FROM task_assignees WHERE task_id = $1",
        vec![ids[59].into()],
    )
    .await;
    for id in [ids[0], ids[1], ids[2], ids[3], ids[57], ids[58]] {
        execute_sql(&app.state.db, "INSERT INTO task_activities (id,task_id,user_id,event_type,payload,created_at) VALUES ($1,$2,$3,'task_created','{}',now())", vec![Uuid::new_v4().into(), id.into(), user.id.into()]).await;
    }
    let first = body(&app, &tp, "filter=today&limit=999").await;
    assert_eq!(first["activities"].as_array().unwrap().len(), 3);
    assert!(
        first["activities"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["task_seq_id"].as_i64().unwrap() <= 4)
    );
    assert_eq!(first["counts"]["today"], 53);
    assert_eq!(first["counts"]["open"], 55);
    assert_eq!(first["counts"]["overdue"], 1);
    assert_eq!(first["counts"]["completed_week"], 1);
    assert_eq!(first["total"], 53);
    assert_eq!(first["tasks"].as_array().unwrap().len(), 50);
    assert_eq!(first["tasks"][0]["done_status_id"], done.to_string());
    assert_eq!(first["projects"][0]["total"], 58);
    assert_eq!(first["projects"][0]["completed"], 2);
    let second = body(&app, &tp, "filter=today&limit=50&offset=50").await;
    assert_eq!(second["tasks"].as_array().unwrap().len(), 3);
    let mut returned: Vec<_> = first["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["tasks"].as_array().unwrap())
        .map(|t| t["task"]["id"].as_str().unwrap())
        .collect();
    returned.sort();
    returned.dedup();
    assert_eq!(returned.len(), 53);
    assert_eq!(
        body(&app, &tp, "filter=all&limit=0").await["tasks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        body(&app, &tp, "filter=all&offset=999").await["tasks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        body(&app, &tp, "filter=overdue").await["tasks"][0]["task"]["id"],
        ids[53].to_string()
    );
}

#[tokio::test]
async fn week_and_deadlines_use_calendar_dates_and_local_completion_boundaries() {
    let mut app = TestApp::new().await;
    let (user, tp, todo, done) = setup(&mut app).await;
    let ids = seed(&app, &tp, user.id, todo, 6).await;
    let local_today = (Utc::now() + Duration::hours(9)).date_naive();
    let monday = local_today - Duration::days(local_today.weekday().num_days_from_monday().into());
    let start = monday.and_hms_opt(0, 0, 0).unwrap().and_utc() - Duration::hours(9);
    for (id, at) in [
        (ids[0], start - Duration::milliseconds(1)),
        (ids[1], start),
        (
            ids[2],
            start + Duration::days(7) - Duration::milliseconds(1),
        ),
        (ids[3], start + Duration::days(7)),
    ] {
        execute_sql(
            &app.state.db,
            "UPDATE tasks SET status_id = $1, completed_at = $2 WHERE id = $3",
            vec![done.into(), at.into(), id.into()],
        )
        .await;
    }
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET soft_deadline = $1 WHERE id = $2",
        vec![
            local_today.and_hms_opt(0, 0, 0).unwrap().and_utc().into(),
            ids[4].into(),
        ],
    )
    .await;
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET soft_deadline = NULL, hard_deadline = $1 WHERE id = $2",
        vec![
            (monday + Duration::days(7))
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
                .into(),
            ids[5].into(),
        ],
    )
    .await;
    let dashboard = body(&app, &tp, "timezone=Asia%2FTokyo&filter=week").await;
    assert_eq!(dashboard["counts"]["completed_week"], 2);
    assert_eq!(dashboard["days"].as_array().unwrap().len(), 7);
    assert_eq!(dashboard["days"][0]["date"], monday.to_string());
    assert_eq!(dashboard["days"][0]["count"], 1);
    assert_eq!(dashboard["days"][6]["count"], 1);
    assert_eq!(dashboard["counts"]["today"], 1);
    assert_eq!(dashboard["counts"]["week"], 1);
    assert_eq!(dashboard["tasks"][0]["task"]["id"], ids[4].to_string());
    assert_eq!(dashboard["total"], 1);
}

#[tokio::test]
async fn projects_counts_and_updates_follow_member_and_guest_access() {
    let mut app = TestApp::new().await;
    let (owner, tp, todo, _) = setup(&mut app).await;
    let viewer = app.insert_user_default().await;
    let other = app.insert_tenant_project(owner.id).await;
    execute_sql(
        &app.state.db,
        "UPDATE projects SET tenant_id = $1 WHERE id = $2",
        vec![tp.tenant_id.into(), other.project_id.into()],
    )
    .await;
    let other_status = status(&app, other.project_id, false).await;
    let public_ids = seed(&app, &tp, viewer.id, todo, 1).await;
    let private_ids = seed(&app, &other, viewer.id, other_status, 1).await;
    execute_sql(
        &app.state.db,
        "INSERT INTO tenant_members (id,tenant_id,user_id,role) VALUES ($1,$2,$3,'Member')",
        vec![Uuid::new_v4().into(), tp.tenant_id.into(), viewer.id.into()],
    )
    .await;
    execute_sql(
        &app.state.db,
        "INSERT INTO project_members (id,project_id,user_id,role) VALUES ($1,$2,$3,'Admin')",
        vec![
            Uuid::new_v4().into(),
            other.project_id.into(),
            owner.id.into(),
        ],
    )
    .await;
    for id in [public_ids[0], private_ids[0]] {
        execute_sql(&app.state.db,"INSERT INTO task_activities (id,task_id,user_id,event_type,payload,created_at) VALUES ($1,$2,$3,'task_created','{}',now())",vec![Uuid::new_v4().into(),id.into(),owner.id.into()]).await;
    }
    app.login_session_no_content(&viewer.email, &viewer.password)
        .await;
    let member = body(&app, &tp, "filter=all").await;
    assert_eq!(member["counts"]["open"], 1);
    assert_eq!(member["projects"].as_array().unwrap().len(), 1);
    assert_eq!(
        member["projects"][0]["project"]["id"],
        tp.project_id.to_string()
    );
    assert_eq!(member["activities"].as_array().unwrap().len(), 1);
    assert_eq!(member["tasks"][0]["task"]["id"], public_ids[0].to_string());
    // An explicit project member without tenant membership sees only that project.
    execute_sql(
        &app.state.db,
        "INSERT INTO project_members (id,project_id,user_id,role) VALUES ($1,$2,$3,'Member')",
        vec![
            Uuid::new_v4().into(),
            other.project_id.into(),
            viewer.id.into(),
        ],
    )
    .await;
    execute_sql(
        &app.state.db,
        "DELETE FROM tenant_members WHERE tenant_id = $1 AND user_id = $2",
        vec![tp.tenant_id.into(), viewer.id.into()],
    )
    .await;
    let guest = body(&app, &tp, "filter=all").await;
    assert_eq!(guest["counts"]["open"], 1);
    assert_eq!(guest["projects"].as_array().unwrap().len(), 1);
    assert_eq!(
        guest["projects"][0]["project"]["id"],
        other.project_id.to_string()
    );
    assert_eq!(guest["activities"].as_array().unwrap().len(), 1);
    assert_eq!(guest["tasks"][0]["task"]["id"], private_ids[0].to_string());
    execute_sql(
        &app.state.db,
        "DELETE FROM project_members WHERE project_id = $1 AND user_id = $2",
        vec![other.project_id.into(), viewer.id.into()],
    )
    .await;
    assert_eq!(
        app.get_with_session(&url(&tp, "")).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn empty_dashboard_validation_and_session_guard() {
    let mut app = TestApp::new().await;
    let (_, tp, _, _) = setup(&mut app).await;
    let empty = body(&app, &tp, "").await;
    assert_eq!(empty["counts"]["open"], 0);
    assert_eq!(empty["counts"]["completed_week"], 0);
    assert_eq!(empty["projects"][0]["total"], 0);
    assert!(empty["tasks"].as_array().unwrap().is_empty());
    assert!(empty["activities"].as_array().unwrap().is_empty());
    assert_eq!(empty["days"].as_array().unwrap().len(), 7);
    for query in [
        "filter=unknown",
        "timezone=invalid",
        "offset=18446744073709551615",
    ] {
        assert_eq!(
            app.get_with_session(&url(&tp, query)).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    app.reset_session_client();
    assert_eq!(
        app.get(&url(&tp, "")).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn token_scope_and_tenant_bindings_are_enforced() {
    use entity::scopes::Scope;
    let mut app = TestApp::new().await;
    let (user, tp, _, _) = setup(&mut app).await;
    let other = app.insert_tenant_project(user.id).await;
    let read_task_only = app
        .insert_pat(user.id, tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&url(&tp, ""), &read_task_only)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let full = app
        .insert_pat(
            user.id,
            tp.tenant_id,
            vec![Scope::ReadTask, Scope::ReadProject],
            None,
        )
        .await;
    assert_eq!(
        app.get_with_bearer(&url(&tp, ""), &full).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        app.get_with_bearer(&url(&other, ""), &full).await.status(),
        StatusCode::FORBIDDEN
    );
    let restricted = app
        .insert_pat(
            user.id,
            tp.tenant_id,
            vec![Scope::ReadTask, Scope::ReadProject],
            Some(vec![tp.project_id]),
        )
        .await;
    assert_eq!(
        app.get_with_bearer(&url(&tp, ""), &restricted)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}
