use crate::common::{
    TestApp, TestTenantProject, TestUser, execute_sql, insert_passkey,
    insert_personal_token_for_test, json_body,
};
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use entity::scopes::Scope;
use entity::{audit_logs, personal_tokens, tenant_members, users};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    prelude::Uuid,
};
use serde_json::{Value, json};

async fn fixture() -> (TestApp, TestUser, TestTenantProject, Uuid) {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let tenant = app.insert_tenant_project(owner.id).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let res = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/bots", tenant.tenant_id),
            json!({"username": "delivery-bot", "role": "Member"}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let created = json_body(res).await;
    assert_eq!(created["account_kind"], "bot");
    let id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    (app, owner, tenant, id)
}

async fn issue(
    app: &TestApp,
    tenant: Uuid,
    bot: Uuid,
    scopes: Vec<&str>,
    project_ids: Option<Vec<Uuid>>,
) -> String {
    let res = app.post_json_with_session(&format!("/v1/tenants/{tenant}/bots/{bot}/tokens"), json!({"name": "pipeline", "scopes": scopes, "project_ids": project_ids, "expires_at": Utc::now() + Duration::days(30)})).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    json_body(res).await["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn tenant_bots_identity_bounds_admin_only_and_self_minting() {
    let (mut app, owner, tenant, bot) = fixture().await;
    // BOT は口からは Admin になれないが、鍵を増やせぬ守りは役に依らぬことを確かめるため、
    // DB で直に Admin にしてから撃つ（Member のままでは発行の口の役の検めで先に止まる）
    let mut bot_member: tenant_members::ActiveModel = tenant_members::Entity::find()
        .filter(tenant_members::Column::TenantId.eq(tenant.tenant_id))
        .filter(tenant_members::Column::UserId.eq(bot))
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    bot_member.role = Set(entity::tenant_members::TenantRole::Admin);
    bot_member.update(&app.state.db).await.unwrap();
    let token = issue(
        &app,
        tenant.tenant_id,
        bot,
        vec!["api"],
        Some(vec![tenant.project_id]),
    )
    .await;
    let identity = app.get_with_bearer("/v1/personal_tokens/me", &token).await;
    assert_eq!(identity.status(), StatusCode::OK);
    let identity = json_body(identity).await;
    assert_eq!(identity["account_kind"], "bot");
    assert_eq!(identity["user_id"], bot.to_string());
    assert_eq!(identity["tenant_id"], tenant.tenant_id.to_string());
    assert_eq!(identity["allowed_project_ids"], json!([tenant.project_id]));
    // BOT Admin でも自分の PAT から鍵を増やせない。
    let own = app
        .post_json_with_bearer(
            "/v1/personal_tokens",
            json!({"name":"multiply", "tenant_id":tenant.tenant_id,"scopes":["api"]}),
            &token,
        )
        .await;
    assert_eq!(own.status(), StatusCode::FORBIDDEN);
    let nested = app
        .post_json_with_bearer(
            &format!("/v1/tenants/{}/bots/{bot}/tokens", tenant.tenant_id),
            json!({"name":"multiply","scopes":["api"],"expires_at":Utc::now()+Duration::days(1)}),
            &token,
        )
        .await;
    assert_eq!(nested.status(), StatusCode::FORBIDDEN);
    let foreign = app.insert_tenant_project(owner.id).await;
    let outside = app
        .get_with_bearer(
            &format!("/v1/tenants/{}/members", foreign.tenant_id),
            &token,
        )
        .await;
    assert_eq!(outside.status(), StatusCode::FORBIDDEN);
    let member = app.insert_user_default().await;
    for role in [
        entity::tenant_members::TenantRole::Member,
        entity::tenant_members::TenantRole::Viewer,
    ] {
        let existing = tenant_members::Entity::find()
            .filter(tenant_members::Column::TenantId.eq(tenant.tenant_id))
            .filter(tenant_members::Column::UserId.eq(member.id))
            .one(&app.state.db)
            .await
            .unwrap();
        if let Some(existing) = existing {
            let mut active: tenant_members::ActiveModel = existing.into();
            active.role = Set(role);
            active.update(&app.state.db).await.unwrap();
        } else {
            tenant_members::ActiveModel {
                id: Set(Uuid::new_v4()),
                tenant_id: Set(tenant.tenant_id),
                user_id: Set(member.id),
                role: Set(role),
                department: Set(None),
                position: Set(None),
                team: Set(None),
            }
            .insert(&app.state.db)
            .await
            .unwrap();
        }
        app.reset_session_client();
        app.login_session_no_content(&member.email, &member.password)
            .await;
        assert_eq!(
            app.get_with_session(&format!("/v1/tenants/{}/bots", tenant.tenant_id))
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    assert_eq!(
        app.get_with_session(&format!("/v1/tenants/{}/bots", tenant.tenant_id))
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn tenant_bots_token_expiry_projects_suspension_and_audits() {
    let (app, owner, tenant, bot) = fixture().await;
    let path = format!("/v1/tenants/{}/bots/{bot}/tokens", tenant.tenant_id);
    for expires in [
        Value::Null,
        json!(Utc::now() - Duration::seconds(1)),
        json!(Utc::now() + Duration::days(366)),
    ] {
        let res = app
            .post_json_with_session(
                &path,
                json!({"name":"invalid","scopes":["api"],"expires_at":expires}),
            )
            .await;
        assert!(res.status().is_client_error());
    }
    let foreign = app.insert_tenant_project(owner.id).await;
    let denied=app.post_json_with_session(&path,json!({"name":"foreign","scopes":["api"],"project_ids":[foreign.project_id],"expires_at":Utc::now()+Duration::days(1)})).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let token = issue(
        &app,
        tenant.tenant_id,
        bot,
        vec!["read:task"],
        Some(vec![tenant.project_id]),
    )
    .await;
    assert_eq!(
        app.get_with_bearer("/v1/personal_tokens/me", &token)
            .await
            .status(),
        StatusCode::OK
    );
    let suspended = app
        .patch_json_with_session(
            &format!("/v1/tenants/{}/bots/{bot}", tenant.tenant_id),
            json!({"is_suspended":true}),
        )
        .await;
    assert_eq!(suspended.status(), StatusCode::NO_CONTENT);
    let rejected = app.get_with_bearer("/v1/personal_tokens/me", &token).await;
    assert!(rejected.status().is_client_error());
    let audit = audit_logs::Entity::find()
        .filter(audit_logs::Column::ResourceId.eq(bot.to_string()))
        .all(&app.state.db)
        .await
        .unwrap();
    assert_eq!(audit.len(), 3);
    assert!(
        audit
            .iter()
            .all(|log| log.actor_id == Some(owner.id) && log.tenant_id == Some(tenant.tenant_id))
    );
    assert!(audit.iter().any(|log| log.action == "bot.suspended"));
    assert_eq!(
        personal_tokens::Entity::find()
            .filter(personal_tokens::Column::UserId.eq(bot))
            .filter(personal_tokens::Column::Revoked.eq(false))
            .count(&app.state.db)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn tenant_bots_cannot_join_or_be_invited_to_foreign_tenant() {
    let (app, owner, tenant, bot) = fixture().await;
    let foreign = app.insert_tenant_project(owner.id).await;
    let email = format!("{bot}@bots.task.invalid");
    let member = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/members", foreign.tenant_id),
            json!({"user_id":bot,"role":"Member"}),
        )
        .await;
    assert_eq!(member.status(), StatusCode::FORBIDDEN);
    let project = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/members",
                foreign.tenant_id, foreign.project_id
            ),
            json!({"user_id":bot,"role":"Member"}),
        )
        .await;
    assert_eq!(project.status(), StatusCode::FORBIDDEN);
    let invitation = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/invitations", foreign.tenant_id),
            json!({"email":email,"role":"Member"}),
        )
        .await;
    assert_eq!(invitation.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        app.delete_with_session(&format!("/v1/tenants/{}/members/{bot}", tenant.tenant_id))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let human = app.insert_user_default().await;
    assert_eq!(
        app.post_json_with_session(
            &format!("/v1/tenants/{}/members", foreign.tenant_id),
            json!({"user_id":human.id,"role":"Member"})
        )
        .await
        .status(),
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn tenant_bots_reset_verification_and_final_mail_suppression() {
    let (app, _, tenant, bot) = fixture().await;
    let verification = format!("bot-verify-{}", Uuid::new_v4());
    service::email_verification::store_token(
        &app.state.redis_client,
        bot,
        &verification,
        Utc::now().timestamp_millis() as u64,
    )
    .await
    .unwrap();
    let verify = app
        .post_json("/v1/auth/verify-email", json!({"token":verification}))
        .await;
    assert_eq!(verify.status(), StatusCode::FORBIDDEN);
    let reset = format!("bot-reset-{}", Uuid::new_v4());
    service::password_reset::store_token(&app.state.redis_client, bot, &reset)
        .await
        .unwrap();
    let res = app
        .post_json("/v1/auth/password-reset/verify", json!({"token":reset}))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let res = app
        .post_json(
            "/v1/auth/password-reset/complete",
            json!({"token":reset,"new_password":"NewPassword123!"}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let user = users::Entity::find_by_id(bot)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    assert!(user.password_hash.is_none());
    let res = app
        .post_json(
            "/v1/auth/login",
            json!({"email":user.email,"password":"NewPassword123!"}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let (smtp, sent) = service::smtp::SmtpClient::capture();
    smtp.send_email(&user.email, "never deliver", "body", None)
        .await
        .unwrap();
    assert!(sent.lock().unwrap().is_empty());
    smtp.send_email("control@example.com", "human control", "body", None)
        .await
        .unwrap();
    assert_eq!(sent.lock().unwrap().len(), 1);
    assert_eq!(user.bot_tenant_id, Some(tenant.tenant_id));
}

#[tokio::test]
async fn tenant_bots_existing_sessions_and_interactive_entrypoints_rejected() {
    let (mut app, owner, tenant, bot) = fixture().await;
    assert_eq!(
        app.get_with_session("/v1/auth/me").await.status(),
        StatusCode::OK
    );
    // 正当な人のセッションを残したまま BOT に切り替え、入口が種別を検査することを確かめる。
    let user = users::Entity::find_by_id(owner.id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = user.into();
    active.account_kind = Set("bot".into());
    active.bot_tenant_id = Set(Some(tenant.tenant_id));
    active.password_hash = Set(None);
    active.email = Set(format!("{}@bots.task.invalid", owner.id));
    active.update(&app.state.db).await.unwrap();
    assert_eq!(
        app.get_with_session("/v1/auth/me").await.status(),
        StatusCode::FORBIDDEN
    );
    for (path, body) in [
        ("/v1/auth/passkeys/registration/start", json!({})),
        ("/v1/auth/2fa/totp/setup", json!({})),
        (
            "/v1/auth/password/change",
            json!({"current_password":owner.password,"new_password":"NewPassword123!"}),
        ),
        ("/v1/invitations/accept", json!({"token":"unused"})),
        (
            "/v1/desktop/auth/codes",
            json!({"name":"bot-device", "code_challenge":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        ),
    ] {
        let res = app.post_json_with_session(path, body).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "{path}");
    }
    let oauth = app
        .get_with_session("/v1/auth/oauth/github?link=true")
        .await;
    assert!(oauth.status().is_client_error());
    app.reset_session_client();
    let token = app
        .insert_pat(bot, tenant.tenant_id, vec![Scope::Api], None)
        .await;
    assert_eq!(
        app.post_json_with_bearer("/v1/auth/2fa/totp/setup", json!({}), &token)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn tenant_bots_oauth_existing_connection_cannot_issue_session() {
    let (mut app, _, tenant, bot) = fixture().await;
    let pat = app
        .insert_pat(bot, tenant.tenant_id, vec![Scope::Api], None)
        .await;
    let start = app
        .session_client()
        .get(format!("{}/v1/auth/oauth/gitlab", app.base_url()))
        .bearer_auth(&pat)
        .send()
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::UNAUTHORIZED);
    app.reset_session_client();
    app.set_mock_user(crate::common::MockGitLabUser {
        id: (Uuid::new_v4().as_u128() & i64::MAX as u128) as i64,
        username: format!("oauth_bot_{}", Uuid::new_v4()),
        email: Some(format!("oauth-bot-control-{}@example.com", Uuid::new_v4())),
    });
    let start = app.oauth_start(false).await;
    let callback = app.follow_oauth_start(start).await;
    assert!(crate::common::is_redirect(callback.status()));
    let me = app.get_me().await;
    assert_eq!(me.status(), StatusCode::OK);
    let user_id = Uuid::parse_str(json_body(me).await["id"].as_str().unwrap()).unwrap();
    let user = users::Entity::find_by_id(user_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = user.into();
    active.account_kind = Set("bot".into());
    active.bot_tenant_id = Set(Some(tenant.tenant_id));
    active.email = Set(format!("{user_id}@bots.task.invalid"));
    active.password_hash = Set(None);
    active.update(&app.state.db).await.unwrap();
    app.reset_session_client();
    let start = app.oauth_start(false).await;
    let callback = app.follow_oauth_start(start).await;
    assert_eq!(callback.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(app.get_me().await.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tenant_bots_are_assignable_mentionable_comment_authors_and_scoped() {
    let (app, owner, tenant, bot) = fixture().await;
    let base = format!(
        "/v1/tenants/{}/projects/{}",
        tenant.tenant_id, tenant.project_id
    );
    let status = app
        .post_json_with_session(
            &format!("{base}/statuses"),
            json!({"name":"Backlog","color":"#336699","position":0,"is_default":true}),
        )
        .await;
    assert_eq!(status.status(), StatusCode::CREATED);
    let status_id = json_body(status).await["id"].as_str().unwrap().to_owned();
    let task=app.post_json_with_session(&format!("{base}/tasks"),json!({"title":"BOT collaboration","status_id":status_id,"description":format!("please review <@{bot}>"),"assignees":[{"user_id":bot,"role":"assignee"}]})).await;
    assert_eq!(task.status(), StatusCode::CREATED);
    let task = json_body(task).await;
    assert_eq!(task["assignees"][0]["user"]["account_kind"], "bot");
    let task_id = task["id"].as_str().unwrap();
    let mentioned = service::mentions::resolve(&app.state.db, tenant.project_id, &[bot])
        .await
        .unwrap();
    assert_eq!(mentioned[&bot].name.as_deref(), Some("delivery-bot"));
    let token = issue(
        &app,
        tenant.tenant_id,
        bot,
        vec!["read:task", "write:task"],
        Some(vec![tenant.project_id]),
    )
    .await;
    let comments_path = format!("{base}/tasks/{task_id}/comments");
    let comment = app
        .post_json_with_bearer(
            &comments_path,
            json!({"body":format!("ready <@{}>",owner.id)}),
            &token,
        )
        .await;
    assert_eq!(comment.status(), StatusCode::CREATED);
    let comments = json_body(app.get_with_bearer(&comments_path, &token).await).await;
    assert!(comments.to_string().contains("\"account_kind\":\"bot\""));
    let activity = json_body(
        app.get_with_bearer(&format!("{base}/tasks/{task_id}/activities"), &token)
            .await,
    )
    .await;
    assert!(activity.to_string().contains("\"account_kind\":\"bot\""));
    let read_only = issue(
        &app,
        tenant.tenant_id,
        bot,
        vec!["read:task"],
        Some(vec![tenant.project_id]),
    )
    .await;
    assert_eq!(
        app.get_with_bearer(&comments_path, &read_only)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.post_json_with_bearer(&comments_path, json!({"body":"denied"}), &read_only)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let second = app.post_json_with_session(
        &format!("/v1/tenants/{}/projects", tenant.tenant_id),
        json!({"name":"Outside token project", "key":format!("B{}", &Uuid::new_v4().simple().to_string()[..8].to_uppercase())}),
    ).await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_id = json_body(second).await["id"].as_str().unwrap().to_owned();
    let outside_project = format!(
        "/v1/tenants/{}/projects/{second_id}/tasks",
        tenant.tenant_id
    );
    assert_eq!(
        app.get_with_session(&outside_project).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        app.get_with_bearer(&outside_project, &token).await.status(),
        StatusCode::FORBIDDEN
    );
    let other = app.insert_tenant_project(owner.id).await;
    assert_eq!(
        app.get_with_bearer(
            &format!(
                "/v1/tenants/{}/projects/{}/tasks",
                other.tenant_id, other.project_id
            ),
            &token
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let revoked = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(bot))
        .filter(personal_tokens::Column::Name.eq("pipeline"))
        .all(&app.state.db)
        .await
        .unwrap();
    for token in revoked {
        assert_eq!(
            app.delete_with_session(&format!(
                "/v1/tenants/{}/bots/{bot}/tokens/{}",
                tenant.tenant_id, token.id
            ))
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
    }
    assert_eq!(
        app.get_with_bearer("/v1/personal_tokens/me", &read_only)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(
        audit_logs::Entity::find()
            .filter(audit_logs::Column::ResourceId.eq(bot.to_string()))
            .filter(audit_logs::Column::Action.eq("bot.token_revoked"))
            .count(&app.state.db)
            .await
            .unwrap()
            > 0
    );
}

#[tokio::test]
async fn tenant_bots_all_mail_templates_are_suppressed_at_delivery() {
    let (app, _, _, bot) = fixture().await;
    let email = format!("{bot}@bots.task.invalid");
    let (smtp, sent) = service::smtp::SmtpClient::capture();
    for address in [&email, "human-control@example.com"] {
        service::verification_email_delivery::send_verification_email(
            &smtp,
            address,
            &app.state.settings,
            "verification",
        )
        .await
        .unwrap();
        service::password_reset_delivery::send_password_reset_email(
            &smtp,
            address,
            &app.state.settings,
            "reset",
        )
        .await
        .unwrap();
        service::password_reset_email_delivery::send_password_reset_email(
            &smtp,
            address,
            &app.state.settings,
            "reset",
        )
        .await
        .unwrap();
        service::already_registered_email_delivery::send_already_registered_email(
            &smtp,
            address,
            &app.state.settings,
        )
        .await
        .unwrap();
        service::tenant_invitations::send_invitation_email(
            &smtp,
            &app.state.settings,
            service::tenant_invitations::InvitationMail {
                to: address,
                tenant_name: "Tenant",
                inviter: "Admin",
                role: &entity::tenant_members::TenantRole::Member,
                token: "invitation",
            },
        )
        .await
        .unwrap();
    }
    let sent = sent.lock().unwrap();
    assert_eq!(sent.len(), 5);
    assert!(
        sent.iter()
            .all(|mail| mail.to == "human-control@example.com")
    );
}

#[tokio::test]
async fn tenant_bots_passkey_with_valid_signature_cannot_issue_session() {
    use super::webauthn_integration::{soft_register_finish, webauthn_origin};
    use webauthn_authenticator_rs::{prelude::WebauthnAuthenticator, softtoken::SoftToken};
    use webauthn_rs::prelude::RequestChallengeResponse;
    let (mut app, owner, tenant, _) = fixture().await;
    let (soft, _) = SoftToken::new(true).unwrap();
    let mut authenticator = WebauthnAuthenticator::new(soft);
    soft_register_finish(&app, &mut authenticator, "bot-boundary").await;
    for bot_case in [false, true] {
        app.reset_session_client();
        let start = app
            .post_json(
                "/v1/auth/passkeys/authentication/start",
                json!({"email":owner.email}),
            )
            .await;
        assert_eq!(start.status(), StatusCode::OK);
        let mut challenge = json_body(start).await;
        let challenge_id = challenge
            .as_object_mut()
            .unwrap()
            .remove("challenge_id")
            .unwrap();
        let options: RequestChallengeResponse = serde_json::from_value(challenge).unwrap();
        let credential = authenticator
            .do_authentication(
                webauthn_origin(&app.state.settings.email_verification_app_url),
                options,
            )
            .unwrap();
        if bot_case {
            let user = users::Entity::find_by_id(owner.id)
                .one(&app.state.db)
                .await
                .unwrap()
                .unwrap();
            let mut active: users::ActiveModel = user.into();
            active.account_kind = Set("bot".into());
            active.bot_tenant_id = Set(Some(tenant.tenant_id));
            active.email = Set(format!("{}@bots.task.invalid", owner.id));
            active.password_hash = Set(None);
            active.update(&app.state.db).await.unwrap();
        }
        let finish = app
            .post_json(
                "/v1/auth/passkeys/authentication/finish",
                json!({"challenge_id":challenge_id,"credential":credential}),
            )
            .await;
        assert_eq!(
            finish.status(),
            if bot_case {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::NO_CONTENT
            }
        );
    }
}

#[tokio::test]
async fn tenant_bots_cannot_exchange_valid_desktop_authorization_code() {
    let (app, owner, tenant, bot) = fixture().await;
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    for user_id in [owner.id, bot] {
        let code = service::desktop_auth::issue_code(
            &app.state.redis_client,
            &service::desktop_auth::PendingCode {
                user_id,
                code_challenge: service::desktop_auth::s256_challenge(verifier),
                name: "desktop".into(),
                issued_at_ms: Utc::now().timestamp_millis(),
            },
        )
        .await
        .unwrap();
        let exchange = app
            .post_json(
                "/v1/desktop/auth/token",
                json!({"code":code,"code_verifier":verifier}),
            )
            .await;
        assert_eq!(
            exchange.status(),
            if user_id == bot {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::CREATED
            }
        );
    }
    assert_eq!(
        users::Entity::find_by_id(bot)
            .one(&app.state.db)
            .await
            .unwrap()
            .unwrap()
            .bot_tenant_id,
        Some(tenant.tenant_id)
    );
}

#[tokio::test]
async fn tenant_bots_notification_outbox_is_abandoned_without_smtp() {
    let (app, _, tenant, bot) = fixture().await;
    let user = users::Entity::find_by_id(bot)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    let mut active: users::ActiveModel = user.into();
    active.email_verified = Set(true);
    active.update(&app.state.db).await.unwrap();
    let id = Uuid::new_v4();
    entity::notifications::ActiveModel {
        id: Set(id),
        user_id: Set(bot),
        task_id: Set(None),
        project_id: Set(Some(tenant.project_id)),
        notification_type: Set("mentioned".into()),
        payload: Set(json!({})),
        read_at: Set(None),
        created_at: Set(Utc::now().into()),
        email_queued_at: Set(Some(Utc::now().into())),
        emailed_at: Set(None),
        email_attempts: Set(0),
    }
    .insert(&app.state.db)
    .await
    .unwrap();
    let state = job::JobState {
        settings: app.state.settings.clone(),
        db: app.state.db.clone(),
        redis_client: app.state.redis_client.clone(),
        smtp_client: app.state.smtp_client.clone(),
        http_client: app.state.http_client.clone(),
        review_summary_storage: app.state.review_summary_storage.clone(),
        github_issue_sync_storage: app.state.github_issue_sync_storage.clone(),
        pg_pool: app.state.pg_pool.clone(),
        oauth_settings: app.state.oauth_settings.clone(),
    };
    job::notification_email::send_pending_once(&state)
        .await
        .unwrap();
    let row = entity::notifications::Entity::find_by_id(id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    assert!(row.email_queued_at.is_none());
    assert!(row.emailed_at.is_none());
    assert!(
        app.sent_mails()
            .iter()
            .all(|mail| !users::is_bot_email(&mail.to))
    );
}

#[tokio::test]
async fn tenant_bots_admin_user_credential_operations_are_rejected_even_with_mail_override() {
    let (mut app, owner, _, bot) = fixture().await;
    let admin = app.insert_user(true, false).await;
    app.reset_session_client();
    app.login_session_no_content(&admin.email, &admin.password)
        .await;
    let before = app.sent_mails().len();
    let denied = app
        .post_json_with_session(
            &format!("/v1/admin/users/{bot}/password-reset"),
            json!({"send_to":"human-control@example.com"}),
        )
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(app.sent_mails().len(), before);
    assert_eq!(
        app.patch_json_with_session(&format!("/v1/admin/users/{bot}"), json!({"is_admin":true}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.delete_with_session(&format!("/v1/admin/users/{bot}"))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.post_json_with_session(
            &format!("/v1/admin/users/{}/password-reset", owner.id),
            json!({"send_to":"human-control@example.com"})
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(app.sent_mails().len(), before + 1);
    app.cleanup_user(admin.id).await;
}

#[tokio::test]
async fn tenant_bots_owner_bulk_revocation_also_records_token_audit() {
    let (app, owner, tenant, bot) = fixture().await;
    let token = issue(&app, tenant.tenant_id, bot, vec!["read:task"], None).await;
    assert_eq!(
        app.delete_json_with_session(
            "/v1/personal_tokens/revoke-all",
            json!({"confirm_tenant_id":tenant.tenant_id})
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.get_with_bearer("/v1/personal_tokens/me", &token)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let audit = audit_logs::Entity::find()
        .filter(audit_logs::Column::ResourceId.eq(bot.to_string()))
        .filter(audit_logs::Column::Action.eq("bot.token_revoked"))
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(audit.actor_id, Some(owner.id));
    assert!(audit.metadata.unwrap().get("token_id").is_some());
}

async fn require_2fa(app: &TestApp, tenant_id: Uuid) {
    execute_sql(
        &app.state.db,
        "UPDATE tenants SET require_2fa = true WHERE id = $1",
        vec![tenant_id.into()],
    )
    .await;
}

async fn assert_2fa_required(res: reqwest::Response, what: &str) {
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "{what}");
    assert_eq!(
        json_body(res).await["message"],
        "2fa-setup-required",
        "{what}"
    );
}

/// BOT は TOTP もパスキーも持てないので、2FA を強制するテナントでも BOT の PAT は業務の口を通る。
/// 免除は BOT に限り、2FA を満たさない人の PAT は従来どおり止まる。
#[tokio::test]
async fn tenant_bots_pat_is_exempt_from_tenant_2fa_but_human_pat_is_not() {
    let (app, owner, tenant, bot) = fixture().await;
    let bot_token = issue(&app, tenant.tenant_id, bot, vec!["read:project"], None).await;
    let human_token = insert_personal_token_for_test(
        &app.state.db,
        owner.id,
        tenant.tenant_id,
        &app.state.settings.personal_token_secret,
        vec![Scope::ReadProject],
        None,
    )
    .await;
    require_2fa(&app, tenant.tenant_id).await;
    let projects = format!("/v1/tenants/{}/projects", tenant.tenant_id);
    assert_eq!(
        app.get_with_bearer(&projects, &bot_token).await.status(),
        StatusCode::OK
    );
    assert_2fa_required(
        app.get_with_bearer(&projects, &human_token).await,
        "2FA の無い人の PAT",
    )
    .await;
}

/// 2FA を強制するテナントでは、2FA を満たさない Admin は BOT 管理の全ての口で 403。
/// 2FA を満たせば通る。
#[tokio::test]
async fn tenant_bots_management_requires_tenant_2fa_of_admin() {
    let (app, owner, tenant, bot) = fixture().await;
    issue(&app, tenant.tenant_id, bot, vec!["read:task"], None).await;
    let token_id = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(bot))
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap()
        .id;
    require_2fa(&app, tenant.tenant_id).await;
    let bots = format!("/v1/tenants/{}/bots", tenant.tenant_id);
    let one = format!("{bots}/{bot}");
    let tokens = format!("{one}/tokens");
    let token = format!("{tokens}/{token_id}");
    let token_body = json!({"name": "ci", "scopes": ["read:task"], "project_ids": null, "expires_at": Utc::now() + Duration::days(30)});

    assert_2fa_required(app.get_with_session(&bots).await, "一覧").await;
    assert_2fa_required(
        app.post_json_with_session(&bots, json!({"username": "late-bot", "role": "Member"}))
            .await,
        "作成",
    )
    .await;
    assert_2fa_required(
        app.patch_json_with_session(&one, json!({"is_suspended": true}))
            .await,
        "停止",
    )
    .await;
    assert_2fa_required(app.get_with_session(&tokens).await, "PAT の一覧").await;
    assert_2fa_required(
        app.post_json_with_session(&tokens, token_body.clone())
            .await,
        "PAT の発行",
    )
    .await;
    assert_2fa_required(app.delete_with_session(&token).await, "PAT の取り消し").await;

    insert_passkey(&app.state.db, owner.id).await;
    assert_eq!(app.get_with_session(&bots).await.status(), StatusCode::OK);
    assert_eq!(
        app.post_json_with_session(&bots, json!({"username": "late-bot", "role": "Member"}))
            .await
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(app.get_with_session(&tokens).await.status(), StatusCode::OK);
    assert_eq!(
        app.post_json_with_session(&tokens, token_body)
            .await
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        app.delete_with_session(&token).await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.patch_json_with_session(&one, json!({"is_suspended": true}))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
}

/// 期限の上限は 365 日に 5 分の猶予を足した所まで。境い目の 1 秒は service の単体試験で縛り、
/// ここでは口から猶予が効くことを確かめる。
#[tokio::test]
async fn tenant_bots_token_expiry_tolerates_client_clock_ahead_by_minutes() {
    let (app, _owner, tenant, bot) = fixture().await;
    let tokens = format!("/v1/tenants/{}/bots/{bot}/tokens", tenant.tenant_id);
    let body = |expires_at: chrono::DateTime<Utc>| json!({"name": "ci", "scopes": ["read:task"], "project_ids": null, "expires_at": expires_at});
    let year = Utc::now() + Duration::days(365);
    assert_eq!(
        app.post_json_with_session(&tokens, body(year + Duration::minutes(4)))
            .await
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        app.post_json_with_session(&tokens, body(year + Duration::minutes(6)))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
}

async fn assert_role_too_strong(res: reqwest::Response, what: &str) {
    assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{what}");
    assert_eq!(
        json_body(res).await["message"],
        "bot-role-too-strong",
        "{what}"
    );
}

/// BOT の役は Member 相当まで。作成、テナントとプロジェクトのメンバーの追加と役の変更で
/// Admin を渡すと 400。Member と Viewer は通り、人の役の扱いは変わらない。
#[tokio::test]
async fn tenant_bots_role_is_capped_at_member() {
    let (app, _owner, tenant, bot) = fixture().await;
    let bots = format!("/v1/tenants/{}/bots", tenant.tenant_id);
    let members = format!("/v1/tenants/{}/members", tenant.tenant_id);
    let project_members = format!(
        "/v1/tenants/{}/projects/{}/members",
        tenant.tenant_id, tenant.project_id
    );

    assert_role_too_strong(
        app.post_json_with_session(&bots, json!({"username": "admin-bot", "role": "Admin"}))
            .await,
        "Admin の BOT を作る",
    )
    .await;
    assert_eq!(
        app.post_json_with_session(&bots, json!({"username": "viewer-bot", "role": "Viewer"}))
            .await
            .status(),
        StatusCode::CREATED
    );

    assert_role_too_strong(
        app.put_json_with_session(&format!("{members}/{bot}"), json!({"role": "Admin"}))
            .await,
        "BOT をテナントの Admin へ上げる",
    )
    .await;
    assert_role_too_strong(
        app.post_json_with_session(&members, json!({"user_id": bot, "role": "Admin"}))
            .await,
        "BOT を Admin でテナントへ加える",
    )
    .await;
    assert_eq!(
        app.put_json_with_session(&format!("{members}/{bot}"), json!({"role": "Viewer"}))
            .await
            .status(),
        StatusCode::OK
    );

    assert_role_too_strong(
        app.post_json_with_session(&project_members, json!({"user_id": bot, "role": "Admin"}))
            .await,
        "BOT を Admin でプロジェクトへ加える",
    )
    .await;
    assert_eq!(
        app.post_json_with_session(&project_members, json!({"user_id": bot, "role": "Member"}))
            .await
            .status(),
        StatusCode::CREATED
    );
    assert_role_too_strong(
        app.put_json_with_session(
            &format!("{project_members}/{bot}"),
            json!({"role": "Admin"}),
        )
        .await,
        "BOT をプロジェクトの Admin へ上げる",
    )
    .await;

    // 人の役の扱いは変わらない
    let human = app.insert_user_default().await;
    assert_eq!(
        app.post_json_with_session(&members, json!({"user_id": human.id, "role": "Member"}))
            .await
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        app.put_json_with_session(&format!("{members}/{}", human.id), json!({"role": "Admin"}))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.post_json_with_session(
            &project_members,
            json!({"user_id": human.id, "role": "Admin"})
        )
        .await
        .status(),
        StatusCode::CREATED
    );
}

/// BOT は Admin になれないので、広い scope の PAT でも人の役を変えられない。
#[tokio::test]
async fn tenant_bots_member_bot_cannot_change_human_roles() {
    let (app, _owner, tenant, bot) = fixture().await;
    let human = app.insert_user_default().await;
    let members = format!("/v1/tenants/{}/members", tenant.tenant_id);
    assert_eq!(
        app.post_json_with_session(&members, json!({"user_id": human.id, "role": "Member"}))
            .await
            .status(),
        StatusCode::CREATED
    );
    let token = issue(&app, tenant.tenant_id, bot, vec!["api"], None).await;
    assert_eq!(
        app.put_json_with_bearer(
            &format!("{members}/{}", human.id),
            json!({"role": "Admin"}),
            &token
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.post_json_with_bearer(
            &members,
            json!({"user_id": app.insert_user_default().await.id, "role": "Admin"}),
            &token
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

/// 担当者の候補の応答は、BOT に account_kind: bot を載せる（画面の札はこれから出る）。
#[tokio::test]
async fn tenant_bots_assignable_users_carry_account_kind() {
    let (app, owner, tenant, bot) = fixture().await;
    let res = app
        .get_with_session(&format!(
            "/v1/tenants/{}/projects/{}/assignable-users",
            tenant.tenant_id, tenant.project_id
        ))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let users = json_body(res).await;
    let kind_of = |id: Uuid| {
        users
            .as_array()
            .unwrap()
            .iter()
            .find(|user| user["id"] == id.to_string())
            .map(|user| user["account_kind"].clone())
    };
    assert_eq!(kind_of(bot), Some(json!("bot")));
    assert_eq!(kind_of(owner.id), Some(json!("human")));
}

/// BOT は個人用プロジェクトを持たない。口は 404 を返し、行を作らない。人は今どおり作れる。
#[tokio::test]
async fn tenant_bots_cannot_create_personal_project() {
    let (app, owner, tenant, bot) = fixture().await;
    let personal = format!("/v1/tenants/{}/users/me/personal-project", tenant.tenant_id);
    let personal_of = |user_id: Uuid| {
        entity::projects::Entity::find()
            .filter(entity::projects::Column::TenantId.eq(tenant.tenant_id))
            .filter(entity::projects::Column::IsPersonal.eq(true))
            .filter(entity::projects::Column::PersonalOwnerId.eq(user_id))
            .count(&app.state.db)
    };

    let token = issue(&app, tenant.tenant_id, bot, vec!["read:project"], None).await;
    assert_eq!(
        app.get_with_bearer(&personal, &token).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        personal_of(bot).await.unwrap(),
        0,
        "BOT の個人用プロジェクトは作られない"
    );

    let human_token = insert_personal_token_for_test(
        &app.state.db,
        owner.id,
        tenant.tenant_id,
        &app.state.settings.personal_token_secret,
        vec![Scope::ReadProject],
        None,
    )
    .await;
    assert_eq!(
        app.get_with_bearer(&personal, &human_token).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        personal_of(owner.id).await.unwrap(),
        1,
        "人は今どおり作れる"
    );
}

/// BOT の PAT は持ち主のテナントでだけ通る。API では作れない形だが、別のテナントに縛った
/// BOT の PAT が在っても認証で断る（所属を見ないエンドポイントでも通さない）。
#[tokio::test]
async fn tenant_bots_pat_bound_to_foreign_tenant_is_rejected_at_authentication() {
    let (app, owner, tenant, bot) = fixture().await;
    let foreign = app.insert_tenant_project(owner.id).await;
    let secret = app.state.settings.personal_token_secret.clone();
    let own = insert_personal_token_for_test(
        &app.state.db,
        bot,
        tenant.tenant_id,
        &secret,
        vec![Scope::ReadProject],
        None,
    )
    .await;
    assert_eq!(
        app.get_with_bearer("/v1/personal_tokens/me", &own)
            .await
            .status(),
        StatusCode::OK
    );
    let foreign_token = insert_personal_token_for_test(
        &app.state.db,
        bot,
        foreign.tenant_id,
        &secret,
        vec![Scope::ReadProject],
        None,
    )
    .await;
    assert_eq!(
        app.get_with_bearer("/v1/personal_tokens/me", &foreign_token)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}
