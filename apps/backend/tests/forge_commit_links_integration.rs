//! push webhook → コミットのリンクとアクティビティ（TASK-140、docs/features/tasks/9.github-tasks.md §5）。

mod common;

use common::{TestApp, TestTenantProject};
use entity::{
    forge_commit_links, forge_commits, forge_webhook_deliveries, github_integrations,
    project_statuses, projects, task_activities, tasks,
};
use hmac::{Hmac, KeyInit, Mac};
use job::github_webhook::{GithubWebhookJob, QUEUE_NAME};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait,
    PaginatorTrait, QueryFilter, Statement,
};
use service::forge::commits::{COMMIT_LINKED_EVENT, apply_push};
use service::forge::events::{ForgeCommit, ForgeEvent, ForgeRepo};
use sha2::Sha256;
use uuid::Uuid;

/// `load_github_test_env` が設定する webhook シークレット
const WEBHOOK_SECRET: &str = "webhook-secret";
const REPO_OWNER: &str = "acme";
const REPO_NAME: &str = "backend";

fn sign(body: &[u8]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(WEBHOOK_SECRET.as_bytes()).expect("HMAC key of any size");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

fn unique_installation_id() -> i64 {
    500_000_000_000_i64 + (Uuid::new_v4().as_u128() % 400_000_000_000) as i64
}

fn sha() -> String {
    format!("{:x}", Uuid::new_v4().as_u128())
        .chars()
        .chain(std::iter::repeat('0'))
        .take(40)
        .collect()
}

struct Fixture {
    owner_id: Uuid,
    tp: TestTenantProject,
    key: String,
    status_id: Uuid,
    installation_id: i64,
}

async fn setup(app: &TestApp) -> Fixture {
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let key = projects::Entity::find_by_id(tp.project_id)
        .one(&app.state.db)
        .await
        .expect("query project")
        .expect("project row")
        .key;
    let status_id = insert_status(app, tp.project_id).await;
    let installation_id = unique_installation_id();
    github_integrations::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(tp.project_id),
        installation_id: Set(installation_id),
        repo_owner: Set(REPO_OWNER.into()),
        repo_name: Set(REPO_NAME.into()),
        access_token_enc: Set("unused".into()),
        token_expires_at: Set(chrono::Utc::now().into()),
        created_by: Set(owner.id),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert integration");
    Fixture {
        owner_id: owner.id,
        tp,
        key,
        status_id,
        installation_id,
    }
}

async fn insert_status(app: &TestApp, project_id: Uuid) -> Uuid {
    project_statuses::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        name: Set("Todo".into()),
        color: Set("#888888".into()),
        position: Set(0),
        is_default: Set(true),
        is_done_state: Set(false),
        is_default_done: Set(false),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert status")
    .id
}

/// テナントに 2 つ目のプロジェクトを足す（キーはテナント内で一意）。
async fn insert_project(app: &TestApp, tenant_id: Uuid, key: &str) -> Uuid {
    projects::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set("second".into()),
        description: Set(String::new()),
        tenant_id: Set(tenant_id),
        icon_emoji: Set(None),
        icon_url: Set(None),
        key: Set(key.into()),
        is_personal: Set(false),
        personal_owner_id: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert project")
    .id
}

async fn insert_task(
    app: &TestApp,
    project_id: Uuid,
    status_id: Uuid,
    created_by: Uuid,
    seq_id: i32,
) -> Uuid {
    let now = chrono::Utc::now();
    tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        seq_id: Set(seq_id),
        title: Set(format!("task {seq_id}")),
        description: Set(None),
        status_id: Set(status_id),
        priority: Set(tasks::TaskPriority::Medium),
        progress_pct: Set(0),
        parent_task_id: Set(None),
        milestone_id: Set(None),
        sprint_id: Set(None),
        soft_deadline: Set(None),
        hard_deadline: Set(None),
        estimated_minutes: Set(None),
        is_archived: Set(false),
        created_by: Set(created_by),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        completed_at: Set(None),
        deleted_at: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert task")
    .id
}

fn push_payload(installation_id: i64, commits: &[(&str, &str)]) -> serde_json::Value {
    serde_json::json!({
        "ref": "refs/heads/main",
        "forced": false,
        "repository": { "name": REPO_NAME, "owner": { "login": REPO_OWNER } },
        "installation": { "id": installation_id },
        "sender": { "login": "yupix", "id": 20 },
        "commits": commits.iter().map(|(sha, message)| serde_json::json!({
            "id": sha,
            "message": message,
            "timestamp": "2026-09-10T03:00:00Z",
            "url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/commit/{sha}"),
            "author": { "name": "Yupix", "email": "yupix@example.com", "username": "yupix" },
        })).collect::<Vec<_>>(),
    })
}

