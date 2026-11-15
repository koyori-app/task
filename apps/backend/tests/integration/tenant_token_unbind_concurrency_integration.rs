//! テナントの削除と BOT の一括失効が同時に走ったときのロック順（TASK-529）。
//!
//! どちらも `service::auth::unbind_tenant_from_tokens` で束縛を外す。待ち合わせは
//! `pg_blocking_pids` で行い、sleep で時間を当てない。
use crate::common::{TestApp, TestUser, insert_tenant, json_body};
use axum::http::StatusCode;
use entity::scopes::Scope;
use entity::{personal_token_tenants, personal_tokens, tenant_members, tenants};
use reqwest::Client;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend,
    DatabaseTransaction, EntityTrait, PaginatorTrait, QueryFilter, Statement, TransactionTrait,
};
use serde_json::json;
use std::time::Duration;
use tokio::task::JoinHandle;
use uuid::Uuid;

const WAIT: Duration = Duration::from_secs(10);

async fn blocker_pid(txn: &DatabaseTransaction) -> i32 {
    txn.query_one_raw(Statement::from_string(
        DatabaseBackend::Postgres,
        "SELECT pg_backend_pid() AS pid",
    ))
    .await
    .expect("query blocker backend pid")
    .expect("blocker backend pid row")
    .try_get("", "pid")
    .expect("blocker backend pid")
}

/// `pid` の接続の後ろで待っている接続の数（待ちの連鎖をたどる）。
async fn blocked_behind(app: &TestApp, pid: i32) -> i64 {
    app.state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "WITH RECURSIVE blocked(pid) AS ( \
                 SELECT pid FROM pg_stat_activity \
                 WHERE $1 = ANY(pg_blocking_pids(pid)) \
                 UNION \
                 SELECT activity.pid FROM pg_stat_activity AS activity \
                 JOIN blocked ON blocked.pid = ANY(pg_blocking_pids(activity.pid)) \
             ) SELECT COUNT(*)::bigint AS count FROM blocked",
            [pid.into()],
        ))
        .await
        .expect("query blocked connections")
        .expect("blocked connections count")
        .try_get("", "count")
        .expect("blocked connections count column")
}

