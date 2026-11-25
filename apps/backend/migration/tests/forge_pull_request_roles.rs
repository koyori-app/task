//! m20261124000000_forge_pull_request_roles（TASK-591）。
//!
//! 新しいバックエンドを migration より先に起動すると、起動時の SeaORM `sync()` が entity から
//! 表と列を先に作る。その後に当てる migration が「already exists」で止まらないことを見る。
use migration::Migrator;
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, Database};

/// 試す移し。後に移しが足されても位置が崩れぬよう、名で引く
const TARGET: &str = "m20261124000000_forge_pull_request_roles";

#[tokio::test]
async fn applies_after_sync_created_the_table_and_columns() {
    let base = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL を置いて撃つ（使い捨ての DB をその server に作る）");
    let admin = Database::connect(&base).await.expect("connect admin");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("mig_pr_roles_{nanos}");
    admin
        .execute_unprepared(&format!("CREATE DATABASE {name}"))
        .await
        .expect("create db");
    let server = base.split('?').next().unwrap();
    let url = format!("{}/{name}", server.rsplit_once('/').unwrap().0);
    let db = Database::connect(&url).await.expect("connect");

    let before = Migrator::migrations()
        .iter()
        .position(|migration| migration.name() == TARGET)
        .expect("PR の役割の移しが Migrator に載っている") as u32;
    Migrator::up(&db, Some(before))
        .await
        .expect("up to previous");

    // sync() が先に作った状態
    db.execute_unprepared(
        r#"
        ALTER TABLE forge_pull_requests
            ADD COLUMN head_branch VARCHAR,
            ADD COLUMN head_repo_owner VARCHAR,
            ADD COLUMN head_repo_name VARCHAR;
        CREATE TABLE forge_pull_request_roles (
            pull_request_id UUID NOT NULL REFERENCES forge_pull_requests(id) ON DELETE CASCADE,
            task_id         UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            role            VARCHAR NOT NULL,
            set_by          UUID REFERENCES users(id) ON DELETE SET NULL,
            created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
            PRIMARY KEY (pull_request_id, task_id)
        );
        CREATE INDEX idx_forge_pr_roles_task ON forge_pull_request_roles(task_id);
        "#,
    )
    .await
    .expect("simulate sync");

    Migrator::up(&db, None)
        .await
        .expect("sync が作った表と列があっても当たる");

    db.close().await.expect("close");
    admin
        .execute_unprepared(&format!("DROP DATABASE {name}"))
        .await
        .expect("drop db");
}
