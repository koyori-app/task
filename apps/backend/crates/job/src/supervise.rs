//! apalis ワーカーの監督ループ。
//!
//! ワーカーは heartbeat の失敗（DB 停滞でプールの取得が時間切れになるなど）で終了し、
//! そのままではジョブが Pending のまま滞留する。[`supervise`] は終了した世代を
//! 待ち時間を伸ばしながら作り直す。
//!
//! 世代ごとにワーカー ID を変える（[`worker_id`]）。同じ ID で登録し直すと、
//! `apalis.workers` の更新がセッション単位の advisory lock で弾かれて即死し、
//! 死んだ世代が掴んでいたジョブも `last_seen` が更新され続けて孤児回収の対象にならない。

use std::future::Future;
use std::pin::Pin;
use std::sync::LazyLock;
use std::time::Duration;

use apalis::prelude::{WorkerContext, WorkerError};
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::{error, info, warn};
use uuid::Uuid;

/// 1 世代分のワーカーの実行。`run_with_ctx` に渡された ctx で動かす future
pub type Generation = Pin<Box<dyn Future<Output = Result<(), WorkerError>> + Send>>;

const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// これだけ動いた世代の終了は連続した失敗と見なさず、待ち時間を初期値に戻す
const HEALTHY_RUN: Duration = Duration::from_secs(5 * 60);

/// プロセス起動時に 1 回だけ作る。再起動前のプロセスの行とも ID が重ならない
static PROCESS_ID: LazyLock<Uuid> = LazyLock::new(Uuid::new_v4);

/// `apalis.workers.id` に登録する ID。`logical_name` はログ用の名前（例 `{queue}-worker`）
pub fn worker_id(logical_name: &str, generation: u64) -> String {
    format!("{logical_name}-{}-{generation}", *PROCESS_ID)
}

