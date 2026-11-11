//! ジョブの最終的な失敗を Sentry へ送る tower レイヤー。
//!
//! `.enable_tracing()` の `task.failed` は試行のたびに ERROR で出るうえ、エラー文に SMTP の
//! 応答などジョブの宛先が混ざるので Sentry へ送らない（`backend::observability`）。
//! 代わりにこのレイヤーが、ジョブが Killed になる試行の失敗を 1 件だけ送る。
//!
//! `.retry()` の外側には置けない。ack は試行ごとに走り、Killed の後のリトライはロックの取得で
//! 失敗して試行回数が増えないので、`RetryPolicy::retries(n)` の n が `max_attempts` 以上だと
//! リトライが終わらず、外側へ結果が戻らない。この空回りでも同じタスクが何度も通るので、
//! 報告済みのタスク ID を覚えて 1 タスク 1 件に絞る（プロセスとワーカーの世代をまたぐ重複は除かない）。
//!
//! 送るのはキュー名・タスク ID・試行回数だけで、ジョブの引数もエラー文も載せない。
//! 原因はタスク ID を手掛かりに、ログの `task.failed` と `apalis.jobs.last_result` で追う。

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use apalis::prelude::Task;
use apalis_postgres::PgContext;
use tower::{Layer, Service};

/// ワーカーの組み立てで `.retry(..)` の後に `.layer(..)` する（後に足したものが内側になる）。
#[derive(Clone, Debug)]
pub struct JobFailureReportLayer {
    queue: &'static str,
    reported: ReportedTasks,
}

impl JobFailureReportLayer {
    pub fn new(queue: &'static str) -> Self {
        Self {
            queue,
            reported: ReportedTasks::default(),
        }
    }
}

impl<S> Layer<S> for JobFailureReportLayer {
    type Service = JobFailureReport<S>;

    fn layer(&self, inner: S) -> Self::Service {
        JobFailureReport {
            inner,
            queue: self.queue,
            reported: self.reported.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct JobFailureReport<S> {
    inner: S,
    queue: &'static str,
    reported: ReportedTasks,
}

/// 覚えておく報告済みタスクの数。同時に走る連なりを除ければよいので多くは要らない
const REPORTED_CAPACITY: usize = 256;

/// 報告済みのタスク ID（古いものから忘れる）
#[derive(Clone, Debug, Default)]
struct ReportedTasks(Arc<Mutex<VecDeque<String>>>);

impl ReportedTasks {
    /// 初めて見るタスクなら記録して true を返す
    fn insert(&self, task_id: &str) -> bool {
        let mut reported = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if reported.iter().any(|known| known == task_id) {
            return false;
        }
        if reported.len() == REPORTED_CAPACITY {
            reported.pop_front();
        }
        reported.push_back(task_id.to_owned());
        true
    }
}

impl<S, Args, IdType> Service<Task<Args, PgContext, IdType>> for JobFailureReport<S>
where
    S: Service<Task<Args, PgContext, IdType>>,
    S::Future: Send + 'static,
    S::Response: 'static,
    S::Error: 'static,
    IdType: std::fmt::Display,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, task: Task<Args, PgContext, IdType>) -> Self::Future {
        let queue = self.queue;
        let reported = self.reported.clone();
        let task_id = task.parts.task_id.as_ref().map(ToString::to_string);
        let attempt = task.parts.attempt.clone();
        let max_attempts = task.parts.ctx.max_attempts();
        let future = self.inner.call(task);
        Box::pin(async move {
            let result = future.await;
            if result.is_err()
                && is_final_attempt(attempt.current(), max_attempts)
                && task_id.as_deref().is_none_or(|id| reported.insert(id))
            {
                report(queue, task_id.as_deref(), attempt.current());
            }
            result
        })
    }
}

/// `apalis_postgres` の ack が Killed にする条件と同じ。これより手前の失敗は Failed になり、
/// リトライを使い切っていても再び取得される。
fn is_final_attempt(attempt: usize, max_attempts: i32) -> bool {
    max_attempts as usize <= attempt
}

fn report(queue: &str, task_id: Option<&str>, attempts: usize) {
    sentry::with_scope(
        |scope| {
            scope.set_tag("job.queue", queue);
            scope.set_extra("task_id", task_id.into());
            scope.set_extra("attempts", attempts.into());
        },
        || {
            sentry::capture_message(
                &format!("apalis job failed after retries: {queue}"),
                sentry::Level::Error,
            )
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_attempt_that_kills_the_job_is_final() {
        assert!(!is_final_attempt(4, 5));
        assert!(is_final_attempt(5, 5));
        assert!(is_final_attempt(9, 5));
    }

    #[test]
    fn reported_tasks_dedupe_and_forget_the_oldest() {
        let reported = ReportedTasks::default();
        assert!(reported.insert("task-0"));
        assert!(!reported.insert("task-0"));

        for i in 1..=REPORTED_CAPACITY {
            assert!(reported.insert(&format!("task-{i}")));
        }
        assert!(
            reported.insert("task-0"),
            "oldest entry should be forgotten"
        );
    }

    #[test]
    fn report_carries_only_the_queue_and_identifiers() {
        let events = sentry::test::with_captured_events(|| report("queue", Some("task-1"), 9));

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.level, sentry::Level::Error);
        assert_eq!(
            event.message.as_deref(),
            Some("apalis job failed after retries: queue")
        );
        assert_eq!(
            event.tags.get("job.queue").map(String::as_str),
            Some("queue")
        );
        assert_eq!(event.extra["task_id"], "task-1");
        assert_eq!(event.extra["attempts"], 9);
    }
}
