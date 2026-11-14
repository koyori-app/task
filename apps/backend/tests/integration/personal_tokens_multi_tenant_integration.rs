//! PAT を複数のテナントに結び付ける（TASK-491）。
//!
//! 束縛は `personal_token_tenants` が正で、作成時に選んだ全テナントで所属・2FA・Admin を確かめる。
//! 認可は「要求のテナントが束縛に含まれるか」に従来の所属判定を足したもの。
use crate::common::{TestApp, TestUser, insert_extra_project, insert_tenant, json_body};
use axum::http::StatusCode;
use entity::scopes::Scope;
use entity::tenant_members::TenantRole;
use reqwest::Response;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, TransactionTrait,
};
use serde_json::{Value, json};
use uuid::Uuid;

const BACKFILL_SQL: &str = include_str!("../../sql/backfill_personal_token_tenants.sql");

async fn add_member(app: &TestApp, tenant_id: Uuid, user_id: Uuid, role: TenantRole) {
    entity::tenant_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        tenant_id: Set(tenant_id),
        user_id: Set(user_id),
        role: Set(role),
        department: Set(None),
        position: Set(None),
        team: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert tenant member");
}

async fn login(app: &mut TestApp, user: &TestUser) {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
}

async fn create_token(app: &TestApp, tenant_ids: Value, project_ids: Option<Value>) -> Response {
    let mut body = json!({
        "name": "multi-tenant",
        "tenant_ids": tenant_ids,
        "scopes": ["api"],
    });
    if let Some(project_ids) = project_ids {
        body["project_ids"] = project_ids;
    }
    app.post_json_with_session("/v1/personal_tokens", body)
        .await
}

async fn token_count(app: &TestApp, user_id: Uuid) -> u64 {
    entity::personal_tokens::Entity::find()
        .filter(entity::personal_tokens::Column::UserId.eq(user_id))
        .count(&app.state.db)
        .await
        .expect("count tokens")
}

async fn status_of(app: &TestApp, path: &str, token: &str) -> StatusCode {
    app.get_with_bearer(path, token).await.status()
}

fn projects_path(tenant_id: Uuid) -> String {
    format!("/v1/tenants/{tenant_id}/projects")
}

fn sorted(ids: &[Uuid]) -> Vec<String> {
    let mut ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn token_bound_to_two_tenants_works_in_both_and_not_in_a_third() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let a = insert_tenant(&app.state.db, user.id).await;
    let b = insert_tenant(&app.state.db, user.id).await;
    let c = insert_tenant(&app.state.db, user.id).await;
    login(&mut app, &user).await;

    let res = create_token(&app, json!([a, b]), None).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let created = json_body(res).await;
    assert_eq!(created["tenant_ids"], json!(sorted(&[a, b])));
    let token = created["token"].as_str().expect("token").to_string();

    assert_eq!(
        status_of(&app, &projects_path(a), &token).await,
        StatusCode::OK
    );
    assert_eq!(
        status_of(&app, &projects_path(b), &token).await,
        StatusCode::OK
    );
    assert_eq!(
        status_of(&app, &projects_path(c), &token).await,
        StatusCode::FORBIDDEN,
        "束縛に無いテナントは、利用者がオーナーでも 403"
    );

    // テナント一覧は束縛したテナントだけ（対照: セッションは 3 つ見える）
    let listed = json_body(app.get_with_bearer("/v1/tenants", &token).await).await;
    let mut ids: Vec<String> = listed
        .as_array()
        .expect("tenants")
        .iter()
        .map(|t| t["id"].as_str().expect("id").to_string())
        .collect();
    ids.sort();
    assert_eq!(ids, sorted(&[a, b]));
    let session_listed = json_body(app.get_with_session("/v1/tenants").await).await;
    assert_eq!(session_listed.as_array().expect("tenants").len(), 3);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn creation_is_rejected_when_any_selected_tenant_fails_the_admin_check() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let other = app.insert_user_default().await;
    let a = insert_tenant(&app.state.db, user.id).await;
    let b = insert_tenant(&app.state.db, other.id).await;
    add_member(&app, b, user.id, TenantRole::Member).await;
    login(&mut app, &user).await;

    let res = create_token(&app, json!([a, b]), None).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        token_count(&app, user.id).await,
        0,
        "一部のテナントだけ束縛したトークンを作らない"
    );

    // 対照: B で Admin なら 201
    let member = entity::tenant_members::Entity::find()
        .filter(entity::tenant_members::Column::TenantId.eq(b))
        .filter(entity::tenant_members::Column::UserId.eq(user.id))
        .one(&app.state.db)
        .await
        .expect("find member")
        .expect("member");
    let mut active: entity::tenant_members::ActiveModel = member.into();
    active.role = Set(TenantRole::Admin);
    active.update(&app.state.db).await.expect("promote");
    let res = create_token(&app, json!([a, b]), None).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(token_count(&app, user.id).await, 1);

    app.cleanup_user(user.id).await;
    app.cleanup_user(other.id).await;
}

