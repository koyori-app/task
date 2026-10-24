//! apalis ワーカーの監督（`job::supervise`）を実 Postgres で確かめる。
//!
//! apalis のタイマーは futures_timer なので `tokio::time::pause` は効かない。
//! keep_alive を 1 秒に縮めた storage で、実時間で待つ。
//! キューはテストごとに作り、他のテストのジョブやワーカーの行と混ざらないようにする。

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use apalis::prelude::{
    BoxDynError, Data, TaskSink, WorkerBuilder, WorkerContext, WorkerError, WorkerStateError,
};
use apalis_postgres::{CompactType, Config, JsonCodec, PgNotify, PostgresStorage};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use tokio::sync::{Notify, watch};
use tokio::time::timeout;
use uuid::Uuid;

use job::supervise::{Generation, run_generation, supervise, worker_id};

use crate::common::TestApp;

type ProbeStorage = PostgresStorage<u32, CompactType, JsonCodec<CompactType>, PgNotify>;

const KEEP_ALIVE: Duration = Duration::from_secs(1);
/// 自然には回収されない長さ。回収はテストが旧世代の行の `last_seen` を戻して起こす
const REENQUEUE_AFTER: Duration = Duration::from_secs(60 * 60);
const DEADLINE: Duration = Duration::from_secs(10);

fn new_queue(prefix: &str) -> (String, String) {
    let queue = format!("{prefix}_{}", Uuid::new_v4().simple());
    let logical = format!("{queue}-worker");
    (queue, logical)
}

fn probe_storage(app: &TestApp, queue: &str) -> ProbeStorage {
    let config = Config::new(queue)
        .set_keep_alive(KEEP_ALIVE)
        .set_reenqueue_orphaned_after(REENQUEUE_AFTER);
    PostgresStorage::<u32>::new_with_notify(&app.state.pg_pool, &config)
}

#[derive(Default)]
struct Gate {
    reached: Notify,
    release: Notify,
}

/// ハンドラーに着いたことを知らせ、解放されるまで止まる
async fn blocking_probe(_n: u32, gate: Data<Arc<Gate>>) -> Result<(), BoxDynError> {
    gate.reached.notify_one();
    gate.release.notified().await;
    Ok(())
}

async fn probe(_n: u32) -> Result<(), BoxDynError> {
    Ok(())
}

fn blocking_generation(
    storage: &ProbeStorage,
    gate: &Arc<Gate>,
    mut ctx: WorkerContext,
) -> Generation {
    let worker = WorkerBuilder::new(ctx.name())
        .backend(storage.clone())
        .data(gate.clone())
        .build(blocking_probe);
    Box::pin(async move { worker.run_with_ctx(&mut ctx).await })
}

fn probe_generation(storage: &ProbeStorage, mut ctx: WorkerContext) -> Generation {
    let worker = WorkerBuilder::new(ctx.name())
        .backend(storage.clone())
        .build(probe);
    Box::pin(async move { worker.run_with_ctx(&mut ctx).await })
}

/// キューに 1 件だけ積んだジョブの (status, lock_by)
async fn job_row(db: &DatabaseConnection, queue: &str) -> (String, Option<String>) {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT status, lock_by FROM apalis.jobs WHERE job_type = $1",
            [queue.into()],
        ))
        .await
        .expect("select job")
        .expect("job row");
    (
        row.try_get("", "status").expect("status"),
        row.try_get("", "lock_by").expect("lock_by"),
    )
}

async fn worker_ids(db: &DatabaseConnection, queue: &str) -> Vec<String> {
    db.query_all_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT id FROM apalis.workers WHERE worker_type = $1 ORDER BY id",
        [queue.into()],
    ))
    .await
    .expect("select workers")
    .iter()
    .map(|r| r.try_get("", "id").expect("id"))
    .collect()
}

async fn wait_until<F, Fut>(what: &str, mut cond: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let waited = timeout(DEADLINE, async {
        while !cond().await {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "timed out waiting until {what}");
}

/// 死んだ世代が掴んでいたジョブは、別 ID の次世代の heartbeat 側の孤児回収で Pending に戻り、
/// 次世代が実行し直す。同じ ID で復帰すると `last_seen` が更新され続けて回収されない。
#[tokio::test]
async fn orphaned_job_of_dead_generation_is_requeued_and_rerun_by_next_generation() {
    let app = TestApp::new().await;
    let db = &app.state.db;
    let (queue, logical) = new_queue("supervise_orphan");
    let mut storage = probe_storage(&app, &queue);
    storage.push(1).await.expect("push job");
    let gate = Arc::new(Gate::default());
    let (tx, rx) = watch::channel(false);

    let id1 = worker_id(&logical, 1);
    let ctx1 = WorkerContext::new::<()>(&id1);
    let gen1 = tokio::spawn(run_generation(
        ctx1.clone(),
        blocking_generation(&storage, &gate, ctx1),
        rx.clone(),
    ));
    timeout(DEADLINE, gate.reached.notified())
        .await
        .expect("generation 1 picks up the job");
    // ジョブを掴んだまま世代 1 のタスクが消える
    gen1.abort();
    let _ = gen1.await;
    assert_eq!(
        job_row(db, &queue).await,
        ("Running".to_string(), Some(id1.clone()))
    );

    let id2 = worker_id(&logical, 2);
    let ctx2 = WorkerContext::new::<()>(&id2);
    let gen2 = tokio::spawn(run_generation(
        ctx2.clone(),
        blocking_generation(&storage, &gate, ctx2.clone()),
        rx,
    ));
    // 登録前に止めると keep_alive が WORKER_DOES_NOT_EXIST で落ちるので、登録を待ってから取り込みを止める
    wait_until("generation 2 registers", || async {
        worker_ids(db, &queue).await.contains(&id2)
    })
    .await;
    ctx2.pause().expect("pause generation 2");
    // 取り込みを止めてから旧世代の行を回収対象にする。回収は heartbeat 側（KEEP_ALIVE ごと）が行う
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE apalis.workers SET last_seen = now() - interval '2 hours' WHERE id = $1",
        [id1.into()],
    ))
    .await
    .expect("age generation 1 row");
    wait_until("the orphaned job returns to Pending", || async {
        job_row(db, &queue).await == ("Pending".to_string(), None)
    })
    .await;

    ctx2.resume().expect("resume generation 2");
    timeout(DEADLINE, gate.reached.notified())
        .await
        .expect("generation 2 re-runs the job");
    gate.release.notify_one();
    wait_until("the job is Done", || async {
        job_row(db, &queue).await.0 == "Done"
    })
    .await;
    assert_eq!(job_row(db, &queue).await.1, Some(id2));

    tx.send(true).unwrap();
    timeout(DEADLINE, gen2)
        .await
        .expect("generation 2 stops after shutdown")
        .unwrap()
        .expect("graceful exit");
}

