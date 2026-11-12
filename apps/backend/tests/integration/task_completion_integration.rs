//! 担当者ごとの完了（TASK-494、docs/dev/features/tasks/1.core.md の「担当者」）。
//!
//! `require_all_assignees_done` が on のタスクは、担当者全員が自分の分を完了にして初めて完了になる。
//! 完了ステータスへの直接移動は、未完了の担当者が残るかぎり 409 `assignees-not-done`。

use crate::common::{
    TestApp, TestUser, create_status, ensure_tenant_member_for_project, json_body,
};
use axum::http::StatusCode;
use entity::{task_activities, task_assignees, tasks};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, prelude::Uuid};
use serde_json::{Value, json};

struct Fx {
    tenant_id: Uuid,
    project_id: Uuid,
    owner: TestUser,
    b: TestUser,
    c: TestUser,
    todo: Uuid,
    done: Uuid,
}

impl Fx {
    fn tasks_path(&self) -> String {
        format!(
            "/v1/tenants/{}/projects/{}/tasks",
            self.tenant_id, self.project_id
        )
    }
    fn task_path(&self, task_id: Uuid) -> String {
        format!("{}/{task_id}", self.tasks_path())
    }
    fn assignee_path(&self, task_id: Uuid, user_id: Uuid, action: &str) -> String {
        format!("{}/assignees/{user_id}/{action}", self.task_path(task_id))
    }
}

async fn login(app: &mut TestApp, user: &TestUser) {
    app.login_session_no_content(&user.email, &user.password)
        .await;
}

/// owner / b / c の 3 人がいるプロジェクトと、Todo（既定）・Done（完了）のステータス。
/// 終わったときは owner でログインしている。
async fn setup(app: &mut TestApp) -> Fx {
    let owner = app.insert_user(false, false).await;
    let b = app.insert_user(false, false).await;
    let c = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    ensure_tenant_member_for_project(&app.state.db, tp.project_id, b.id).await;
    ensure_tenant_member_for_project(&app.state.db, tp.project_id, c.id).await;
    login(app, &owner).await;
    let todo = create_status(app, &tp, "Todo", false).await;
    let done = create_status(app, &tp, "Done", true).await;
    Fx {
        tenant_id: tp.tenant_id,
        project_id: tp.project_id,
        owner,
        b,
        c,
        todo,
        done,
    }
}

fn assignees(users: &[&TestUser]) -> Value {
    Value::Array(
        users
            .iter()
            .map(|u| json!({ "user_id": u.id, "role": "assignee" }))
            .collect(),
    )
}

/// 現在ログインしている利用者でタスクを作る。
async fn create_task(
    app: &TestApp,
    fx: &Fx,
    users: &[&TestUser],
    require_all: bool,
    status: Uuid,
) -> reqwest::Response {
    app.post_json_with_session(
        &fx.tasks_path(),
        json!({
            "title": "task",
            "status_id": status,
            "assignees": assignees(users),
            "require_all_assignees_done": require_all,
        }),
    )
    .await
}

