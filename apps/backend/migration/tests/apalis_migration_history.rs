//! m20261117000000_apalis_migration_history_schema の移し。
//!
//! `TEST_DATABASE_URL` の server に使い捨ての DB を作り、この移しの一つ前まで当てた所へ、
//! apalis-postgres 1.0.0-rc.8 以前の DB の状態（`public._sqlx_migrations` に最初の移行の記録）を置いてから、
//! 移しを当てる。
use migration::Migrator;
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};

const NAME: &str = "m20261117000000_apalis_migration_history_schema";
const FIRST: i64 = 20220530084123;
/// apalis-postgres 1.0.0-rc.8 の最初の移行のチェックサム（SHA-384）
const OLD_CHECKSUM: &str = "0e2ae5fd117c32925d7ff4ba4f26368269fba7706bf1c3e9578a763a3e2acc5fd44c8edf2aa25e7c0ac1c6ebc8e95a9a";
/// apalis-postgres 1.1.0 の最初の移行のチェックサム
const NEW_CHECKSUM: &str = "d0839c6f57a379769dc27ccd581feb3d2709239c8f138e05271c9e3c760c4517a78a4d8912ab3d63b074b28d15ec74e9";

const TABLE: &str = "(
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
    success BOOLEAN NOT NULL,
    checksum BYTEA NOT NULL,
    execution_time BIGINT NOT NULL
)";

async fn scratch_db(label: &str) -> DatabaseConnection {
    let base = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL を置いて撃つ（使い捨ての DB をその server に作る）");
    let admin = Database::connect(&base).await.expect("connect admin");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("mig_apalis_{label}_{nanos}");
    admin
        .execute_unprepared(&format!("CREATE DATABASE {name}"))
        .await
        .expect("create db");
    let server = base.split('?').next().unwrap();
    let url = format!("{}/{name}", server.rsplit_once('/').unwrap().0);
    Database::connect(&url).await.expect("connect")
}

async fn up_to_previous(db: &DatabaseConnection) {
    let position = Migrator::migrations()
        .iter()
        .position(|m| m.name() == NAME)
        .expect("migration is registered");
    Migrator::up(db, Some(position as u32))
        .await
        .expect("up to previous");
}

async fn text(db: &DatabaseConnection, sql: &str) -> Option<String> {
    db.query_one_raw(Statement::from_string(DbBackend::Postgres, sql.to_owned()))
        .await
        .expect("query")
        .and_then(|row| {
            row.try_get_by_index::<Option<String>>(0)
                .expect("text column")
        })
}

/// 1.0.0-rc.8 以前の DB: apalis のスキーマと、`public._sqlx_migrations` の最初の移行の記録
async fn seed_old_apalis(db: &DatabaseConnection) {
    db.execute_unprepared(&format!(
        "CREATE SCHEMA apalis;
         CREATE TABLE public._sqlx_migrations {TABLE};
         INSERT INTO public._sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES ({FIRST}, 'jobs workers', true, decode('{OLD_CHECKSUM}', 'hex'), 1);"
    ))
    .await
    .expect("seed old apalis");
}

async fn assert_moved(db: &DatabaseConnection) {
    assert_eq!(
        text(db, "SELECT to_regclass('public._sqlx_migrations')::text").await,
        None,
        "public._sqlx_migrations is moved away"
    );
    assert_eq!(
        text(
            db,
            &format!("SELECT encode(checksum, 'hex') FROM apalis._sqlx_migrations WHERE version = {FIRST}")
        )
        .await
        .as_deref(),
        Some(NEW_CHECKSUM),
        "the first migration's checksum is re-stamped"
    );
    assert_eq!(
        text(db, "SELECT count(*)::text FROM apalis._sqlx_migrations")
            .await
            .as_deref(),
        Some("1"),
        "history rows are kept"
    );
}

#[tokio::test]
async fn old_history_moves_into_the_apalis_schema_and_the_checksum_is_restamped() {
    let db = scratch_db("old").await;
    up_to_previous(&db).await;
    seed_old_apalis(&db).await;

    Migrator::up(&db, None).await.expect("up");
    assert_moved(&db).await;
}

/// 移す前に新しい版のバックエンドが起動して、空の `apalis._sqlx_migrations` を作ってしまった DB
#[tokio::test]
async fn an_empty_history_table_left_by_a_failed_start_is_replaced() {
    let db = scratch_db("empty").await;
    up_to_previous(&db).await;
    seed_old_apalis(&db).await;
    db.execute_unprepared(&format!("CREATE TABLE apalis._sqlx_migrations {TABLE}"))
        .await
        .expect("empty history table");

    Migrator::up(&db, None).await.expect("up");
    assert_moved(&db).await;
}

/// apalis を一度も使っていない DB と、既に移し終えた DB では、何もしない
#[tokio::test]
async fn databases_without_old_history_are_left_alone() {
    let db = scratch_db("fresh").await;
    Migrator::up(&db, None)
        .await
        .expect("up on a fresh database");
    assert_eq!(
        text(&db, "SELECT to_regclass('public._sqlx_migrations')::text").await,
        None
    );
    assert_eq!(
        text(&db, "SELECT to_regclass('apalis._sqlx_migrations')::text").await,
        None
    );

    // 移し終えた DB（`apalis._sqlx_migrations` だけが在る）
    let done = scratch_db("done").await;
    up_to_previous(&done).await;
    done.execute_unprepared(&format!(
        "CREATE SCHEMA apalis;
         CREATE TABLE apalis._sqlx_migrations {TABLE};
         INSERT INTO apalis._sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES ({FIRST}, 'jobs workers', true, decode('{NEW_CHECKSUM}', 'hex'), 1);"
    ))
    .await
    .expect("seed moved apalis");
    Migrator::up(&done, None)
        .await
        .expect("up on a moved database");
    assert_moved(&done).await;
}

/// 別の物が `public._sqlx_migrations` を使っていても、apalis の最初の移行が無ければ触らない
#[tokio::test]
async fn a_foreign_sqlx_history_without_the_apalis_version_is_not_touched() {
    let db = scratch_db("foreign").await;
    up_to_previous(&db).await;
    db.execute_unprepared(&format!(
        "CREATE TABLE public._sqlx_migrations {TABLE};
         INSERT INTO public._sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (1, 'foreign', true, decode('00', 'hex'), 1);"
    ))
    .await
    .expect("seed foreign history");

    Migrator::up(&db, None).await.expect("up");
    assert_eq!(
        text(&db, "SELECT count(*)::text FROM public._sqlx_migrations")
            .await
            .as_deref(),
        Some("1")
    );
}

/// 両方に記録が在る時は、どちらも壊さずに失敗する
#[tokio::test]
async fn rows_on_both_sides_fail_without_losing_either() {
    let db = scratch_db("both").await;
    up_to_previous(&db).await;
    seed_old_apalis(&db).await;
    db.execute_unprepared(&format!(
        "CREATE TABLE apalis._sqlx_migrations {TABLE};
         INSERT INTO apalis._sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (99, 'other', true, decode('00', 'hex'), 1);"
    ))
    .await
    .expect("seed both");

    assert!(Migrator::up(&db, None).await.is_err());
    assert_eq!(
        text(&db, "SELECT count(*)::text FROM public._sqlx_migrations")
            .await
            .as_deref(),
        Some("1")
    );
    assert_eq!(
        text(&db, "SELECT count(*)::text FROM apalis._sqlx_migrations")
            .await
            .as_deref(),
        Some("1")
    );
}