async fn wait_until_blocked(app: &TestApp, pid: i32, at_least: i64) {
    tokio::time::timeout(WAIT, async {
        while blocked_behind(app, pid).await < at_least {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{at_least} request(s) should wait behind the held lock"));
}

async fn hold(app: &TestApp, sql: &str, id: Uuid) -> (DatabaseTransaction, i32) {
    let txn = app.state.db.begin().await.expect("begin holder txn");
    let pid = blocker_pid(&txn).await;
    txn.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [id.into()],
    ))
    .await
    .expect("take the holding lock");
    (txn, pid)
}

fn spawn_delete(client: Client, url: String) -> JoinHandle<StatusCode> {
    tokio::spawn(async move { client.delete(url).send().await.expect("delete").status() })
}

fn spawn_revoke_all(client: Client, app: &TestApp, tenant_id: Uuid) -> JoinHandle<StatusCode> {
    let url = format!("{}/v1/personal_tokens/revoke-all", app.base_url());
    tokio::spawn(async move {
        client
            .delete(url)
            .json(&json!({ "confirm_tenant_id": tenant_id }))
            .send()
            .await
            .expect("revoke-all")
            .status()
    })
}

async fn finish(handle: JoinHandle<StatusCode>) -> StatusCode {
    tokio::time::timeout(WAIT, handle)
        .await
        .expect("request should finish")
        .expect("join request")
}

async fn create_bot(app: &TestApp, tenant_id: Uuid) -> Uuid {
    let res = app
        .post_json_with_session(
            &format!("/v1/tenants/{tenant_id}/bots"),
            json!({"username": "lock-order-bot", "role": "Member"}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    Uuid::parse_str(json_body(res).await["id"].as_str().unwrap()).unwrap()
}

#[derive(Clone, Copy)]
enum Entry {
    Owner,
    Admin,
}

/// 同じテナントの削除（`entry`）と一括失効が、BOT の行を握られた状態で重なる。
async fn deleting_a_tenant_during_bulk_revocation(entry: Entry) {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let tenant_id = app.insert_tenant_project(owner.id).await.tenant_id;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let bot_id = create_bot(&app, tenant_id).await;
    app.insert_pat(owner.id, tenant_id, vec![Scope::Api], None)
        .await;
    let owner_client = app.session_client();

    let (delete_client, delete_url) = match entry {
        Entry::Owner => (
            owner_client.clone(),
            format!("{}/v1/tenants/{tenant_id}", app.base_url()),
        ),
        Entry::Admin => {
            let admin = app.insert_user(true, false).await;
            app.reset_session_client();
            app.login_session_no_content(&admin.email, &admin.password)
                .await;
            (
                app.session_client(),
                format!("{}/v1/admin/tenants/{tenant_id}", app.base_url()),
            )
        }
    };

    // BOT の users 行を握る。一括失効はテナント行を取ったあと、ここで待つ。
    let (holder, pid) = hold(
        &app,
        "SELECT id FROM users WHERE id = $1 FOR UPDATE",
        bot_id,
    )
    .await;
    let revoke = spawn_revoke_all(owner_client, &app, tenant_id);
    wait_until_blocked(&app, pid, 1).await;
    // 削除は一括失効の後ろに並ぶ。
    let delete = spawn_delete(delete_client, delete_url);
    wait_until_blocked(&app, pid, 2).await;

    holder.rollback().await.expect("release the holder");
    assert_eq!(finish(revoke).await, StatusCode::NO_CONTENT);
    assert_eq!(finish(delete).await, StatusCode::NO_CONTENT);

    assert!(
        tenants::Entity::find_by_id(tenant_id)
            .one(&app.state.db)
            .await
            .expect("find tenant")
            .is_none(),
        "the tenant should be deleted"
    );
    let tokens = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(owner.id))
        .all(&app.state.db)
        .await
        .expect("list tokens");
    assert_eq!(tokens.len(), 1);
    assert!(tokens.iter().all(|token| token.revoked));
}

#[tokio::test]
async fn owner_deleting_a_tenant_during_bulk_revocation_does_not_deadlock() {
    deleting_a_tenant_during_bulk_revocation(Entry::Owner).await;
}

#[tokio::test]
async fn admin_deleting_a_tenant_during_bulk_revocation_does_not_deadlock() {
    deleting_a_tenant_during_bulk_revocation(Entry::Admin).await;
}

async fn add_member(app: &TestApp, tenant_id: Uuid, user: &TestUser) {
    tenant_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        tenant_id: Set(tenant_id),
        user_id: Set(user.id),
        role: Set(tenant_members::TenantRole::Member),
        department: Set(None),
        position: Set(None),
        team: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert tenant member");
}

/// 別々のテナントの削除と一括失効が、共有トークンの束縛を同時に外す。
#[tokio::test]
async fn concurrent_unbinds_of_two_tenants_revoke_the_shared_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let t1 = insert_tenant(&app.state.db, owner.id).await;
    let t2 = app.insert_tenant_project(owner.id).await.tenant_id;
    let member = app.insert_user_default().await;
    add_member(&app, t1, &member).await;
    app.insert_pat_in_tenants(owner.id, &[t1, t2], vec![Scope::Api], None)
        .await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    // T1 のメンバー行を握る。T1 の削除は束縛を外したあと、Cascade でここに止まる。
    let (holder, pid) = hold(
        &app,
        "SELECT id FROM tenant_members WHERE tenant_id = $1 FOR UPDATE",
        t1,
    )
    .await;
    let delete = spawn_delete(
        app.session_client(),
        format!("{}/v1/tenants/{t1}", app.base_url()),
    );
    wait_until_blocked(&app, pid, 1).await;
    let revoke = spawn_revoke_all(app.session_client(), &app, t2);
    // 修正後は共有トークンの行ロックで削除の後ろに並ぶ。修正前は待たずに最後まで走る。
    tokio::time::timeout(WAIT, async {
        while !revoke.is_finished() && blocked_behind(&app, pid).await < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("revoke-all should finish or wait behind the deletion");

    holder.rollback().await.expect("release the holder");
    assert_eq!(finish(delete).await, StatusCode::NO_CONTENT);
    assert_eq!(finish(revoke).await, StatusCode::NO_CONTENT);

    let tokens = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(owner.id))
        .all(&app.state.db)
        .await
        .expect("list tokens");
    assert_eq!(tokens.len(), 1);
    assert!(
        tokens[0].revoked,
        "a token left with no tenant bindings must be revoked"
    );
    let bindings = personal_token_tenants::Entity::find()
        .filter(personal_token_tenants::Column::TokenId.eq(tokens[0].id))
        .count(&app.state.db)
        .await
        .expect("count bindings");
    assert_eq!(bindings, 0);
}
