use crate::common::{TestApp, TestUser};
use apalis::prelude::Data;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use entity::{tenant_invitations, users};
use job::tenant_invitation_email::{TenantInvitationEmailJob, process};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use serde_json::{Value, json};
use service::tenant_invitations::send_slot_key;
use uuid::Uuid;

// テナントへのメール招待（TASK-184）の統合テスト。
// 仕様は apps/backend/docs/tenant-project-authz.md の「招待」。

fn job_state(app: &TestApp) -> job::JobState {
    job::JobState {
        settings: app.state.settings.clone(),
        db: app.state.db.clone(),
        redis_client: app.state.redis_client.clone(),
        smtp_client: app.state.smtp_client.clone(),
        http_client: app.state.http_client.clone(),
        review_summary_storage: app.state.review_summary_storage.clone(),
        pg_pool: app.state.pg_pool.clone(),
    }
}

fn invitations_path(tenant_id: Uuid) -> String {
    format!("/v1/tenants/{tenant_id}/invitations")
}

fn unique_email() -> String {
    format!("invitee-{}@example.com", Uuid::new_v4())
}

async fn login(app: &mut TestApp, user: &TestUser) {
    app.reset_session_client();
    app.login_session(&user.email, &user.password).await;
}

async fn invite(app: &TestApp, tenant_id: Uuid, email: &str, role: &str) -> reqwest::Response {
    app.post_json_with_session(
        &invitations_path(tenant_id),
        json!({ "email": email, "role": role }),
    )
    .await
}

async fn invite_ok(app: &TestApp, tenant_id: Uuid, email: &str, role: &str) -> Uuid {
    let res = invite(app, tenant_id, email, role).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: Value = res.json().await.expect("invitation json");
    Uuid::parse_str(body["id"].as_str().expect("invitation id")).expect("uuid")
}

async fn current_generation(app: &TestApp, invitation_id: Uuid) -> i32 {
    tenant_invitations::Entity::find_by_id(invitation_id)
        .one(&app.state.db)
        .await
        .expect("find invitation")
        .expect("invitation exists")
        .generation
}

async fn run_job(app: &TestApp, invitation_id: Uuid, generation: i32) {
    process(
        TenantInvitationEmailJob::new(invitation_id, generation),
        Data::new(job_state(app)),
    )
    .await
    .expect("invitation email job");
}

/// 今の世代の送信ジョブを 1 回走らせ、届いたメールのリンクからトークンを取り出す。
async fn send_and_take_token(app: &TestApp, invitation_id: Uuid, email: &str) -> String {
    run_job(
        app,
        invitation_id,
        current_generation(app, invitation_id).await,
    )
    .await;
    let mail = app
        .sent_mails()
        .into_iter()
        .rev()
        .find(|m| m.to == email)
        .expect("invitation mail was sent");
    let start = mail.text.find("token=").expect("mail has accept link") + "token=".len();
    mail.text[start..]
        .split_whitespace()
        .next()
        .expect("token")
        .to_string()
}

async fn clear_send_slot(app: &TestApp, tenant_id: Uuid, email: &str) {
    let mut conn = app
        .state
        .redis_client
        .conn
        .acquire()
        .await
        .expect("redis acquire");
    let _: () = redis::cmd("DEL")
        .arg(send_slot_key(tenant_id, email))
        .query_async(&mut conn)
        .await
        .expect("redis DEL");
}

async fn accept(app: &TestApp, token: &str) -> reqwest::Response {
    app.post_json_with_session("/v1/invitations/accept", json!({ "token": token }))
        .await
}

/// 宛先のアドレスを持つ利用者を作る（招待の後に登録した人に当たる）。
async fn register_as(app: &TestApp, email: &str) -> TestUser {
    let mut user = app.insert_user(false, false).await;
    let mut active: users::ActiveModel = users::Entity::find_by_id(user.id)
        .one(&app.state.db)
        .await
        .expect("find user")
        .expect("user exists")
        .into();
    active.email = Set(email.to_string());
    active.update(&app.state.db).await.expect("update email");
    user.email = email.to_string();
    user
}