fn repo() -> ForgeRepo {
    ForgeRepo {
        host: "github".into(),
        host_url: "https://github.com".into(),
        repo_owner: REPO_OWNER.into(),
        repo_name: REPO_NAME.into(),
    }
}

fn commit(message: &str) -> ForgeCommit {
    let sha = sha();
    ForgeCommit {
        html_url: format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/commit/{sha}"),
        sha,
        message: message.into(),
        author_handle: "yupix".into(),
        author_name: "Yupix".into(),
        committed_at: chrono::Utc::now(),
    }
}

async fn post_push(
    app: &TestApp,
    delivery_id: &str,
    body: &serde_json::Value,
    signature: Option<&str>,
) -> u16 {
    let body = serde_json::to_vec(body).expect("serialize body");
    let signature = signature.map_or_else(|| sign(&body), str::to_owned);
    app.client()
        .post(format!("{}/v1/github/webhook", app.base_url()))
        .header("content-type", "application/json")
        .header("X-GitHub-Event", "push")
        .header("X-GitHub-Delivery", delivery_id)
        .header("X-Hub-Signature-256", signature)
        .body(body)
        .send()
        .await
        .expect("post webhook")
        .status()
        .as_u16()
}

async fn delivery_rows(app: &TestApp, delivery_id: &str) -> u64 {
    forge_webhook_deliveries::Entity::find()
        .filter(forge_webhook_deliveries::Column::DeliveryId.eq(delivery_id))
        .count(&app.state.db)
        .await
        .expect("count deliveries")
}

/// このプロジェクト宛てに積まれた webhook ジョブ（apalis.jobs.job は JSON の bytea）。
async fn queued_jobs(app: &TestApp, project_id: Uuid) -> Vec<GithubWebhookJob> {
    app.state
        .db
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT convert_from(job, 'UTF8') AS job FROM apalis.jobs \
             WHERE job_type = $1 AND convert_from(job, 'UTF8') LIKE $2 \
             ORDER BY run_at, id",
            [QUEUE_NAME.into(), format!("%{project_id}%").into()],
        ))
        .await
        .expect("query jobs")
        .iter()
        .map(|row| {
            let raw: String = row.try_get("", "job").expect("job column");
            serde_json::from_str(&raw).expect("decode job")
        })
        .collect()
}

/// ワーカーの代わりにジョブを処理する（処理本体はワーカーと同じ関数）。
async fn run_jobs(app: &TestApp, jobs: &[GithubWebhookJob]) {
    for job in jobs {
        let Some(ForgeEvent::Push { repo, commits, .. }) = &job.forge_event else {
            panic!("push ジョブは正規化イベントを持つ");
        };
        apply_push(&app.state.db, job.project_id, repo, commits)
            .await
            .expect("apply push");
    }
}

async fn project_commit_rows(app: &TestApp, project_id: Uuid) -> u64 {
    forge_commits::Entity::find()
        .filter(forge_commits::Column::ProjectId.eq(project_id))
        .count(&app.state.db)
        .await
        .expect("count commits")
}

async fn link_rows(app: &TestApp, task_id: Uuid) -> u64 {
    forge_commit_links::Entity::find()
        .filter(forge_commit_links::Column::TaskId.eq(task_id))
        .count(&app.state.db)
        .await
        .expect("count links")
}

