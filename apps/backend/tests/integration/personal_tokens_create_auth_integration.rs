use crate::common::{
    TestApp, TestUser, create_status, insert_extra_project, insert_tenant, json_body,
};
use axum::http::StatusCode;
use entity::project_members::ProjectRole;
use entity::tenant_members::TenantRole;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

/// tenant_members に指定ロールで一行挿す。
async fn insert_tenant_member(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    role: TenantRole,
) {
    entity::tenant_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        tenant_id: Set(tenant_id),
        user_id: Set(user_id),
        role: Set(role),
        department: Set(None),
        position: Set(None),
        team: Set(None),
    }
    .insert(db)
    .await
    .expect("insert tenant member");
}

/// その利用者の session で鍵の発行を試み、status を返す。
async fn try_create_token(app: &mut TestApp, user: &TestUser, tenant_id: Uuid) -> StatusCode {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let res = app
        .post_json_with_session(
            "/v1/personal_tokens",
            serde_json::json!({
                "name": "boundary-check",
                "tenant_ids": [tenant_id],
                "scopes": ["read:task"],
            }),
        )
        .await;
    res.status()
}

/// `try_create_token` と同じ要求で、status と本文の `message` を返す（拒否の理由を固定する）。
async fn try_create_token_with_message(
    app: &mut TestApp,
    user: &TestUser,
    tenant_id: Uuid,
) -> (StatusCode, String) {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let res = app
        .post_json_with_session("/v1/personal_tokens", token_body(tenant_id))
        .await;
    let status = res.status();
    let message = json_body(res).await["message"]
        .as_str()
        .expect("message")
        .to_string();
    (status, message)
}

/// その利用者の session で、指定の scope の鍵を発行 API で作り、平文を返す。
async fn create_token_with_scopes(
    app: &mut TestApp,
    user: &TestUser,
    tenant_id: Uuid,
    scopes: &[&str],
) -> String {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let res = app
        .post_json_with_session(
            "/v1/personal_tokens",
            serde_json::json!({
                "name": "boundary-check",
                "tenant_ids": [tenant_id],
                "scopes": scopes,
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "鍵の発行");
    json_body(res).await["token"]
        .as_str()
        .expect("token")
        .to_string()
}

/// テナントから外す（`tenant_members` の行を消す。`project_members` には触れない）。
async fn remove_tenant_member(db: &DatabaseConnection, tenant_id: Uuid, user_id: Uuid) {
    entity::tenant_members::Entity::delete_many()
        .filter(entity::tenant_members::Column::TenantId.eq(tenant_id))
        .filter(entity::tenant_members::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .expect("remove tenant member");
}

async fn token_rows(db: &DatabaseConnection, user_id: Uuid) -> Vec<entity::personal_tokens::Model> {
    entity::personal_tokens::Entity::find()
        .filter(entity::personal_tokens::Column::UserId.eq(user_id))
        .all(db)
        .await
        .expect("find tokens")
}

fn tasks_path(tenant_id: Uuid, project_id: Uuid) -> String {
    format!("/v1/tenants/{tenant_id}/projects/{project_id}/tasks")
}

/// テナントの 2FA を強制にする（強制前に取った session は残したまま）。
async fn enforce_2fa(db: &DatabaseConnection, tenant_id: Uuid) {
    let tenant = entity::tenants::Entity::find_by_id(tenant_id)
        .one(db)
        .await
        .expect("find tenant")
        .expect("tenant exists");
    let mut active: entity::tenants::ActiveModel = tenant.into();
    active.require_2fa = Set(true);
    active.update(db).await.expect("enforce 2fa");
}

fn token_body(tenant_id: Uuid) -> serde_json::Value {
    serde_json::json!({
        "name": "boundary-check",
        "tenant_ids": [tenant_id],
        "scopes": ["read:task"],
    })
}

/// 2FA を強制するテナントでは、2FA を設定していない Admin は強制前に取った session でも
/// 鍵を作れない（メンバー管理の口と同じく 403 の 2fa-setup-required）。修正前は 201 で通っていた。
#[tokio::test]
async fn admin_without_2fa_cannot_create_token_when_tenant_requires_2fa() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, admin.id, TenantRole::Admin).await;

    // 強制の前に本認証の session を取る
    app.reset_session_client();
    app.login_session_no_content(&admin.email, &admin.password)
        .await;
    enforce_2fa(&app.state.db, tenant_id).await;

    let res = app
        .post_json_with_session("/v1/personal_tokens", token_body(tenant_id))
        .await;
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "強制の下で 2FA が無ければ 403"
    );
    let body: serde_json::Value = res.json().await.expect("error body");
    assert_eq!(body["message"], "2fa-setup-required", "理由を名指しする");
}

