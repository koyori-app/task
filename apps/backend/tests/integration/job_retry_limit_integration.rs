//! 失敗し続けるジョブの実際の試行の回数が `max_attempts` を越えぬことを、実 Postgres のワーカーで確かめる。
//!
//! apalis 1.x の `max_attempts` は行を取り直す条件にしか使われず、`RetryPolicy` はこれを見ない
//! （1.0.0-rc.8 までは、各試行の後に上限へ達した行を Killed にして止めた）。`job::retry_policy` が
//! 上限で打ち切らなければ、`retries(n)` の n が許す n + 1 回まで走り、最後の失敗を Sentry へ送った後に
//! 成功して、報告と結果が食い違うこともある。
//!
//! `with_captured_events` の Hub はスレッドに紐づくので、current_thread のランタイムでワーカーも同じ
//! スレッドで動かす（`sentry_job_failure_integration` と同じ）。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use apalis::layers::WorkerBuilderExt;
use apalis::prelude::{BoxDynError, Data, TaskBuilder, TaskSink, WorkerBuilder};
use apalis_postgres::{Config, PostgresStorage};
use job::AttemptLimit;
use job::failure_report::JobFailureReportLayer;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use sentry::protocol::Event;
use uuid::Uuid;

use crate::common::TestApp;

type ProbeStorage = PostgresStorage<String>;
type Runs = Arc<AtomicUsize>;

const QUEUE_LABEL: &str = "retry-limit-probe";
/// 使い切っていない行は、ワーカーの poll の間隔を挟んで取り直される
const DEADLINE: Duration = Duration::from_secs(120);
/// 上限を越えて走るなら、この間に次の試行が走る（再試行は待たずに即座に走る）
const SETTLE: Duration = Duration::from_secs(3);

/// 試行を数える。上限を越えて走れば成功するので、越えたかどうかが結果にも表れる。
async fn fails_until_beyond(
    _email: String,
    runs: Data<Runs>,
    limit: Data<usize>,
) -> Result<(), BoxDynError> {
    let nth = runs.fetch_add(1, Ordering::SeqCst) + 1;
    if nth <= *limit {
        Err(format!("550 mailbox unavailable (run {nth})").into())
    } else {
        Ok(())
    }
}

async fn status_of(db: &DatabaseConnection, queue: &str) -> (String, i32, i32) {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT status, attempts, max_attempts FROM apalis.jobs WHERE job_type = $1",
            [queue.into()],
        ))
        .await
        .expect("select job")
        .expect("job row");
    (
        row.try_get("", "status").expect("status"),
        row.try_get("", "attempts").expect("attempts"),
        row.try_get("", "max_attempts").expect("max_attempts"),
    )
}

struct Outcome {
    runs: usize,
    status: String,
    attempts: i32,
    events: Vec<Event<'static>>,
}

/// 最初の `max_attempts` 回の失敗のあとなら成功するジョブを、`policy` で動かす。
/// 上限で止まれば成功する試行は走らず、行は Failed で残る。
fn run_failing_job(
    policy: AttemptLimit<apalis::layers::retry::RetryPolicy>,
    max_attempts: usize,
) -> Outcome {
    let runs: Runs = Arc::default();
    let mut outcome = None;
    let events = sentry::test::with_captured_events(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(async {
                let app = TestApp::new().await;
                let queue = format!("retry_limit_{}", Uuid::new_v4().simple());
                let mut storage: ProbeStorage = PostgresStorage::new(&app.state.pg_pool)
                    .with_config(Config::default().queue(&queue));
                storage
                    .push_task(
                        TaskBuilder::new("victim@example.com".to_owned())
                            .max_attempts(max_attempts)
                            .build(),
                    )
                    .await
                    .expect("push");

                let worker = WorkerBuilder::new(format!("{queue}-worker"))
                    .backend(job::worker_backend(&storage, Duration::from_millis(100)))
                    .data(runs.clone())
                    .data(max_attempts)
                    .retry(policy)
                    .layer(JobFailureReportLayer::new(QUEUE_LABEL))
                    .build(fails_until_beyond);
                let handle = tokio::spawn(worker.run());

                // 使い切って Failed のまま残るか、上限を越えて成功（Done）するまで待つ
                let (status, attempts) = tokio::time::timeout(DEADLINE, async {
                    loop {
                        let (status, attempts, max) = status_of(&app.state.db, &queue).await;
                        if status == "Done" || (status == "Failed" && attempts >= max) {
                            return (status, attempts);
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                })
                .await
                .expect("job did not settle");
                tokio::time::sleep(SETTLE).await;
                let (status_after, attempts_after, _) = status_of(&app.state.db, &queue).await;
                assert_eq!(
                    (status.as_str(), attempts),
                    (status_after.as_str(), attempts_after)
                );
                handle.abort();
                outcome = Some((status, attempts));
            });
    });
    let (status, attempts) = outcome.expect("outcome");
    Outcome {
        runs: runs.load(Ordering::SeqCst),
        status,
        attempts,
        events,
    }
}

fn assert_stopped_at(outcome: &Outcome, max_attempts: usize) {
    assert_eq!(outcome.runs, max_attempts, "試行の回数");
    assert_eq!(
        outcome.status, "Failed",
        "上限で止まれば、成功する試行は走らない"
    );
    assert_eq!(usize::try_from(outcome.attempts).ok(), Some(max_attempts));
    assert_eq!(outcome.events.len(), 1, "{:?}", outcome.events);
}

/// 本番の `MAX_RETRIES = 8` だった認証メールなどの形（再試行の回数に上限が無い方針）
#[test]
fn retries_stop_at_max_attempts() {
    let outcome = run_failing_job(job::retry_policy(), job::DEFAULT_MAX_ATTEMPTS);
    assert_stopped_at(&outcome, job::DEFAULT_MAX_ATTEMPTS);
}

/// 1 回の取得の再試行を絞った方針（レビュー要約）。取り直しで続きが走っても、上限は越えない
#[test]
fn refetched_retries_stop_at_max_attempts() {
    let outcome = run_failing_job(job::retry_policy_within(1), job::DEFAULT_MAX_ATTEMPTS);
    assert_stopped_at(&outcome, job::DEFAULT_MAX_ATTEMPTS);
}

/// 上限はタスクごとの値に従う（定数ではない）
#[test]
fn retries_follow_the_task_max_attempts() {
    let outcome = run_failing_job(job::retry_policy(), 3);
    assert_stopped_at(&outcome, 3);
}

/// 本番のワーカーの組み立て（`server.rs`）が、再試行を `max_attempts` で打ち切る方針で組まれていること。
/// 生の `RetryPolicy::retries(n)` に戻すと、n + 1 回まで上限を越えて走る。ワーカーは起動時に
/// 組むのでこの試験からは動かせず、組み立ての文面で縛る。
#[test]
fn production_workers_use_the_attempt_limit() {
    let server = include_str!("../../src/server.rs");
    let retries = server.matches(".retry(").count();
    let limited = server.matches(".retry(job::retry_policy").count();
    assert_eq!(retries, 7, "ワーカーの数");
    assert_eq!(limited, retries, "上限で打ち切らない再試行が在る");
    assert!(!server.contains("RetryPolicy::retries"));
}
