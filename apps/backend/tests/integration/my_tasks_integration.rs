use crate::common::{
    TestApp, TestTenantProject, TestUser, create_status, execute_sql, insert_tenant,
};
use axum::http::StatusCode;
use chrono::{Datelike, Duration, FixedOffset, NaiveDate, Utc};
use entity::projects;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
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

/// 同時の要求でも個人プロジェクトは 1 つだけ作られる。既定のキーがテナント内で埋まっていると
/// 要求ごとに別のキーへ作り直すので、(tenant_id, key) の UNIQUE では止まらない。
#[tokio::test]
async fn personal_project_is_created_once_under_concurrent_requests() {
    let mut app = TestApp::new().await;
    let (user, tp) = setup(&mut app).await;
    // 既定のキーはユーザーごとに決まるので、別テナントで作って知る
    let other_tenant = insert_tenant(&app.state.db, user.id).await;
    let default_key = app
        .get_with_session(&format!("{}/personal-project", my_tasks_base(other_tenant)))
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap()["key"]
        .clone();
    let occupied = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/projects", tp.tenant_id),
            serde_json::json!({"name": "occupied", "key": default_key}),
        )
        .await;
    assert_eq!(occupied.status(), StatusCode::CREATED);

    let url = format!("{}/personal-project", my_tasks_base(tp.tenant_id));
    let responses = futures::future::join_all((0..5).map(|_| app.get_with_session(&url))).await;
    for response in responses {
        assert_eq!(response.status(), StatusCode::OK);
    }

    let personal = projects::Entity::find()
        .filter(projects::Column::TenantId.eq(tp.tenant_id))
        .filter(projects::Column::IsPersonal.eq(true))
        .count(&app.state.db)
        .await
        .unwrap();
    assert_eq!(personal, 1);
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

/// 自分に割り当てたタスクを作り、期限（UTC 0 時の暦日）を SQL で入れる。
async fn add_task(
    app: &TestApp,
    user: &TestUser,
    tp: &TestTenantProject,
    status_id: Uuid,
    title: &str,
    soft: Option<NaiveDate>,
    hard: Option<NaiveDate>,
) {
    let path = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let assignee = serde_json::json!([{"user_id": user.id, "role": "assignee"}]);
    let res = app
        .post_json_with_session(
            &path,
            serde_json::json!({"title": title, "status_id": status_id, "assignees": assignee}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let id = res.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    let at = |d: Option<NaiveDate>| d.map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc());
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET soft_deadline = $1, hard_deadline = $2 WHERE id = $3",
        vec![at(soft).into(), at(hard).into(), id.into()],
    )
    .await;
}

async fn titles(app: &TestApp, tp: &TestTenantProject, query: &str) -> Vec<String> {
    let res = app
        .get_with_session(&format!("{}/tasks?{query}", my_tasks_base(tp.tenant_id)))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    res.json::<serde_json::Value>().await.unwrap()["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap().to_string())
        .collect()
}

fn today_in(hours: i32) -> NaiveDate {
    Utc::now()
        .with_timezone(&FixedOffset::east_opt(hours * 3600).unwrap())
        .date_naive()
}

#[tokio::test]
async fn overdue_uses_soft_deadline_like_home() {
    let mut app = TestApp::new().await;
    let (user, tp) = setup(&mut app).await;
    let todo = create_status(&app, &tp, "Todo", false).await;
    let past = Utc::now().date_naive() - Duration::days(3);
    let future = Utc::now().date_naive() + Duration::days(3);
    add_task(&app, &user, &tp, todo, "soft only", Some(past), None).await;
    add_task(
        &app,
        &user,
        &tp,
        todo,
        "soft wins",
        Some(future),
        Some(past),
    )
    .await;
    let got = titles(&app, &tp, "filter=overdue").await;
    assert_eq!(got, vec!["soft only".to_string()]);
}

#[tokio::test]
async fn today_follows_requested_timezone() {
    let mut app = TestApp::new().await;
    let (user, tp) = setup(&mut app).await;
    let todo = create_status(&app, &tp, "Todo", false).await;
    // UTC+14 と UTC-11 は常に暦日が異なるので、UTC 判定だと必ずどちらかが外れる
    let east = today_in(14);
    let west = today_in(-11);
    assert_ne!(east, west);
    add_task(&app, &user, &tp, todo, "east", Some(east), None).await;
    add_task(&app, &user, &tp, todo, "west", Some(west), None).await;
    assert_eq!(
        titles(&app, &tp, "filter=today&timezone=Pacific%2FKiritimati").await,
        vec!["east"]
    );
    assert_eq!(
        titles(&app, &tp, "filter=today&timezone=Pacific%2FPago_Pago").await,
        vec!["west"]
    );
}

#[tokio::test]
async fn week_is_monday_to_sunday_and_excludes_done() {
    let mut app = TestApp::new().await;
    let (user, tp) = setup(&mut app).await;
    let todo = create_status(&app, &tp, "Todo", false).await;
    let done = create_status(&app, &tp, "Done", true).await;
    let today = Utc::now().date_naive();
    let monday = today - Duration::days(today.weekday().num_days_from_monday().into());
    add_task(&app, &user, &tp, todo, "monday", Some(monday), None).await;
    add_task(
        &app,
        &user,
        &tp,
        todo,
        "sunday",
        None,
        Some(monday + Duration::days(6)),
    )
    .await;
    add_task(
        &app,
        &user,
        &tp,
        todo,
        "next monday",
        Some(monday + Duration::days(7)),
        None,
    )
    .await;
    add_task(
        &app,
        &user,
        &tp,
        todo,
        "last sunday",
        Some(monday - Duration::days(1)),
        None,
    )
    .await;
    add_task(&app, &user, &tp, done, "done this week", Some(today), None).await;
    let mut week = titles(&app, &tp, "filter=week").await;
    week.sort();
    assert_eq!(week, vec!["monday", "sunday"]);
    add_task(&app, &user, &tp, done, "done today", Some(today), None).await;
    // 期待は実行日の曜日から組む（月曜は "monday"、日曜は "sunday" が当日期限の未完了として
    // 入り、それ以外の曜日は空）ので、どの曜日に走らせても通る。完了済みは常に出ない。
    let expected_today: Vec<&str> = match today.weekday().num_days_from_monday() {
        0 => vec!["monday"],
        6 => vec!["sunday"],
        _ => vec![],
    };
    assert_eq!(titles(&app, &tp, "filter=today").await, expected_today);
    // 対照: all には完了済みも出る
    assert!(
        titles(&app, &tp, "filter=all")
            .await
            .contains(&"done today".to_string())
    );
}

#[tokio::test]
async fn list_is_ordered_by_deadline_and_rejects_bad_timezone() {
    let mut app = TestApp::new().await;
    let (user, tp) = setup(&mut app).await;
    let todo = create_status(&app, &tp, "Todo", false).await;
    let d = Utc::now().date_naive();
    add_task(&app, &user, &tp, todo, "none", None, None).await;
    add_task(
        &app,
        &user,
        &tp,
        todo,
        "late",
        None,
        Some(d + Duration::days(5)),
    )
    .await;
    add_task(
        &app,
        &user,
        &tp,
        todo,
        "early",
        Some(d + Duration::days(1)),
        None,
    )
    .await;
    assert_eq!(
        titles(&app, &tp, "filter=all").await,
        vec!["early", "late", "none"]
    );
    let res = app
        .get_with_session(&format!(
            "{}/tasks?filter=today&timezone=Nope%2FZone",
            my_tasks_base(tp.tenant_id)
        ))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