/// 対照: 2FA を設定済みの Admin は、強制するテナントでも鍵を作れる。
#[tokio::test]
async fn admin_with_2fa_can_create_token_when_tenant_requires_2fa() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, admin.id, TenantRole::Admin).await;
    let admin_2fa = app.enable_2fa(&admin).await;
    enforce_2fa(&app.state.db, tenant_id).await;

    app.reset_session_client();
    app.login_half_authed(&admin).await;
    let verified = app
        .post_json_with_session(
            "/v1/auth/2fa/verify",
            serde_json::json!({ "recovery_code": admin_2fa.recovery_codes[0] }),
        )
        .await;
    assert_eq!(
        verified.status(),
        StatusCode::NO_CONTENT,
        "2FA 検証で本認証になる"
    );

    let res = app
        .post_json_with_session("/v1/personal_tokens", token_body(tenant_id))
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "2FA 済みなら 201");
}

/// 主は鍵を作れる。
#[tokio::test]
async fn owner_can_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    assert_eq!(
        try_create_token(&mut app, &owner, tenant_id).await,
        StatusCode::CREATED,
        "主は 201"
    );
}

/// role が Admin の member は鍵を作れる（この境目がこの司令で広がった当のもの）。
#[tokio::test]
async fn admin_member_can_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, admin.id, TenantRole::Admin).await;
    assert_eq!(
        try_create_token(&mut app, &admin, tenant_id).await,
        StatusCode::CREATED,
        "テナント Admin は 201"
    );
}

/// role が Member の member も鍵を作れる（TASK-566。発行の条件は所属と 2FA の強制だけ）。
/// 修正前は 403 だった。
#[tokio::test]
async fn plain_member_can_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, member.id, TenantRole::Member).await;
    assert_eq!(
        try_create_token(&mut app, &member, tenant_id).await,
        StatusCode::CREATED,
        "Member は 201"
    );
}

/// role が Viewer の member も鍵を作れる。修正前は 403 だった。
#[tokio::test]
async fn viewer_member_can_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let viewer = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, viewer.id, TenantRole::Viewer).await;
    assert_eq!(
        try_create_token(&mut app, &viewer, tenant_id).await,
        StatusCode::CREATED,
        "Viewer は 201"
    );
}

/// テナントに属さぬ者は作れぬ（Member に開いたあとも、所属の無い者には開かない）。
#[tokio::test]
async fn outsider_cannot_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let outsider = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    assert_eq!(
        try_create_token_with_message(&mut app, &outsider, tenant_id).await,
        (
            StatusCode::FORBIDDEN,
            "tenant-membership-missing".to_string()
        ),
        "非所属は 403"
    );
}

/// project-only の客分（project_members のみ、tenant_members 無し）は作れぬ。
#[tokio::test]
async fn project_only_guest_cannot_create_token() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let guest = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    entity::project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(tp.project_id),
        user_id: Set(guest.id),
        role: Set(ProjectRole::Member),
    }
    .insert(&app.state.db)
    .await
    .expect("insert project member");
    assert_eq!(
        try_create_token_with_message(&mut app, &guest, tp.tenant_id).await,
        (
            StatusCode::FORBIDDEN,
            "tenant-membership-missing".to_string()
        ),
        "客分は 403"
    );
}

/// 2FA を強制するテナントでは、2FA を設定していない Member は鍵を作れない
/// （Member に開いても、2FA の強制は発行の条件に残る）。
#[tokio::test]
async fn member_without_2fa_cannot_create_token_when_tenant_requires_2fa() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, member.id, TenantRole::Member).await;

    // 強制の前に本認証の session を取る
    app.reset_session_client();
    app.login_session_no_content(&member.email, &member.password)
        .await;
    enforce_2fa(&app.state.db, tenant_id).await;

    let res = app
        .post_json_with_session("/v1/personal_tokens", token_body(tenant_id))
        .await;
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "強制の下で 2FA が無ければ 403"
    );
    assert_eq!(json_body(res).await["message"], "2fa-setup-required");
    assert_eq!(token_rows(&app.state.db, member.id).await.len(), 0);
}