#[tokio::test]
async fn unregistered_invitee_registers_then_joins_with_invited_role() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let email = unique_email();

    login(&mut app, &owner).await;
    let invitation_id = invite_ok(&app, tp.tenant_id, &email, "Admin").await;
    let token = send_and_take_token(&app, invitation_id, &email).await;

    // 未登録でも、承諾画面の中身はトークンだけで見られる
    app.reset_session_client();
    let preview = app
        .post_json("/v1/invitations/preview", json!({ "token": token }))
        .await;
    assert_eq!(preview.status(), StatusCode::OK);
    let preview: Value = preview.json().await.expect("preview json");
    assert_eq!(preview["email"], email.as_str());
    assert_eq!(preview["role"], "Admin");
    assert_eq!(preview["tenant_id"], tp.tenant_id.to_string());

    // 承諾はログインが要る
    assert_eq!(
        accept(&app, &token).await.status(),
        StatusCode::UNAUTHORIZED
    );

    // 招待されたアドレスで登録してから承諾すると、招待のロールで入る
    let invitee = register_as(&app, &email).await;
    login(&mut app, &invitee).await;
    let accepted = accept(&app, &token).await;
    assert_eq!(accepted.status(), StatusCode::CREATED);
    let member: Value = accepted.json().await.expect("member json");
    assert_eq!(member["user_id"], invitee.id.to_string());
    assert_eq!(member["role"], "Admin");
    assert_eq!(
        app.get_with_session(&format!("/v1/tenants/{}", tp.tenant_id))
            .await
            .status(),
        StatusCode::OK,
        "承諾した人はテナントに入れる"
    );

    // 使用済みのトークンはもう使えない。招待も一覧から消える
    assert_eq!(accept(&app, &token).await.status(), StatusCode::NOT_FOUND);
    login(&mut app, &owner).await;
    let listed: Value = app
        .get_with_session(&invitations_path(tp.tenant_id))
        .await
        .json()
        .await
        .expect("list json");
    assert_eq!(listed, json!([]));
}

#[tokio::test]
async fn accept_is_limited_to_the_invited_address() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let other = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let email = unique_email();

    login(&mut app, &owner).await;
    let invitation_id = invite_ok(&app, tp.tenant_id, &email, "Member").await;
    let token = send_and_take_token(&app, invitation_id, &email).await;

    // リンクを受け取った別人は入れない
    login(&mut app, &other).await;
    let res = accept(&app, &token).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        res.json::<Value>().await.expect("error json")["message"],
        "invitation-email-mismatch"
    );

    // 対照: 宛先の本人なら入れる（拒否で招待が消えていない）
    let invitee = register_as(&app, &email).await;
    login(&mut app, &invitee).await;
    assert_eq!(accept(&app, &token).await.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn expired_revoked_and_reissued_tokens_are_rejected() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let email = unique_email();
    let invitee = register_as(&app, &email).await;

    login(&mut app, &owner).await;
    let invitation_id = invite_ok(&app, tp.tenant_id, &email, "Member").await;
    let old_token = send_and_take_token(&app, invitation_id, &email).await;

    // 期限切れは 410
    let row = tenant_invitations::Entity::find_by_id(invitation_id)
        .one(&app.state.db)
        .await
        .expect("find invitation")
        .expect("invitation exists");
    let mut active: tenant_invitations::ActiveModel = row.into();
    active.expires_at = Set((Utc::now() - Duration::minutes(1)).into());
    active
        .update(&app.state.db)
        .await
        .expect("expire invitation");
    login(&mut app, &invitee).await;
    assert_eq!(accept(&app, &old_token).await.status(), StatusCode::GONE);

    // 期限切れのまま送信ジョブが回ってもメールは出ない
    let mails_before = app.sent_mails().len();
    let old_generation = current_generation(&app, invitation_id).await;
    run_job(&app, invitation_id, old_generation).await;
    assert_eq!(app.sent_mails().len(), mails_before);

    // 再送で期限が延び、前のリンクは使えなくなる
    login(&mut app, &owner).await;
    clear_send_slot(&app, tp.tenant_id, &email).await;
    let resent = app
        .post_json_with_session(
            &format!("{}/{invitation_id}/resend", invitations_path(tp.tenant_id)),
            json!({}),
        )
        .await;
    assert_eq!(resent.status(), StatusCode::OK);
    // 再送の前に積まれた古い世代のジョブは、後から回ってきても送らない
    // （新しい世代のジョブが送る。到着順が逆転して新しいリンクを古いメールが追い越さない）
    run_job(&app, invitation_id, old_generation).await;
    assert_eq!(app.sent_mails().len(), mails_before);
    let new_token = send_and_take_token(&app, invitation_id, &email).await;
    assert_ne!(new_token, old_token);
    login(&mut app, &invitee).await;
    assert_eq!(
        accept(&app, &old_token).await.status(),
        StatusCode::NOT_FOUND
    );

    // 取り消した招待のリンクは使えない
    login(&mut app, &owner).await;
    let deleted = app
        .delete_with_session(&format!(
            "{}/{invitation_id}",
            invitations_path(tp.tenant_id)
        ))
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    login(&mut app, &invitee).await;
    assert_eq!(
        accept(&app, &new_token).await.status(),
        StatusCode::NOT_FOUND
    );
}