async fn commit_activities(app: &TestApp, task_id: Uuid) -> Vec<task_activities::Model> {
    task_activities::Entity::find()
        .filter(task_activities::Column::TaskId.eq(task_id))
        .filter(task_activities::Column::EventType.eq(COMMIT_LINKED_EVENT))
        .all(&app.state.db)
        .await
        .expect("query activities")
}

/// 署名済みの push が受信記録 1 行・ジョブ 1 件になり、処理するとコミットがタスクに結ばれる。
/// 同じ配信の再送・ジョブの再試行・別配信での同じコミットでは行もアクティビティも増えない。
#[tokio::test]
async fn push_webhook_links_commit_once() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let task_id = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;

    let sha = sha();
    let message = format!("fix: ログイン修正 {}-1\n\n本文", fx.key);
    let body = push_payload(fx.installation_id, &[(sha.as_str(), message.as_str())]);
    let delivery = Uuid::new_v4().to_string();

    assert_eq!(post_push(&app, &delivery, &body, None).await, 200);
    assert_eq!(delivery_rows(&app, &delivery).await, 1);
    assert_eq!(queued_jobs(&app, fx.tp.project_id).await.len(), 1);

    // ホストの再送（同じ配信 ID）は受け取るが何も積まない
    assert_eq!(post_push(&app, &delivery, &body, None).await, 200);
    assert_eq!(delivery_rows(&app, &delivery).await, 1);
    let jobs = queued_jobs(&app, fx.tp.project_id).await;
    assert_eq!(jobs.len(), 1, "同じ配信の再送でジョブを積まない");
    assert!(
        jobs[0].payload.is_null(),
        "正規化したイベントのジョブにホストのペイロードを残さない"
    );

    run_jobs(&app, &jobs).await;
    assert_eq!(project_commit_rows(&app, fx.tp.project_id).await, 1);
    assert_eq!(link_rows(&app, task_id).await, 1);
    let activities = commit_activities(&app, task_id).await;
    assert_eq!(activities.len(), 1);
    let commit_row = forge_commits::Entity::find()
        .filter(forge_commits::Column::ProjectId.eq(fx.tp.project_id))
        .one(&app.state.db)
        .await
        .expect("query commit")
        .expect("commit row");
    assert_eq!(commit_row.sha, sha);
    assert_eq!(commit_row.message, message, "実体はメッセージ全文を持つ");
    let activity = &activities[0];
    assert_eq!(activity.user_id, None);
    assert_eq!(
        activity.dedupe_key.as_deref(),
        Some(format!("{COMMIT_LINKED_EVENT}:{}:{task_id}", commit_row.id).as_str())
    );
    assert_eq!(
        activity.payload,
        serde_json::json!({
            "host": "github",
            "sha": sha,
            "message": format!("fix: ログイン修正 {}-1", fx.key),
            "author_handle": "yupix",
            "author_name": "Yupix",
            "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/commit/{sha}"),
        })
    );

    // ワーカーの再試行
    run_jobs(&app, &jobs).await;
    // 同じコミットを含む別の配信（別ブランチへの push など）
    let other_delivery = Uuid::new_v4().to_string();
    assert_eq!(post_push(&app, &other_delivery, &body, None).await, 200);
    let jobs = queued_jobs(&app, fx.tp.project_id).await;
    assert_eq!(jobs.len(), 2, "別の配信は初回として積む");
    run_jobs(&app, &jobs).await;

    assert_eq!(project_commit_rows(&app, fx.tp.project_id).await, 1);
    assert_eq!(link_rows(&app, task_id).await, 1);
    assert_eq!(commit_activities(&app, task_id).await.len(), 1);

    app.cleanup_user(fx.owner_id).await;
}

