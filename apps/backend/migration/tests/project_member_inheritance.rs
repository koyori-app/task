//! m20261111000000_project_member_inheritance の移し（TASK-501）。
//!
//! `TEST_DATABASE_URL` の server に使い捨ての DB を作り、一つ前の migration まで当てた所へ
//! メンバーを載せたプロジェクトを置いてから、移しを当てる。
use migration::Migrator;
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

const USER: &str = "11111111-1111-4111-8111-111111111111";
const TENANT: &str = "22222222-2222-4222-8222-222222222222";
const LISTED: &str = "33333333-3333-4333-8333-333333333333";
const OPEN: &str = "44444444-4444-4444-8444-444444444444";

async fn scalar<C: ConnectionTrait>(db: &C, sql: &str) -> Option<String> {
    db.query_one_raw(Statement::from_string(DbBackend::Postgres, sql.to_owned()))
        .await
        .expect("query")
        .map(|row| row.try_get_by_index::<String>(0).expect("text column"))
}

#[tokio::test]
async fn existing_projects_inherit_and_listed_roles_remain() {
    let base = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL を置いて撃つ（使い捨ての DB をその server に作る）");
    let admin = Database::connect(&base).await.expect("connect admin");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("mig_inherit_{nanos}");
    admin
        .execute_unprepared(&format!("CREATE DATABASE {name}"))
        .await
        .expect("create db");
    let server = base.split('?').next().unwrap();
    let url = format!("{}/{name}", server.rsplit_once('/').unwrap().0);
    let db = Database::connect(&url).await.expect("connect");

    let total = Migrator::migrations().len() as u32;
    Migrator::up(&db, Some(total - 1))
        .await
        .expect("up to previous");
    db.execute_unprepared(&format!(
        r#"
        INSERT INTO users (id, username, email) VALUES ('{USER}', 'aki', 'aki@example.com');
        INSERT INTO tenants (id, display_id, name, owner_id) VALUES ('{TENANT}', 'koyori', 'koyori', '{USER}');
        INSERT INTO projects (id, name, tenant_id, key) VALUES ('{LISTED}', 'task', '{TENANT}', 'TASK');
        INSERT INTO projects (id, name, tenant_id, key) VALUES ('{OPEN}', 'ui', '{TENANT}', 'UI');
        INSERT INTO project_members (id, project_id, user_id, role)
            VALUES (gen_random_uuid(), '{LISTED}', '{USER}', 'Admin');
        "#
    ))
    .await
    .expect("seed previous schema");

    Migrator::up(&db, None).await.expect("up");
    for project in [LISTED, OPEN] {
        assert_eq!(
            scalar(
                &db,
                &format!(
                    "SELECT inherit_tenant_members::text FROM projects WHERE id = '{project}'"
                )
            )
            .await
            .as_deref(),
            Some("true"),
            "既存のプロジェクトはメンバーの有無にかかわらず継承する ({project})"
        );
    }
    let role_sql = format!("SELECT role FROM project_members WHERE project_id = '{LISTED}'");
    assert_eq!(scalar(&db, &role_sql).await.as_deref(), Some("Admin"));

    Migrator::down(&db, Some(1)).await.expect("down");
    assert_eq!(
        scalar(
            &db,
            "SELECT column_name::text FROM information_schema.columns \
             WHERE table_name = 'projects' AND column_name = 'inherit_tenant_members'"
        )
        .await,
        None,
        "down で列が消える"
    );
    assert_eq!(scalar(&db, &role_sql).await.as_deref(), Some("Admin"));

    db.close().await.expect("close");
    admin
        .execute_unprepared(&format!("DROP DATABASE {name}"))
        .await
        .expect("drop db");
}
