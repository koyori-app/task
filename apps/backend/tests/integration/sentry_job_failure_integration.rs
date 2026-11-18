//! ジョブの最終的な失敗だけが Sentry へ届くことを、実 Postgres のワーカーで確かめる。
//!
//! `with_captured_events` の Hub はスレッドに紐づくので、current_thread のランタイムで
//! ワーカーも同じスレッドで動かす。

use std::time::Duration;

use apalis::layers::WorkerBuilderExt;
use apalis::layers::retry::RetryPolicy;
use apalis::prelude::{BoxDynError, TaskBuilder, TaskSink, WorkerBuilder};
use apalis_postgres::{Config, PostgresStorage};
use backend::observability;
use job::AttemptLimit;
use job::failure_report::JobFailureReportLayer;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use sentry::protocol::Event;
use tracing_subscriber::layer::SubscriberExt;
use uuid::Uuid;

use crate::common::TestApp;

type ProbeStorage = PostgresStorage<String>;

const QUEUE_LABEL: &str = "sentry-probe";
const VICTIM: &str = "victim@example.com";
/// 使い切っていない行は、ワーカーの poll の間隔（空振りが続くと最大 60 秒まで伸びる）を挟んで取り直される
const DEADLINE: Duration = Duration::from_secs(180);
/// 使い切った後も回り続けるリトライが、重複して報告しないことを見る長さ
const SETTLE: Duration = Duration::from_secs(3);
/// 試験ごとの見張り。上限で打ち切られないリトライは休まず回って tokio のタイマーを止めるので、
/// `DEADLINE` の timeout は効かない。正常の最長（初回の schema 同期込みで 1 分余り）の三倍ほど
const WATCHDOG: Duration = Duration::from_secs(240);

fn probe_storage(app: &TestApp, queue: &str) -> ProbeStorage {
    PostgresStorage::new(&app.state.pg_pool).with_config(Config::default().queue(queue))
}

async fn always_fails(email: String) -> Result<(), BoxDynError> {
    Err(format!("550 mailbox unavailable <{email}>").into())
}

async fn succeeds(_email: String) -> Result<(), BoxDynError> {
    Ok(())
}

async fn job_status(db: &DatabaseConnection, queue: &str) -> String {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT status FROM apalis.jobs WHERE job_type = $1",
            [queue.into()],
        ))
        .await
        .expect("select job")
        .expect("job row");
    row.try_get("", "status").expect("status")
}

/// 試行を使い切った行（apalis-postgres 1.0.0-rc.9 以降は `Failed` のまま残り、`Killed` にはならない）
async fn is_exhausted(db: &DatabaseConnection, queue: &str) -> bool {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT status, attempts, max_attempts FROM apalis.jobs WHERE job_type = $1",
            [queue.into()],
        ))
        .await
        .expect("select job")
        .expect("job row");
    let status: String = row.try_get("", "status").expect("status");
    let attempts: i32 = row.try_get("", "attempts").expect("attempts");
    let max_attempts: i32 = row.try_get("", "max_attempts").expect("max_attempts");
    status == "Failed" && attempts >= max_attempts
}

async fn wait_for_exhausted(db: &DatabaseConnection, queue: &str) {
    tokio::time::timeout(DEADLINE, async {
        loop {
            if is_exhausted(db, queue).await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("job did not use up its attempts"));
}

async fn wait_for_status(db: &DatabaseConnection, queue: &str, expected: &str) {
    tokio::time::timeout(DEADLINE, async {
        loop {
            if job_status(db, queue).await == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("job did not reach {expected}"));
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

/// 失敗し続けるジョブを `policy` のリトライで最後まで動かし、送られたイベントを返す
fn events_of_failing_job(
    policy: AttemptLimit<RetryPolicy>,
    max_attempts: usize,
) -> Vec<Event<'static>> {
    let _watchdog = crate::common::watchdog("失敗し続けるジョブの試行の使い切り", WATCHDOG);
    sentry::test::with_captured_events(|| {
        let subscriber = tracing_subscriber::registry().with(observability::tracing_layer());
        tracing::subscriber::with_default(subscriber, || {
            runtime().block_on(async {
                let app = TestApp::new().await;
                let queue = format!("sentry_fail_{}", Uuid::new_v4().simple());
                let mut storage = probe_storage(&app, &queue);
                storage
                    .push_task(
                        TaskBuilder::new(VICTIM.to_owned())
                            .max_attempts(max_attempts)
                            .build(),
                    )
                    .await
                    .expect("push");

                let worker = WorkerBuilder::new(format!("{queue}-worker"))
                    .backend(job::worker_backend(&storage, Duration::from_millis(100)))
                    .retry(policy)
                    .layer(JobFailureReportLayer::new(QUEUE_LABEL))
                    .enable_tracing()
                    .build(always_fails);
                let handle = tokio::spawn(worker.run());

                wait_for_exhausted(&app.state.db, &queue).await;
                tokio::time::sleep(SETTLE).await;
                handle.abort();
            });
        });
    })
}

fn assert_single_failure_report(events: &[Event<'static>]) {
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        events[0].message.as_deref(),
        Some("apalis job failed after retries: sentry-probe")
    );
    assert_eq!(
        events[0].tags.get("job.queue").map(String::as_str),
        Some(QUEUE_LABEL)
    );
    let rendered = serde_json::to_string(&events[0]).expect("serialize event");
    assert!(!rendered.contains(VICTIM), "{rendered}");
}

/// 再試行の回数に上限を置かない方針（認証メールなど）: 試行を使い切る最後の試行の失敗を 1 件だけ送る
#[test]
fn exhausted_retries_report_once() {
    // 本番のジョブと同じ上限（`job::DEFAULT_MAX_ATTEMPTS`）
    assert_single_failure_report(&events_of_failing_job(
        job::retry_policy(),
        job::DEFAULT_MAX_ATTEMPTS,
    ));
}

/// リトライが max_attempts に満たないキュー: 使い切っても Failed で再取得されるので、
/// 試行を使い切る最後の試行まで送らない
#[test]
fn refetched_job_reports_only_when_killed() {
    // 1 回の取得で 2 回試し、2 回目の取得で 3 回目の試行が最後になる
    assert_single_failure_report(&events_of_failing_job(job::retry_policy_within(1), 3));
}

#[test]
fn successful_job_reports_nothing() {
    let _watchdog = crate::common::watchdog("成功するジョブの Done", WATCHDOG);
    let events = sentry::test::with_captured_events(|| {
        runtime().block_on(async {
            let app = TestApp::new().await;
            let queue = format!("sentry_ok_{}", Uuid::new_v4().simple());
            let mut storage = probe_storage(&app, &queue);
            storage
                .push_task(job::new_task(VICTIM.to_owned()))
                .await
                .expect("push");

            let worker = WorkerBuilder::new(format!("{queue}-worker"))
                .backend(job::worker_backend(&storage, Duration::from_millis(100)))
                .retry(job::retry_policy_within(1))
                .layer(JobFailureReportLayer::new(QUEUE_LABEL))
                .build(succeeds);
            let handle = tokio::spawn(worker.run());

            wait_for_status(&app.state.db, &queue, "Done").await;
            handle.abort();
        });
    });

    assert!(events.is_empty(), "{events:?}");
}