/// shutdown まで世代を作り直し続ける。`build` は ctx を受け取り、その ctx で
/// ワーカーを動かす future を返す（DB には触らない同期の組み立てだけを行う）。
pub async fn supervise<F>(logical_name: String, build: F, shutdown: watch::Receiver<bool>)
where
    F: Fn(WorkerContext) -> Generation,
{
    let mut backoff = INITIAL_BACKOFF;
    let mut generation = 0;
    loop {
        if *shutdown.borrow() {
            return;
        }
        generation += 1;
        let id = worker_id(&logical_name, generation);
        let ctx = WorkerContext::new::<()>(&id);
        let started = Instant::now();
        let handle = tokio::spawn(run_generation(ctx.clone(), build(ctx), shutdown.clone()));
        info!(worker = %logical_name, generation, id = %id, "apalis worker generation started");
        let outcome = handle.await;
        // shutdown 後は Ok でも Err でも作り直さない（停止の順序次第で Err が返る）
        if *shutdown.borrow() {
            return;
        }

        if started.elapsed() >= HEALTHY_RUN {
            backoff = INITIAL_BACKOFF;
        }
        let cause = match outcome {
            Ok(Ok(())) => "exited without error".to_string(),
            Ok(Err(e)) => e.to_string(),
            Err(e) => e.to_string(),
        };
        if backoff == INITIAL_BACKOFF {
            error!(worker = %logical_name, generation, cause = %cause, retry_in = ?backoff, "apalis worker stopped; restarting");
        } else {
            warn!(worker = %logical_name, generation, cause = %cause, retry_in = ?backoff, "apalis worker stopped again; restarting");
        }
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = wait_for_shutdown(shutdown.clone()) => return,
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

/// 1 世代を動かし、shutdown が来たら ctx を止めて処理中のジョブの完了を待つ。
///
/// `run_until` は使わない。signal が先に完了すると未開始の ctx へ `stop()` して
/// `NotStarted` が捨てられ、その後に起動したワーカーが止まらなくなる。
pub async fn run_generation(
    mut ctx: WorkerContext,
    mut run: Generation,
    shutdown: watch::Receiver<bool>,
) -> Result<(), WorkerError> {
    ctx.start()?;
    tokio::select! {
        // 既に shutdown 済みなら初回 poll 前に止める（starter の start() が AlreadyStarted を返す）
        biased;
        _ = wait_for_shutdown(shutdown) => {
            let _ = ctx.stop();
            run.await
        }
        res = &mut run => res,
    }
}

async fn wait_for_shutdown(mut shutdown: watch::Receiver<bool>) {
    while !*shutdown.borrow() {
        if shutdown.changed().await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Copy)]
    enum Fake {
        Err,
        Ok,
        Panic,
        /// この時間動いてから Err で終わる
        ErrAfter(Duration),
        /// stop されるまで動き、stop 後に Err で終わる（処理中に heartbeat が落ちた場合）
        ErrAfterStop,
    }

    /// 台本どおりに終わる偽の世代を作る `build` と、各世代を作った時刻。
    /// 台本を使い切った後の世代は stop されるまで動いて Ok で終わる
    fn script(
        gens: Vec<Fake>,
    ) -> (
        impl Fn(WorkerContext) -> Generation,
        Arc<Mutex<Vec<Instant>>>,
    ) {
        let starts = Arc::new(Mutex::new(Vec::new()));
        let recorded = starts.clone();
        let build = move |ctx: WorkerContext| -> Generation {
            let mut starts = recorded.lock().unwrap();
            let fake = gens.get(starts.len()).copied();
            starts.push(Instant::now());
            Box::pin(async move {
                match fake {
                    Some(Fake::Err) => Err(WorkerError::HeartbeatError("boom".into())),
                    Some(Fake::Ok) => Ok(()),
                    Some(Fake::Panic) => panic!("listener creation failed"),
                    Some(Fake::ErrAfter(d)) => {
                        tokio::time::sleep(d).await;
                        Err(WorkerError::HeartbeatError("boom".into()))
                    }
                    Some(Fake::ErrAfterStop) => {
                        ctx.await?;
                        Err(WorkerError::HeartbeatError("boom".into()))
                    }
                    // WorkerContext は stop 後（処理中のジョブが無ければ）に Ok で完了する
                    None => ctx.await,
                }
            })
        };
        (build, starts)
    }

    fn gaps(starts: &Mutex<Vec<Instant>>) -> Vec<Duration> {
        starts
            .lock()
            .unwrap()
            .windows(2)
            .map(|w| w[1] - w[0])
            .collect()
    }

    /// 台本を流し、`run_for` 経ってから shutdown して監督の終了を待つ
    async fn run_script(gens: Vec<Fake>, run_for: Duration) -> Vec<Duration> {
        let (tx, rx) = watch::channel(false);
        let (build, starts) = script(gens);
        let handle = tokio::spawn(supervise("test-worker".into(), build, rx));
        tokio::time::sleep(run_for).await;
        tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .expect("supervisor must return after shutdown")
            .unwrap();
        gaps(&starts)
    }

    fn secs(s: &[u64]) -> Vec<Duration> {
        s.iter().map(|&s| Duration::from_secs(s)).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn restarts_failed_generations_with_doubling_backoff() {
        let gaps = run_script(vec![Fake::Err; 3], Duration::from_secs(60)).await;
        // 3 回作り直し、4 世代目が動き続ける
        assert_eq!(gaps, secs(&[1, 2, 4]));
    }

    #[tokio::test(start_paused = true)]
    async fn backoff_is_capped() {
        let gaps = run_script(vec![Fake::Err; 8], Duration::from_secs(600)).await;
        assert_eq!(gaps, secs(&[1, 2, 4, 8, 16, 32, 60, 60]));
    }

    #[tokio::test(start_paused = true)]
    async fn backoff_resets_after_a_healthy_run() {
        let gens = vec![Fake::Err, Fake::Err, Fake::Err, Fake::ErrAfter(HEALTHY_RUN)];
        let gaps = run_script(gens, Duration::from_secs(600)).await;
        // 5 分動いた世代の後は 8 秒ではなく 1 秒で作り直す
        assert_eq!(gaps, secs(&[1, 2, 4, 5 * 60 + 1]));
    }

    #[tokio::test(start_paused = true)]
    async fn short_run_just_under_threshold_keeps_growing() {
        let gens = vec![
            Fake::Err,
            Fake::ErrAfter(HEALTHY_RUN - Duration::from_secs(1)),
        ];
        let gaps = run_script(gens, Duration::from_secs(600)).await;
        assert_eq!(gaps, secs(&[1, 5 * 60 - 1 + 2]));
    }

    #[tokio::test(start_paused = true)]
    async fn restarts_panicked_generation() {
        let gaps = run_script(vec![Fake::Panic], Duration::from_secs(10)).await;
        assert_eq!(gaps, secs(&[1]));
    }

    #[tokio::test(start_paused = true)]
    async fn restarts_generation_that_exits_ok_before_shutdown() {
        let gaps = run_script(vec![Fake::Ok], Duration::from_secs(10)).await;
        assert_eq!(gaps, secs(&[1]));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_during_backoff_does_not_restart() {
        // 世代 1 が落ち、1 秒の待ちの途中で shutdown
        let gaps = run_script(vec![Fake::Err], Duration::from_millis(500)).await;
        assert!(gaps.is_empty(), "rebuilt during backoff: {gaps:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn generation_failing_after_shutdown_is_not_restarted() {
        let gaps = run_script(vec![Fake::ErrAfterStop], Duration::from_secs(10)).await;
        assert!(gaps.is_empty(), "rebuilt after shutdown: {gaps:?}");
    }

    #[test]
    fn worker_id_changes_per_generation_and_keeps_full_process_id() {
        let first = worker_id("q-worker", 1);
        assert_eq!(first, format!("q-worker-{}-1", *PROCESS_ID));
        assert_ne!(first, worker_id("q-worker", 2));
        assert_eq!(PROCESS_ID.get_version_num(), 4);
    }
}