async fn create_ok(
    app: &TestApp,
    fx: &Fx,
    users: &[&TestUser],
    require_all: bool,
    status: Uuid,
) -> Uuid {
    let res = create_task(app, fx, users, require_all, status).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = json_body(res).await;
    assert_eq!(body["require_all_assignees_done"], require_all);
    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn complete(app: &TestApp, fx: &Fx, task: Uuid, user: Uuid) -> reqwest::Response {
    app.post_json_with_session(&fx.assignee_path(task, user, "complete"), json!({}))
        .await
}

async fn reopen(app: &TestApp, fx: &Fx, task: Uuid, user: Uuid) -> reqwest::Response {
    app.post_json_with_session(&fx.assignee_path(task, user, "reopen"), json!({}))
        .await
}

async fn put(app: &TestApp, fx: &Fx, task: Uuid, body: Value) -> reqwest::Response {
    app.put_json_with_session(&fx.task_path(task), body).await
}

async fn message(res: reqwest::Response) -> String {
    json_body(res).await["message"]
        .as_str()
        .expect("message")
        .to_string()
}

async fn task_row(app: &TestApp, task: Uuid) -> tasks::Model {
    tasks::Entity::find_by_id(task)
        .one(&app.state.db)
        .await
        .expect("query task")
        .expect("task row")
}

async fn assignee_rows(app: &TestApp, task: Uuid) -> Vec<task_assignees::Model> {
    task_assignees::Entity::find()
        .filter(task_assignees::Column::TaskId.eq(task))
        .all(&app.state.db)
        .await
        .expect("query assignees")
}

async fn completed_count(app: &TestApp, task: Uuid) -> usize {
    assignee_rows(app, task)
        .await
        .iter()
        .filter(|a| a.completed_at.is_some())
        .count()
}

fn completed_in(body: &Value) -> usize {
    body["assignees"]
        .as_array()
        .expect("assignees")
        .iter()
        .filter(|a| !a["completed_at"].is_null())
        .count()
}

async fn activity_count(app: &TestApp, task: Uuid, event_type: &str) -> u64 {
    task_activities::Entity::find()
        .filter(task_activities::Column::TaskId.eq(task))
        .filter(task_activities::Column::EventType.eq(event_type))
        .count(&app.state.db)
        .await
        .expect("count activities")
}

#[tokio::test]
async fn task_completes_only_after_every_assignee_is_done() {
    let mut app = TestApp::new().await;
    let fx = setup(&mut app).await;
    let task = create_ok(&app, &fx, &[&fx.owner, &fx.b], true, fx.todo).await;

    // 1 人目: ステータスは動かず、完了が 1 つ入る
    let first = complete(&app, &fx, task, fx.owner.id).await;
    assert_eq!(first.status(), StatusCode::OK);
    let body = json_body(first).await;
    assert_eq!(body["status_id"], fx.todo.to_string());
    assert_eq!(completed_in(&body), 1);
    assert!(task_row(&app, task).await.completed_at.is_none());

    // 同じ人の 2 回目は冪等
    let again = complete(&app, &fx, task, fx.owner.id).await;
    assert_eq!(again.status(), StatusCode::OK);
    assert_eq!(activity_count(&app, task, "assignee_completed").await, 1);

    // 2 人目: 既定の完了ステータスへ移る
    login(&mut app, &fx.b).await;
    let second = complete(&app, &fx, task, fx.b.id).await;
    assert_eq!(second.status(), StatusCode::OK);
    let body = json_body(second).await;
    assert_eq!(body["status_id"], fx.done.to_string());
    assert!(!body["completed_at"].is_null());
    assert_eq!(completed_in(&body), 2);
    assert_eq!(activity_count(&app, task, "status_changed").await, 1);
    assert_eq!(activity_count(&app, task, "assignee_completed").await, 2);
    assert_eq!(task_row(&app, task).await.status_id, fx.done);
}

#[tokio::test]
async fn direct_move_to_done_is_rejected_while_someone_is_pending() {
    let mut app = TestApp::new().await;
    let fx = setup(&mut app).await;
    let blocked = create_ok(&app, &fx, &[&fx.owner, &fx.b], true, fx.todo).await;

    let res = put(&app, &fx, blocked, json!({ "status_id": fx.done })).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(message(res).await, "assignees-not-done");
    let row = task_row(&app, blocked).await;
    assert_eq!(row.status_id, fx.todo, "DB は変わらない");
    assert!(row.completed_at.is_none());

    // 対照: 設定 off の同じタスク
    let off = create_ok(&app, &fx, &[&fx.owner, &fx.b], false, fx.todo).await;
    let res = put(&app, &fx, off, json!({ "status_id": fx.done })).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(task_row(&app, off).await.status_id, fx.done);

    // 対照: 設定 on でも担当者 0 人なら止めない
    let nobody = create_ok(&app, &fx, &[], true, fx.todo).await;
    let res = put(&app, &fx, nobody, json!({ "status_id": fx.done })).await;
    assert_eq!(res.status(), StatusCode::OK);

    // 同じ要求で設定を on にしても、未完了の担当者が残れば止まる
    let later = create_ok(&app, &fx, &[&fx.owner], false, fx.todo).await;
    let res = put(
        &app,
        &fx,
        later,
        json!({ "status_id": fx.done, "require_all_assignees_done": true }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(task_row(&app, later).await.status_id, fx.todo);
    assert!(!task_row(&app, later).await.require_all_assignees_done);
}

#[tokio::test]
async fn only_the_assignee_can_complete_their_own_part() {
    let mut app = TestApp::new().await;
    let fx = setup(&mut app).await;
    let task = create_ok(&app, &fx, &[&fx.owner, &fx.b], true, fx.todo).await;

    // 他人の分は 403（完了も取り消しも）
    let res = complete(&app, &fx, task, fx.b.id).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let res = reopen(&app, &fx, task, fx.b.id).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(completed_count(&app, task).await, 0);

    // 担当者でない本人は 404
    login(&mut app, &fx.c).await;
    let res = complete(&app, &fx, task, fx.c.id).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 設定 off のタスクは 409
    login(&mut app, &fx.owner).await;
    let off = create_ok(&app, &fx, &[&fx.owner], false, fx.todo).await;
    let res = complete(&app, &fx, off, fx.owner.id).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(message(res).await, "completion-not-required");
    let res = reopen(&app, &fx, off, fx.owner.id).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // 対照: 本人は 200
    let res = complete(&app, &fx, task, fx.owner.id).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(completed_count(&app, task).await, 1);
}

#[tokio::test]
async fn reopening_the_task_clears_everyones_completion() {
    let mut app = TestApp::new().await;
    let fx = setup(&mut app).await;
    let task = create_ok(&app, &fx, &[&fx.owner, &fx.b], true, fx.todo).await;
    complete(&app, &fx, task, fx.owner.id).await;
    login(&mut app, &fx.b).await;
    complete(&app, &fx, task, fx.b.id).await;
    assert_eq!(task_row(&app, task).await.status_id, fx.done);
    assert_eq!(completed_count(&app, task).await, 2);

    // 完了から未完了へ戻すと、全員の完了が消える
    let res = put(&app, &fx, task, json!({ "status_id": fx.todo })).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(completed_count(&app, task).await, 0);
    assert!(task_row(&app, task).await.completed_at.is_none());

    // 戻した直後の直接完了は止まる
    let res = put(&app, &fx, task, json!({ "status_id": fx.done })).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(message(res).await, "assignees-not-done");
}

#[tokio::test]
async fn reopen_own_part() {
    let mut app = TestApp::new().await;
    let fx = setup(&mut app).await;
    let task = create_ok(&app, &fx, &[&fx.owner, &fx.b], true, fx.todo).await;

    // 未完了のタスクでは取り消せる
    complete(&app, &fx, task, fx.owner.id).await;
    assert_eq!(completed_count(&app, task).await, 1);
    let res = reopen(&app, &fx, task, fx.owner.id).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(completed_in(&json_body(res).await), 0);
    assert_eq!(completed_count(&app, task).await, 0);
    assert_eq!(activity_count(&app, task, "assignee_reopened").await, 1);

    // 完了済みのタスクでは取り消せない
    complete(&app, &fx, task, fx.owner.id).await;
    login(&mut app, &fx.b).await;
    complete(&app, &fx, task, fx.b.id).await;
    assert_eq!(task_row(&app, task).await.status_id, fx.done);
    let res = reopen(&app, &fx, task, fx.b.id).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(message(res).await, "task-already-done");
    assert_eq!(completed_count(&app, task).await, 2);
}

#[tokio::test]
async fn assignee_changes() {
    let mut app = TestApp::new().await;
    let fx = setup(&mut app).await;
    let task = create_ok(&app, &fx, &[&fx.owner, &fx.b], true, fx.todo).await;
    complete(&app, &fx, task, fx.owner.id).await;

    // 足した担当者は未完了で始まる
    let res = put(
        &app,
        &fx,
        task,
        json!({ "assignees": assignees(&[&fx.owner, &fx.b, &fx.c]) }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let rows = assignee_rows(&app, task).await;
    assert_eq!(rows.len(), 3);
    assert_eq!(completed_count(&app, task).await, 1);

    // 同じ要求で完了へ移すときは、置き換え後の担当者で判定する
    let res = put(
        &app,
        &fx,
        task,
        json!({ "assignees": assignees(&[&fx.owner, &fx.c]), "status_id": fx.done }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(assignee_rows(&app, task).await.len(), 3, "全体が巻き戻る");

    // 未完了の担当者を外しても、自動では完了にならない
    let res = put(
        &app,
        &fx,
        task,
        json!({ "assignees": assignees(&[&fx.owner]) }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(task_row(&app, task).await.status_id, fx.todo);
    assert_eq!(completed_count(&app, task).await, 1);

    // その後の直接完了は通る
    let res = put(&app, &fx, task, json!({ "status_id": fx.done })).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(task_row(&app, task).await.status_id, fx.done);

    // 完了済みのタスクは、担当者を足しても動かさない
    let res = put(
        &app,
        &fx,
        task,
        json!({ "assignees": assignees(&[&fx.owner, &fx.c]) }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(task_row(&app, task).await.status_id, fx.done);
}

#[tokio::test]
async fn turning_the_rule_off_clears_completions() {
    let mut app = TestApp::new().await;
    let fx = setup(&mut app).await;
    let task = create_ok(&app, &fx, &[&fx.owner, &fx.b], true, fx.todo).await;
    complete(&app, &fx, task, fx.owner.id).await;
    assert_eq!(completed_count(&app, task).await, 1);

    let res = put(
        &app,
        &fx,
        task,
        json!({ "require_all_assignees_done": false }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(json_body(res).await["require_all_assignees_done"], false);
    assert_eq!(completed_count(&app, task).await, 0);
    assert_eq!(
        activity_count(&app, task, "completion_rule_changed").await,
        1
    );

    // off になったので直接完了できる
    let res = put(&app, &fx, task, json!({ "status_id": fx.done })).await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_with_rule_and_done_status_is_rejected() {
    let mut app = TestApp::new().await;
    let fx = setup(&mut app).await;

    let res = create_task(&app, &fx, &[&fx.owner], true, fx.done).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(message(res).await, "assignees-not-done");

    // 対照: 未完了ステータスなら作れる
    let task = create_ok(&app, &fx, &[&fx.owner], true, fx.todo).await;
    assert!(task_row(&app, task).await.require_all_assignees_done);
    // 対照: 設定 off なら完了ステータスでも作れる
    create_ok(&app, &fx, &[&fx.owner], false, fx.done).await;
}

#[tokio::test]
async fn last_completion_fails_when_required_custom_field_is_missing() {
    let mut app = TestApp::new().await;
    let fx = setup(&mut app).await;
    let task = create_ok(&app, &fx, &[&fx.owner, &fx.b], true, fx.todo).await;
    let field = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/custom-fields",
                fx.tenant_id, fx.project_id
            ),
            json!({ "name": "Points", "field_type": "number", "is_required": true }),
        )
        .await;
    assert_eq!(field.status(), StatusCode::CREATED);

    assert_eq!(
        complete(&app, &fx, task, fx.owner.id).await.status(),
        StatusCode::OK
    );
    login(&mut app, &fx.b).await;
    let res = complete(&app, &fx, task, fx.b.id).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    // 最後の人の完了も記録されず、ステータスも動かない
    assert_eq!(completed_count(&app, task).await, 1);
    assert_eq!(task_row(&app, task).await.status_id, fx.todo);
}