/// 対照: 2FA を設定済みの Member は、強制するテナントでも鍵を作れる。
#[tokio::test]
async fn member_with_2fa_can_create_token_when_tenant_requires_2fa() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, member.id, TenantRole::Member).await;
    let member_2fa = app.enable_2fa(&member).await;
    enforce_2fa(&app.state.db, tenant_id).await;

    app.reset_session_client();
    app.login_half_authed(&member).await;
    let verified = app
        .post_json_with_session(
            "/v1/auth/2fa/verify",
            serde_json::json!({ "recovery_code": member_2fa.recovery_codes[0] }),
        )
        .await;
    assert_eq!(
        verified.status(),
        StatusCode::NO_CONTENT,
        "2FA 検証で本認証になる"
    );

    let res = app
        .post_json_with_session("/v1/personal_tokens", token_body(tenant_id))
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "2FA 済みなら 201");
}

/// Member のトークンは Member にできることしかできない。発行のときにロールで scope は絞らないが、
/// ロールで守る操作は要求のたびに本人の今のロールで判定される。
#[tokio::test]
async fn member_token_with_write_scopes_cannot_do_admin_operations() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let invitee = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    insert_tenant_member(&app.state.db, tp.tenant_id, member.id, TenantRole::Member).await;
    let scopes = ["write:tenant", "write:project", "write:task"];

    // 状態の作成とオーナーの鍵の発行は、オーナーのセッションで行う
    let owner_token = create_token_with_scopes(&mut app, &owner, tp.tenant_id, &scopes).await;
    let status_id = create_status(&app, &tp, "Todo", false).await;
    let member_token = create_token_with_scopes(&mut app, &member, tp.tenant_id, &scopes).await;

    let members_path = format!("/v1/tenants/{}/members", tp.tenant_id);
    let add_member = serde_json::json!({ "user_id": invitee.id, "role": "Member" });
    let res = app
        .post_json_with_bearer(&members_path, add_member.clone(), &member_token)
        .await;
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "メンバーの追加は Admin 以上"
    );
    assert_eq!(
        json_body(res).await["message"],
        "forbidden",
        "scope や所属ではなく、ロールで拒否している"
    );

    let projects_path = format!("/v1/tenants/{}/projects", tp.tenant_id);
    let new_project = serde_json::json!({ "name": "member-made" });
    let res = app
        .post_json_with_bearer(&projects_path, new_project.clone(), &member_token)
        .await;
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "共有プロジェクトの作成は Admin 以上"
    );
    assert_eq!(json_body(res).await["message"], "forbidden");

    // 対照: Member にできる操作は、同じ鍵で通る
    let res = app
        .post_json_with_bearer(
            &tasks_path(tp.tenant_id, tp.project_id),
            serde_json::json!({ "title": "member-task", "status_id": status_id }),
            &member_token,
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "タスクの作成は通る");

    // 対照: 同じ scope・同じ本文でも、オーナーの鍵ならメンバーを追加できる
    let res = app
        .post_json_with_bearer(&members_path, add_member, &owner_token)
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "オーナーの鍵は通る");
    // 共有プロジェクトの作成も同じ（403 が scope 不足ではなく、ロールの拒否であることの対照）
    let res = app
        .post_json_with_bearer(&projects_path, new_project, &owner_token)
        .await;
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "オーナーの鍵は共有プロジェクトを作れる"
    );
}

/// テナントから外した Member の鍵は、そのテナント全体の操作で 403 になる。
/// 鍵は取り消さず、戻れば再び使える。
#[tokio::test]
async fn removed_member_token_is_blocked_on_tenant_wide_operations_until_readded() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    insert_tenant_member(&app.state.db, tenant_id, member.id, TenantRole::Member).await;
    let token = create_token_with_scopes(&mut app, &member, tenant_id, &["read:tenant"]).await;
    let tenant_path = format!("/v1/tenants/{tenant_id}");
    assert_eq!(
        app.get_with_bearer(&tenant_path, &token).await.status(),
        StatusCode::OK,
        "所属している間は通る"
    );

    remove_tenant_member(&app.state.db, tenant_id, member.id).await;

    let res = app.get_with_bearer(&tenant_path, &token).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(json_body(res).await["message"], "tenant-membership-missing");
    let rows = token_rows(&app.state.db, member.id).await;
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].revoked, "離脱で鍵は取り消さない");

    insert_tenant_member(&app.state.db, tenant_id, member.id, TenantRole::Member).await;
    assert_eq!(
        app.get_with_bearer(&tenant_path, &token).await.status(),
        StatusCode::OK,
        "戻れば回復する"
    );
}