/// 署名が不正な配信は受信記録もジョブも残さない。同じ配信 ID の正当な配信は初回として積まれる
/// （先取りされた配信 ID で本物が重複扱いにならない）。
#[tokio::test]
async fn invalid_signature_leaves_no_delivery_record() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let sha = sha();
    let message = format!("{}-1", fx.key);
    let body = push_payload(fx.installation_id, &[(sha.as_str(), message.as_str())]);
    let delivery = Uuid::new_v4().to_string();

    assert_eq!(
        post_push(&app, &delivery, &body, Some("sha256=deadbeef")).await,
        403
    );
    assert_eq!(delivery_rows(&app, &delivery).await, 0);
    assert!(queued_jobs(&app, fx.tp.project_id).await.is_empty());

    assert_eq!(post_push(&app, &delivery, &body, None).await, 200);
    assert_eq!(delivery_rows(&app, &delivery).await, 1);
    assert_eq!(queued_jobs(&app, fx.tp.project_id).await.len(), 1);

    app.cleanup_user(fx.owner_id).await;
}

/// 複数コミット・複数参照はそれぞれリンクし、同じテナントの別プロジェクトにも結ぶ。
/// キーの無いコミット、存在しないタスク、別テナントのキーはリンクしない。
#[tokio::test]
async fn push_links_each_reference_within_the_tenant() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let task1 = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let task2 = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;

    // 同じテナントの別プロジェクト（対照: テナント内ならプロジェクトをまたいで結ぶ）
    let sibling_key = format!("Q{}", &fx.key[1..]);
    let sibling_project = insert_project(&app, fx.tp.tenant_id, &sibling_key).await;
    let sibling_status = insert_status(&app, sibling_project).await;
    let sibling_task = insert_task(&app, sibling_project, sibling_status, fx.owner_id, 1).await;

    // 別テナント
    let other = setup(&app).await;
    let other_task = insert_task(
        &app,
        other.tp.project_id,
        other.status_id,
        other.owner_id,
        1,
    )
    .await;

    let key = &fx.key;
    let commits = vec![
        commit(&format!("feat: {key}-1")),
        commit(&format!("fix: {key}-1 Closes {key}-2 {sibling_key}-1")),
        commit("chore: キーの無いコミット"),
        commit(&format!("chore: 存在しないタスク {key}-999")),
        commit(&format!("chore: 別テナントのキー {}-1", other.key)),
    ];
    apply_push(&app.state.db, fx.tp.project_id, &repo(), &commits)
        .await
        .expect("apply push");

    assert_eq!(
        project_commit_rows(&app, fx.tp.project_id).await,
        2,
        "タスクに結ばないコミットは積まない"
    );
    assert_eq!(link_rows(&app, task1).await, 2);
    assert_eq!(link_rows(&app, task2).await, 1);
    assert_eq!(link_rows(&app, sibling_task).await, 1);
    assert_eq!(
        link_rows(&app, other_task).await,
        0,
        "別テナントには結ばない"
    );
    assert_eq!(commit_activities(&app, task1).await.len(), 2);
    assert_eq!(commit_activities(&app, task2).await.len(), 1);
    assert_eq!(commit_activities(&app, sibling_task).await.len(), 1);
    assert!(commit_activities(&app, other_task).await.is_empty());

    // 同じ push をもう一度処理しても増えない
    apply_push(&app.state.db, fx.tp.project_id, &repo(), &commits)
        .await
        .expect("apply push again");
    assert_eq!(project_commit_rows(&app, fx.tp.project_id).await, 2);
    assert_eq!(link_rows(&app, task1).await, 2);
    assert_eq!(commit_activities(&app, task1).await.len(), 2);
    assert_eq!(commit_activities(&app, task2).await.len(), 1);

    // 手で行う操作の履歴は dedupe_key が NULL のままで、同じ操作を 2 回すれば 2 行積まれる
    for _ in 0..2 {
        service::task_activities::record_activity(
            &app.state.db,
            task1,
            Some(fx.owner_id),
            "status_changed",
            serde_json::json!({ "to": "Todo" }),
        )
        .await
        .expect("record manual activity");
    }
    let manual = task_activities::Entity::find()
        .filter(task_activities::Column::TaskId.eq(task1))
        .filter(task_activities::Column::EventType.eq("status_changed"))
        .all(&app.state.db)
        .await
        .expect("query manual activities");
    assert_eq!(manual.len(), 2);
    assert!(manual.iter().all(|a| a.dedupe_key.is_none()));

    app.cleanup_user(fx.owner_id).await;
    app.cleanup_user(other.owner_id).await;
}