#[tokio::test]
async fn creation_checks_2fa_on_every_selected_tenant() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let a = insert_tenant(&app.state.db, user.id).await;
    let b = insert_tenant(&app.state.db, user.id).await;
    // 強制の前に本認証の session を取る
    login(&mut app, &user).await;
    let tenant = entity::tenants::Entity::find_by_id(b)
        .one(&app.state.db)
        .await
        .expect("find tenant")
        .expect("tenant");
    let mut active: entity::tenants::ActiveModel = tenant.into();
    active.require_2fa = Set(true);
    active.update(&app.state.db).await.expect("enforce 2fa");

    let res = create_token(&app, json!([a, b]), None).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(json_body(res).await["message"], "2fa-setup-required");
    assert_eq!(token_count(&app, user.id).await, 0);

    // 対照: 2FA を強制しない A だけなら通る
    let res = create_token(&app, json!([a]), None).await;
    assert_eq!(res.status(), StatusCode::CREATED);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn creation_validates_the_tenant_count() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    login(&mut app, &user).await;

    let mut tenants = Vec::new();
    for _ in 0..20 {
        tenants.push(insert_tenant(&app.state.db, user.id).await);
    }

    assert_eq!(
        create_token(&app, json!([]), None).await.status(),
        StatusCode::BAD_REQUEST,
        "0 件は検証エラー（400）"
    );
    let mut over = tenants.clone();
    over.push(insert_tenant(&app.state.db, user.id).await);
    assert_eq!(
        create_token(&app, json!(over), None).await.status(),
        StatusCode::BAD_REQUEST,
        "21 件は検証エラー（400）"
    );
    assert_eq!(token_count(&app, user.id).await, 0);

    let res = create_token(&app, json!(tenants), None).await;
    assert_eq!(res.status(), StatusCode::CREATED, "20 件（境界）は通る");
    assert_eq!(
        json_body(res).await["tenant_ids"]
            .as_array()
            .expect("tenant_ids")
            .len(),
        20
    );

    let res = create_token(&app, json!([tenants[0], tenants[0]]), None).await;
    assert_eq!(res.status(), StatusCode::CREATED, "重複は 1 件に畳む");
    assert_eq!(
        json_body(res).await["tenant_ids"],
        json!([tenants[0].to_string()])
    );

    let res = create_token(&app, json!([tenants[0], Uuid::new_v4()]), None).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND, "無いテナントは 404");

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn leaving_one_tenant_blocks_only_that_tenant() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let other = app.insert_user_default().await;
    let a = insert_tenant(&app.state.db, user.id).await;
    let b = insert_tenant(&app.state.db, other.id).await;
    add_member(&app, b, user.id, TenantRole::Admin).await;
    login(&mut app, &user).await;
    let created = json_body(create_token(&app, json!([a, b]), None).await).await;
    let token = created["token"].as_str().expect("token").to_string();
    assert_eq!(
        status_of(&app, &projects_path(b), &token).await,
        StatusCode::OK
    );

    entity::tenant_members::Entity::delete_many()
        .filter(entity::tenant_members::Column::TenantId.eq(b))
        .filter(entity::tenant_members::Column::UserId.eq(user.id))
        .exec(&app.state.db)
        .await
        .expect("leave tenant");

    let res = app.get_with_bearer(&projects_path(b), &token).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(json_body(res).await["message"], "tenant-membership-missing");
    assert_eq!(
        status_of(&app, &projects_path(a), &token).await,
        StatusCode::OK,
        "残ったテナントは使える"
    );

    // 戻れば回復する（束縛の行は変えていない）
    add_member(&app, b, user.id, TenantRole::Admin).await;
    assert_eq!(
        status_of(&app, &projects_path(b), &token).await,
        StatusCode::OK
    );

    app.cleanup_user(user.id).await;
    app.cleanup_user(other.id).await;
}