/// テナントから外したあとも、共有プロジェクトの `project_members` に明示の行が残っていれば、
/// そのプロジェクトの中だけは同じ鍵で通る（既存の規則の固定）。
#[tokio::test]
async fn removed_member_token_still_reaches_projects_with_an_explicit_member_row() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    let other_project = insert_extra_project(&app, tp.tenant_id).await;
    insert_tenant_member(&app.state.db, tp.tenant_id, member.id, TenantRole::Member).await;
    entity::project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(tp.project_id),
        user_id: Set(member.id),
        role: Set(ProjectRole::Member),
    }
    .insert(&app.state.db)
    .await
    .expect("insert project member");
    let token = create_token_with_scopes(
        &mut app,
        &member,
        tp.tenant_id,
        &["read:tenant", "read:task"],
    )
    .await;
    assert_eq!(
        app.get_with_bearer(&tasks_path(tp.tenant_id, other_project), &token)
            .await
            .status(),
        StatusCode::OK,
        "所属している間は、明示の行が無い共有プロジェクトにも入れる"
    );

    remove_tenant_member(&app.state.db, tp.tenant_id, member.id).await;

    assert_eq!(
        app.get_with_bearer(&tasks_path(tp.tenant_id, tp.project_id), &token)
            .await
            .status(),
        StatusCode::OK,
        "明示の行が残るプロジェクトは通る"
    );
    let res = app
        .get_with_bearer(&tasks_path(tp.tenant_id, other_project), &token)
        .await;
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "明示の行が無いプロジェクトは 403"
    );
    assert_eq!(json_body(res).await["message"], "tenant-membership-missing");
    let res = app
        .get_with_bearer(&format!("/v1/tenants/{}", tp.tenant_id), &token)
        .await;
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "テナント全体の操作は 403"
    );
    assert_eq!(json_body(res).await["message"], "tenant-membership-missing");
}

/// Admin のときに作った鍵は、Member に下げたあとは Member にできることしかできない。
/// 鍵は取り消さない。
#[tokio::test]
async fn demoted_admin_token_loses_admin_operations_but_stays_valid() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let first = app.insert_user_default().await;
    let second = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    insert_tenant_member(&app.state.db, tp.tenant_id, admin.id, TenantRole::Admin).await;
    let token = create_token_with_scopes(
        &mut app,
        &admin,
        tp.tenant_id,
        &["write:tenant", "read:task"],
    )
    .await;
    let members_path = format!("/v1/tenants/{}/members", tp.tenant_id);

    // 対照: Admin の間はメンバーを追加できる
    let res = app
        .post_json_with_bearer(
            &members_path,
            serde_json::json!({ "user_id": first.id, "role": "Member" }),
            &token,
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "Admin の間は通る");

    let row = entity::tenant_members::Entity::find()
        .filter(entity::tenant_members::Column::TenantId.eq(tp.tenant_id))
        .filter(entity::tenant_members::Column::UserId.eq(admin.id))
        .one(&app.state.db)
        .await
        .expect("find member")
        .expect("member");
    let mut active: entity::tenant_members::ActiveModel = row.into();
    active.role = Set(TenantRole::Member);
    active.update(&app.state.db).await.expect("demote");

    let res = app
        .post_json_with_bearer(
            &members_path,
            serde_json::json!({ "user_id": second.id, "role": "Member" }),
            &token,
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "降格のあとは次の要求から 403"
    );
    assert_eq!(json_body(res).await["message"], "forbidden");
    assert_eq!(
        app.get_with_bearer(&tasks_path(tp.tenant_id, tp.project_id), &token)
            .await
            .status(),
        StatusCode::OK,
        "Member にできる操作は通る"
    );
    let rows = token_rows(&app.state.db, admin.id).await;
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].revoked, "降格で鍵は取り消さない");
}
