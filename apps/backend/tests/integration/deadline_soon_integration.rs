use std::sync::atomic::{AtomicI32, Ordering};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use entity::{
    notification_settings, notifications, project_statuses, task_assignees, task_watchers, tasks,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::common::{TestApp, TestUser, ensure_tenant_member_for_project};
use crate::forge_commit_links_integration::job_state;

// 期限の近いタスクの通知（`deadline_soon`、TASK-263）の統合テスト。
// 仕様は docs/dev/features/tasks/5.notifications.md の通知種別の表。
// `send_once` は DB 全体を走査するので、件数は宛先ごとに数える（他のテストのタスクに左右されない）。

static SEQ: AtomicI32 = AtomicI32::new(1);

struct Fixture {
    app: TestApp,
    project_id: Uuid,
    open: Uuid,
    done: Uuid,
    owner: TestUser,
    member: TestUser,
}

async fn setup() -> Fixture {
    let app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    // メンバー未指定のプロジェクトはテナントメンバー全員が入れる
    ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    let open = status(&app, tp.project_id, false).await;
    let done = status(&app, tp.project_id, true).await;
    Fixture {
        app,
        project_id: tp.project_id,
        open,
        done,
        owner,
        member,
    }
}

async fn status(app: &TestApp, project_id: Uuid, done: bool) -> Uuid {
    let id = Uuid::new_v4();
    project_statuses::ActiveModel {
        id: Set(id),
        project_id: Set(project_id),
        name: Set(if done { "Done" } else { "Todo" }.into()),
        color: Set("#123456".into()),
        position: Set(i16::from(done)),
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

async fn task(
    fx: &Fixture,
    status_id: Uuid,
    soft: Option<DateTime<Utc>>,
    hard: Option<DateTime<Utc>>,
) -> tasks::Model {
    let now = Utc::now();
    tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(fx.project_id),
        seq_id: Set(SEQ.fetch_add(1, Ordering::Relaxed)),
        title: Set("締切のある作業".into()),
        description: Set(None),
        status_id: Set(status_id),
        priority: Set(tasks::TaskPriority::Medium),
        progress_pct: Set(0),
        parent_task_id: Set(None),
        milestone_id: Set(None),
        soft_deadline: Set(soft.map(Into::into)),
        hard_deadline: Set(hard.map(Into::into)),
        planned_start_on: Set(None),
        require_all_assignees_done: Set(false),
        estimated_minutes: Set(None),
        is_archived: Set(false),
        created_by: Set(fx.owner.id),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        deleted_at: Set(None),
        sprint_id: Set(None),
        completed_at: Set(None),
    }
    .insert(&fx.app.state.db)
    .await
    .unwrap()
}

async fn assign(fx: &Fixture, task_id: Uuid, user_id: Uuid) {
    task_assignees::ActiveModel {
        id: Set(Uuid::new_v4()),
        task_id: Set(task_id),
        user_id: Set(user_id),
        role: Set("primary".into()),
        assigned_at: Set(Utc::now().into()),
        completed_at: Set(None),
    }
    .insert(&fx.app.state.db)
    .await
    .unwrap();
}

async fn watch(fx: &Fixture, task_id: Uuid, user_id: Uuid) {
    task_watchers::ActiveModel {
        task_id: Set(task_id),
        user_id: Set(user_id),
        created_at: Set(Utc::now().into()),
    }
    .insert(&fx.app.state.db)
    .await
    .unwrap();
}

/// その利用者の `deadline_soon` 通知（作成順）。
async fn deadline_notices(fx: &Fixture, user_id: Uuid) -> Vec<notifications::Model> {
    notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(user_id))
        .filter(notifications::Column::NotificationType.eq("deadline_soon"))
        .all(&fx.app.state.db)
        .await
        .unwrap()
}

async fn send(fx: &Fixture) {
    job::deadline_soon::send_once(&job_state(&fx.app))
        .await
        .expect("deadline_soon sweep");
}

fn rfc3339(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// 担当者とウォッチャーに 1 件ずつ届き、同じ期限では増えず、期限を変えると送り直す。
#[tokio::test]
async fn notifies_assignee_and_watcher_once_per_deadline() {
    let fx = setup().await;
    let soft = Utc::now() + Duration::hours(12);
    let t = task(&fx, fx.open, Some(soft), None).await;
    // 担当者は通常ウォッチャーも兼ねる。和集合なので 1 件
    assign(&fx, t.id, fx.member.id).await;
    watch(&fx, t.id, fx.member.id).await;
    watch(&fx, t.id, fx.owner.id).await;

    send(&fx).await;

    let member = deadline_notices(&fx, fx.member.id).await;
    assert_eq!(member.len(), 1, "担当者に 1 件");
    assert_eq!(member[0].task_id, Some(t.id));
    assert_eq!(member[0].project_id, Some(fx.project_id));
    let payload = &member[0].payload;
    assert_eq!(payload["kind"], "soft");
    assert_eq!(payload["task_id"], t.id.to_string());
    assert_eq!(payload["title"], "締切のある作業");
    let stored = tasks::Entity::find_by_id(t.id)
        .one(&fx.app.state.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        payload["deadline"],
        rfc3339(stored.soft_deadline.unwrap().into())
    );
    assert!(
        payload["seq_key"]
            .as_str()
            .is_some_and(|k| k.ends_with(&format!("-{}", t.seq_id))),
        "seq_key: {}",
        payload["seq_key"]
    );
    assert_eq!(
        deadline_notices(&fx, fx.owner.id).await.len(),
        1,
        "ウォッチャーに 1 件"
    );

    // 同じ期限では送り直さない
    send(&fx).await;
    assert_eq!(deadline_notices(&fx, fx.member.id).await.len(), 1);
    assert_eq!(deadline_notices(&fx, fx.owner.id).await.len(), 1);

    // 期限を変えたら送り直す
    let mut active: tasks::ActiveModel = stored.into();
    active.soft_deadline = Set(Some((Utc::now() + Duration::hours(6)).into()));
    active.update(&fx.app.state.db).await.unwrap();
    send(&fx).await;
    assert_eq!(deadline_notices(&fx, fx.member.id).await.len(), 2);
    assert_eq!(deadline_notices(&fx, fx.owner.id).await.len(), 2);
}

/// 完了・アーカイブ・論理削除・24 時間より先・期限切れのタスクには送らない。
#[tokio::test]
async fn skips_tasks_outside_the_target() {
    let fx = setup().await;
    let now = Utc::now();
    let soon = Some(now + Duration::hours(12));

    let done = task(&fx, fx.done, soon, None).await;
    let archived = task(&fx, fx.open, soon, None).await;
    let mut active: tasks::ActiveModel = archived.clone().into();
    active.is_archived = Set(true);
    active.update(&fx.app.state.db).await.unwrap();
    let deleted = task(&fx, fx.open, None, soon).await;
    let mut active: tasks::ActiveModel = deleted.clone().into();
    active.deleted_at = Set(Some(now.into()));
    active.update(&fx.app.state.db).await.unwrap();
    let far = task(&fx, fx.open, Some(now + Duration::hours(48)), None).await;
    let past = task(&fx, fx.open, None, Some(now - Duration::hours(1))).await;
    for t in [&done, &archived, &deleted, &far, &past] {
        assign(&fx, t.id, fx.member.id).await;
    }
    // 対照: 同じ担当者でも対象のタスクには届く。24 時間より先の仮締めの分は送らない
    let target = task(&fx, fx.open, Some(now + Duration::hours(48)), soon).await;
    assign(&fx, target.id, fx.member.id).await;

    send(&fx).await;

    let got = deadline_notices(&fx, fx.member.id).await;
    assert_eq!(got.len(), 1, "対象のタスクの分だけ: {got:?}");
    assert_eq!(got[0].task_id, Some(target.id));
    assert_eq!(got[0].payload["kind"], "hard");
}

/// アプリ内通知で `deadline_soon` を外した利用者と、プロジェクトに入れないウォッチャーには送らない。
#[tokio::test]
async fn respects_in_app_settings_and_project_access() {
    let fx = setup().await;
    let outsider = fx.app.insert_user_default().await;
    notification_settings::ActiveModel {
        user_id: Set(fx.member.id),
        project_id: Set(fx.project_id),
        email_events: Set(vec![]),
        in_app_events: Set(vec!["assigned".into(), "status_changed".into()]),
    }
    .insert(&fx.app.state.db)
    .await
    .unwrap();

    let t = task(&fx, fx.open, Some(Utc::now() + Duration::hours(12)), None).await;
    assign(&fx, t.id, fx.member.id).await;
    watch(&fx, t.id, outsider.id).await;
    // 対照: 既定の設定のテナントオーナーには届く
    watch(&fx, t.id, fx.owner.id).await;

    send(&fx).await;

    assert!(deadline_notices(&fx, fx.member.id).await.is_empty());
    assert!(deadline_notices(&fx, outsider.id).await.is_empty());
    assert_eq!(deadline_notices(&fx, fx.owner.id).await.len(), 1);
}

/// 他のインスタンスがロックを持っている間は走査せずに 0 を返し、そのロックを消さない。
/// 明けたら送り、終わったら自分のロックを解放する。
#[tokio::test]
async fn skips_while_another_instance_holds_the_lock() {
    use job::deadline_soon::LOCK_KEY;

    let fx = setup().await;
    let t = task(&fx, fx.open, Some(Utc::now() + Duration::hours(12)), None).await;
    assign(&fx, t.id, fx.member.id).await;
    let state = job_state(&fx.app);
    lock_cmd(
        &fx,
        redis::cmd("SET")
            .arg(LOCK_KEY)
            .arg("other-instance")
            .arg("EX")
            .arg(60),
    )
    .await;

    assert_eq!(job::deadline_soon::send_once(&state).await.unwrap(), 0);
    assert!(deadline_notices(&fx, fx.member.id).await.is_empty());
    assert_eq!(
        lock_cmd(&fx, redis::cmd("GET").arg(LOCK_KEY)).await,
        redis::Value::BulkString(b"other-instance".to_vec()),
        "他のインスタンスのロックは消さない"
    );

    // 対照: ロックが明けたら送る
    lock_cmd(&fx, redis::cmd("DEL").arg(LOCK_KEY)).await;
    assert!(job::deadline_soon::send_once(&state).await.unwrap() >= 1);
    assert_eq!(deadline_notices(&fx, fx.member.id).await.len(), 1);
    assert_eq!(
        lock_cmd(&fx, redis::cmd("GET").arg(LOCK_KEY)).await,
        redis::Value::Nil,
        "終わったらロックを解放する"
    );
}

async fn lock_cmd(fx: &Fixture, cmd: &mut redis::Cmd) -> redis::Value {
    let mut conn = fx.app.state.redis_client.conn.acquire().await.unwrap();
    cmd.query_async(&mut conn).await.unwrap()
}

/// 仮締めと Deadline が両方 24 時間以内なら、kind 違いで 2 件。
#[tokio::test]
async fn soft_and_hard_are_separate_notifications() {
    let fx = setup().await;
    let now = Utc::now();
    let t = task(
        &fx,
        fx.open,
        Some(now + Duration::hours(6)),
        Some(now + Duration::hours(20)),
    )
    .await;
    assign(&fx, t.id, fx.member.id).await;

    send(&fx).await;

    let mut kinds: Vec<String> = deadline_notices(&fx, fx.member.id)
        .await
        .iter()
        .map(|n| n.payload["kind"].as_str().unwrap().to_string())
        .collect();
    kinds.sort();
    assert_eq!(kinds, ["hard", "soft"]);
}