/// heartbeat が失敗した世代（ワーカーの行が消えて WORKER_DOES_NOT_EXIST）を、
/// 監督が別 ID の世代で作り直し、新しいジョブを処理できる
#[tokio::test]
async fn supervisor_restarts_generation_after_heartbeat_failure() {
    let app = TestApp::new().await;
    let db = &app.state.db;
    let (queue, logical) = new_queue("supervise_restart");
    let mut storage = probe_storage(&app, &queue);
    let (tx, rx) = watch::channel(false);
    let build_storage = storage.clone();
    let supervisor = tokio::spawn(supervise(
        logical.clone(),
        move |ctx| probe_generation(&build_storage, ctx),
        rx,
    ));

    let id1 = worker_id(&logical, 1);
    wait_until("generation 1 registers", || async {
        worker_ids(db, &queue).await == [id1.clone()]
    })
    .await;
    // ジョブを積んでいないキューなので jobs.lock_by から参照されておらず消せる
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "DELETE FROM apalis.workers WHERE id = $1",
        [id1.into()],
    ))
    .await
    .expect("delete worker row");

    let id2 = worker_id(&logical, 2);
    wait_until("generation 2 registers under a new id", || async {
        worker_ids(db, &queue).await == [id2.clone()]
    })
    .await;

    storage.push(1).await.expect("push job");
    wait_until("the job is Done", || async {
        job_row(db, &queue).await.0 == "Done"
    })
    .await;
    assert_eq!(job_row(db, &queue).await.1, Some(id2));

    tx.send(true).unwrap();
    timeout(DEADLINE, supervisor)
        .await
        .expect("supervisor returns after shutdown")
        .unwrap();
}

/// 世代を作った直後（初回 poll 前）に shutdown が来ても世代は終わり、監督は作り直さない
#[tokio::test]
async fn shutdown_before_first_poll_ends_generation_without_restart() {
    let app = TestApp::new().await;
    let db = &app.state.db;
    let (queue, logical) = new_queue("supervise_early_stop");
    let storage = probe_storage(&app, &queue);

    let (_tx, stopped) = watch::channel(true);
    let ctx = WorkerContext::new::<()>(&worker_id(&logical, 1));
    let res = timeout(
        DEADLINE,
        run_generation(ctx.clone(), probe_generation(&storage, ctx), stopped),
    )
    .await
    .expect("generation ends");
    assert!(
        matches!(
            res,
            Ok(()) | Err(WorkerError::StateError(WorkerStateError::AlreadyStarted))
        ),
        "unexpected result: {res:?}"
    );

    let (tx, rx) = watch::channel(false);
    let builds = Arc::new(AtomicUsize::new(0));
    let counted = builds.clone();
    let supervisor = tokio::spawn(supervise(
        logical,
        move |ctx| {
            counted.fetch_add(1, Ordering::SeqCst);
            tx.send(true).unwrap();
            probe_generation(&storage, ctx)
        },
        rx,
    ));
    timeout(DEADLINE, supervisor)
        .await
        .expect("supervisor returns")
        .unwrap();
    assert_eq!(builds.load(Ordering::SeqCst), 1);
    // 初回 poll 前に止まったので、どの世代も登録していない
    assert!(worker_ids(db, &queue).await.is_empty());
}

/// 動き出した世代も shutdown で終わり、監督が抜ける
#[tokio::test]
async fn shutdown_after_first_poll_ends_generation_and_supervisor() {
    let app = TestApp::new().await;
    let db = &app.state.db;
    let (queue, logical) = new_queue("supervise_stop");
    let storage = probe_storage(&app, &queue);
    let (tx, rx) = watch::channel(false);
    let builds = Arc::new(AtomicUsize::new(0));
    let counted = builds.clone();
    let supervisor = tokio::spawn(supervise(
        logical.clone(),
        move |ctx| {
            counted.fetch_add(1, Ordering::SeqCst);
            probe_generation(&storage, ctx)
        },
        rx,
    ));

    let id1 = worker_id(&logical, 1);
    wait_until("generation 1 registers", || async {
        worker_ids(db, &queue).await == [id1.clone()]
    })
    .await;
    tx.send(true).unwrap();
    timeout(DEADLINE, supervisor)
        .await
        .expect("supervisor returns after shutdown")
        .unwrap();
    assert_eq!(builds.load(Ordering::SeqCst), 1);
}
