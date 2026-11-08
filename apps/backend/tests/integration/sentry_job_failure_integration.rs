//! ジョブの最終的な失敗だけが Sentry へ届くことを、実 Postgres のワーカーで確かめる。
//!
//! `with_captured_events` の Hub はスレッドに紐づくので、current_thread のランタイムで
//! ワーカーも同じスレッドで動かす。

use std::time::Duration;

use apalis::layers::WorkerBuilderExt;
use apalis::layers::retry::RetryPolicy;
use apalis::prelude::{BoxDynError, TaskSink, WorkerBuilder};
use apalis_postgres::{CompactType, Config, JsonCodec, PgNotify, PostgresStorage};
use backend::observability;
use job::failure_report::JobFailureReportLayer;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use sentry::protocol::Event;
use tracing_subscriber::layer::SubscriberExt;
use uuid::Uuid;

use crate::common::TestApp;

type ProbeStorage = PostgresStorage<String, CompactType, JsonCodec<CompactType>, PgNotify>;

const QUEUE_LABEL: &str = "sentry-probe";
const VICTIM: &str = "victim@example.com";
const DEADLINE: Duration = Duration::from_secs(60);
/// Killed の後も回り続けるリトライが、重複して報告しないことを見る長さ
const SETTLE: Duration = Duration::from_secs(3);

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

/// 失敗し続けるジョブを `retries` のリトライで最後まで動かし、送られたイベントを返す
fn events_of_failing_job(retries: usize) -> Vec<Event<'static>> {
    sentry::test::with_captured_events(|| {
        let subscriber = tracing_subscriber::registry().with(observability::tracing_layer());
        tracing::subscriber::with_default(subscriber, || {
            runtime().block_on(async {
                let app = TestApp::new().await;
                let queue = format!("sentry_fail_{}", Uuid::new_v4().simple());
                let mut storage: ProbeStorage = PostgresStorage::<String>::new_with_notify(
                    &app.state.pg_pool,
                    &Config::new(&queue),
                );
                storage.push(VICTIM.to_owned()).await.expect("push");

                let worker = WorkerBuilder::new(format!("{queue}-worker"))
                    .backend(storage)
                    .retry(RetryPolicy::retries(retries))
                    .layer(JobFailureReportLayer::new(QUEUE_LABEL))
                    .enable_tracing()
                    .build(always_fails);
                let handle = tokio::spawn(worker.run());

                wait_for_status(&app.state.db, &queue, "Killed").await;
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

/// リトライが max_attempts 以上のキュー: Killed の後もリトライが終わらないので、
/// レイヤーをリトライの外に置くと 1 件も届かない
#[test]
fn exhausted_retries_report_once() {
    assert_single_failure_report(&events_of_failing_job(8));
}

/// リトライが max_attempts に満たないキュー: 使い切っても Failed で再取得されるので、
/// Killed になる最後の試行まで送らない
#[test]
fn refetched_job_reports_only_when_killed() {
    assert_single_failure_report(&events_of_failing_job(1));
}

#[test]
fn successful_job_reports_nothing() {
    let events = sentry::test::with_captured_events(|| {
        runtime().block_on(async {
            let app = TestApp::new().await;
            let queue = format!("sentry_ok_{}", Uuid::new_v4().simple());
            let mut storage: ProbeStorage = PostgresStorage::<String>::new_with_notify(
                &app.state.pg_pool,
                &Config::new(&queue),
            );
            storage.push(VICTIM.to_owned()).await.expect("push");

            let worker = WorkerBuilder::new(format!("{queue}-worker"))
                .backend(storage)
                .retry(RetryPolicy::retries(1))
                .layer(JobFailureReportLayer::new(QUEUE_LABEL))
                .build(succeeds);
            let handle = tokio::spawn(worker.run());

            wait_for_status(&app.state.db, &queue, "Done").await;
            handle.abort();
        });
    });

    assert!(events.is_empty(), "{events:?}");
}