#[tokio::test]
async fn project_limited_token_spans_tenants() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tp_a = app.insert_tenant_project(user.id).await;
    let tp_b = app.insert_tenant_project(user.id).await;
    let tp_c = app.insert_tenant_project(user.id).await;
    let (a, b) = (tp_a.tenant_id, tp_b.tenant_id);
    let (pa, pb) = (tp_a.project_id, tp_b.project_id);
    let other_in_a = insert_extra_project(&app, a).await;
    login(&mut app, &user).await;

    let res = create_token(&app, json!([a, b]), Some(json!([pa, pb]))).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let token = json_body(res).await["token"]
        .as_str()
        .expect("token")
        .to_string();
    let project_path = |t: Uuid, p: Uuid| format!("/v1/tenants/{t}/projects/{p}");
    assert_eq!(
        status_of(&app, &project_path(a, pa), &token).await,
        StatusCode::OK
    );
    assert_eq!(
        status_of(&app, &project_path(b, pb), &token).await,
        StatusCode::OK
    );
    assert_eq!(
        status_of(&app, &project_path(a, other_in_a), &token).await,
        StatusCode::FORBIDDEN,
        "選んでいないプロジェクトは 403"
    );
    assert_eq!(
        status_of(&app, &projects_path(a), &token).await,
        StatusCode::FORBIDDEN,
        "プロジェクト制限付きはテナント全体の操作ができない"
    );

    // プロジェクトを 1 つも持たないテナントが混ざる束縛は作れない
    let res = create_token(&app, json!([a, b]), Some(json!([pa]))).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    // 選んだテナントに属さないプロジェクトは 403
    let res = create_token(&app, json!([a, b]), Some(json!([pa, pb, tp_c.project_id]))).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(token_count(&app, user.id).await, 1);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn deleting_a_tenant_keeps_the_token_for_the_others_and_kills_it_when_none_remain() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let admin = app.insert_user(true, false).await;
    let a = insert_tenant(&app.state.db, user.id).await;
    let b = insert_tenant(&app.state.db, user.id).await;
    let both = app
        .insert_pat_in_tenants(user.id, &[a, b], vec![Scope::Api], None)
        .await;
    let only_a = app
        .insert_pat_in_tenants(user.id, &[a], vec![Scope::Api], None)
        .await;

    login(&mut app, &admin).await;
    let res = app
        .delete_with_session(&format!("/v1/admin/tenants/{a}"))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    assert_eq!(
        status_of(&app, &projects_path(b), &both).await,
        StatusCode::OK,
        "残ったテナントでは通る"
    );
    let me = json_body(app.get_with_bearer("/v1/personal_tokens/me", &both).await).await;
    assert_eq!(me["tenant_ids"], json!([b.to_string()]));
    assert_eq!(
        status_of(&app, "/v1/personal_tokens/me", &only_a).await,
        StatusCode::UNAUTHORIZED,
        "束縛が尽きたトークンは 401"
    );

    login(&mut app, &user).await;
    let listed = json_body(app.get_with_session("/v1/personal_tokens").await).await;
    let listed = listed.as_array().expect("tokens");
    assert_eq!(listed.len(), 1, "束縛の無いトークンは一覧に出さない");
    assert_eq!(listed[0]["tenant_ids"], json!([b.to_string()]));

    app.cleanup_user(user.id).await;
    app.cleanup_user(admin.id).await;
}