/// SMTP がメールを受けた後に応答だけ失敗すると、apalis は同じジョブをやり直す。
/// やり直しで送るリンクは同じなので、先に届いたメールのリンクも使える。
#[tokio::test]
async fn retrying_the_job_sends_the_same_link() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let email = unique_email();

    login(&mut app, &owner).await;
    let invitation_id = invite_ok(&app, tp.tenant_id, &email, "Member").await;
    let delivered = send_and_take_token(&app, invitation_id, &email).await;
    let retried = send_and_take_token(&app, invitation_id, &email).await;
    assert_eq!(delivered, retried, "同じ世代の再試行は同じリンクを送る");

    let invitee = register_as(&app, &email).await;
    login(&mut app, &invitee).await;
    assert_eq!(
        accept(&app, &delivered).await.status(),
        StatusCode::CREATED,
        "先に届いたメールのリンクで承諾できる"
    );
}

#[tokio::test]
async fn create_validates_and_reissues_duplicate_invitations() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let member = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let email = unique_email();

    login(&mut app, &owner).await;
    assert_eq!(
        invite(&app, tp.tenant_id, "not-an-email", "Member")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );

    // 既にメンバーの人（オーナーを含む）は招待しない
    let added = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/members", tp.tenant_id),
            json!({ "user_id": member.id, "role": "Member" }),
        )
        .await;
    assert_eq!(added.status(), StatusCode::CREATED);
    for existing in [&member.email, &owner.email] {
        let res = invite(&app, tp.tenant_id, &existing.to_uppercase(), "Member").await;
        assert_eq!(res.status(), StatusCode::CONFLICT, "{existing}");
        assert_eq!(
            res.json::<Value>().await.expect("error json")["message"],
            "already-member"
        );
    }

    // 二重招待: 続けて送ると 429、間隔を空ければ同じ招待をロールごと作り直す
    let first = invite_ok(&app, tp.tenant_id, &email, "Member").await;
    assert_eq!(
        invite(&app, tp.tenant_id, &email, "Admin").await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    clear_send_slot(&app, tp.tenant_id, &email).await;
    let second = invite(&app, tp.tenant_id, &email, "Admin").await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second: Value = second.json().await.expect("invitation json");
    assert_eq!(
        second["id"],
        first.to_string(),
        "同じ宛先の招待は 1 件にまとまる"
    );
    assert_eq!(second["role"], "Admin");

    let listed: Value = app
        .get_with_session(&invitations_path(tp.tenant_id))
        .await
        .json()
        .await
        .expect("list json");
    assert_eq!(listed.as_array().expect("array").len(), 1);
}

#[tokio::test]
async fn invitations_are_admin_only_and_scoped_to_the_tenant() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let admin = app.insert_user(false, false).await;
    let member = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let other_tp = app.insert_tenant_project(owner.id).await;

    login(&mut app, &owner).await;
    for (user, role) in [(&admin, "Admin"), (&member, "Member")] {
        let res = app
            .post_json_with_session(
                &format!("/v1/tenants/{}/members", tp.tenant_id),
                json!({ "user_id": user.id, "role": role }),
            )
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    // 0 件
    let empty: Value = app
        .get_with_session(&invitations_path(tp.tenant_id))
        .await
        .json()
        .await
        .expect("list json");
    assert_eq!(empty, json!([]));

    let other_invitation = invite_ok(&app, other_tp.tenant_id, &unique_email(), "Member").await;

    // テナント Admin は発行できる（オーナーだけに絞っていない）
    login(&mut app, &admin).await;
    let a = invite_ok(&app, tp.tenant_id, &unique_email(), "Member").await;
    let b = invite_ok(&app, tp.tenant_id, &unique_email(), "Viewer").await;
    let listed: Value = app
        .get_with_session(&invitations_path(tp.tenant_id))
        .await
        .json()
        .await
        .expect("list json");
    let ids: Vec<&str> = listed
        .as_array()
        .expect("array")
        .iter()
        .map(|i| i["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids.len(), 2, "他テナントの招待は混ざらない");
    assert!(ids.contains(&a.to_string().as_str()));
    assert!(ids.contains(&b.to_string().as_str()));

    // 他テナントの招待は、id を知っていても取り消し・再送できない
    for res in [
        app.delete_with_session(&format!(
            "{}/{other_invitation}",
            invitations_path(tp.tenant_id)
        ))
        .await,
        app.post_json_with_session(
            &format!(
                "{}/{other_invitation}/resend",
                invitations_path(tp.tenant_id)
            ),
            json!({}),
        )
        .await,
        app.delete_with_session(&format!(
            "{}/{}",
            invitations_path(tp.tenant_id),
            Uuid::new_v4()
        ))
        .await,
    ] {
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    // Member は一覧も発行もできない
    login(&mut app, &member).await;
    assert_eq!(
        app.get_with_session(&invitations_path(tp.tenant_id))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        invite(&app, tp.tenant_id, &unique_email(), "Member")
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.delete_with_session(&format!("{}/{a}", invitations_path(tp.tenant_id)))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}