async fn insert_notification(app: &TestApp, user_id: Uuid, project_id: Uuid) {
    entity::notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        task_id: Set(None),
        project_id: Set(Some(project_id)),
        notification_type: Set("review_round_created".into()),
        payload: Set(json!({ "pr_number": 491, "round": 1 })),
        read_at: Set(None),
        created_at: Set(chrono::Utc::now().into()),
        email_queued_at: Set(None),
        emailed_at: Set(None),
        email_attempts: Set(0),
    }
    .insert(&app.state.db)
    .await
    .expect("insert notification");
}

#[tokio::test]
async fn notifications_cover_every_bound_tenant_only() {
    let app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let a = app.insert_tenant_project(user.id).await;
    let b = app.insert_tenant_project(user.id).await;
    let c = app.insert_tenant_project(user.id).await;
    for tp in [&a, &b, &c] {
        insert_notification(&app, user.id, tp.project_id).await;
    }
    let token = app
        .insert_pat_in_tenants(
            user.id,
            &[a.tenant_id, b.tenant_id],
            vec![Scope::ReadReview],
            None,
        )
        .await;

    let body = json_body(
        app.get_with_bearer("/v1/users/me/notifications", &token)
            .await,
    )
    .await;
    let mut seen: Vec<String> = body["notifications"]
        .as_array()
        .expect("notifications")
        .iter()
        .map(|n| n["project_id"].as_str().expect("project_id").to_string())
        .collect();
    seen.sort();
    assert_eq!(seen, sorted(&[a.project_id, b.project_id]));
    assert_eq!(body["unread_count"].as_u64(), Some(2));

    app.cleanup_user(user.id).await;
}

/// 既存トークンは、移行後も元の 1 テナントにだけ結び付く。移行の SQL は 2 回流しても増えない。
/// マイグレーションの前の形（`tenant_id` 列）をトランザクション内で作り、巻き戻す。
#[tokio::test]
async fn backfill_binds_existing_tokens_to_their_single_tenant() {
    let app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let a = insert_tenant(&app.state.db, user.id).await;
    let b = insert_tenant(&app.state.db, user.id).await;
    app.insert_pat(user.id, a, vec![Scope::Api], None).await;
    app.insert_pat(user.id, b, vec![Scope::Api], None).await;
    // 移行前のトークンと、束縛されたテナントの対
    let mut expected = Vec::new();
    for tenant_id in [a, b] {
        let binding = entity::personal_token_tenants::Entity::find()
            .filter(entity::personal_token_tenants::Column::TenantId.eq(tenant_id))
            .one(&app.state.db)
            .await
            .expect("binding")
            .expect("bound token");
        expected.push((binding.token_id, tenant_id));
    }

    let txn = app.state.db.begin().await.expect("begin");
    txn.execute_unprepared("ALTER TABLE personal_tokens ADD COLUMN tenant_id UUID")
        .await
        .expect("add legacy column");
    // 旧い形: 束縛は列だけにあり、中間テーブルは空。同じ DB に残る他のテストのトークンも、
    // 旧い形では必ずテナントを持っていたので、持たないもの（テナントの削除で束縛が尽きた行）は消す
    txn.execute_unprepared(
        "UPDATE personal_tokens SET tenant_id = (
             SELECT MIN(tenant_id::text)::uuid FROM personal_token_tenants
             WHERE token_id = personal_tokens.id
         )",
    )
    .await
    .expect("set legacy tenant_id");
    txn.execute_unprepared("DELETE FROM personal_tokens WHERE tenant_id IS NULL")
        .await
        .expect("drop unbound tokens");
    txn.execute_unprepared("DELETE FROM personal_token_tenants")
        .await
        .expect("clear bindings");

    txn.execute_unprepared(BACKFILL_SQL)
        .await
        .expect("backfill");
    txn.execute_unprepared(BACKFILL_SQL)
        .await
        .expect("backfill is idempotent");

    for (token_id, tenant_id) in expected {
        let rows = entity::personal_token_tenants::Entity::find()
            .filter(entity::personal_token_tenants::Column::TokenId.eq(token_id))
            .all(&txn)
            .await
            .expect("bindings");
        assert_eq!(rows.len(), 1, "1 テナントに 1 行");
        assert_eq!(rows[0].tenant_id, tenant_id);
    }

    txn.rollback().await.expect("rollback");
    app.cleanup_user(user.id).await;
}
