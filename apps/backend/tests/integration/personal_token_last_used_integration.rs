// PAT の last_used_at が、認証に成功した使用で更新されることを確かめる（TASK-492）。
// - 使うと記録され、一覧に出る / 別の PAT は動かない
// - 認証後の認可 403 は記録し、認証で拒否された要求（失効・期限切れ・凍結）は記録しない
// - 5 分未満の再使用では書き直さず、5 分以上空くと書き直す
// - BOT のトークンでも同じ

use crate::common::{TestApp, insert_tenant, insert_user, json_body};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use entity::personal_tokens;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;
use uuid::Uuid;

async fn tokens_for_user(app: &TestApp, user_id: Uuid) -> Vec<personal_tokens::Model> {
    personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(user_id))
        .all(&app.state.db)
        .await
        .expect("query tokens")
}

async fn only_token(app: &TestApp, user_id: Uuid) -> personal_tokens::Model {
    let mut tokens = tokens_for_user(app, user_id).await;
    assert_eq!(tokens.len(), 1, "トークンは 1 本だけ");
    tokens.remove(0)
}

async fn set_last_used(app: &TestApp, token: personal_tokens::Model, at: DateTime<Utc>) {
    let mut active: personal_tokens::ActiveModel = token.into();
    active.last_used_at = Set(Some(at.fixed_offset()));
    active
        .update(&app.state.db)
        .await
        .expect("set last_used_at");
}

#[tokio::test]
async fn using_a_token_records_last_used_at() {
    let app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let token = app.insert_pat(user.id, tenant_id, vec![], None).await;
    assert!(only_token(&app, user.id).await.last_used_at.is_none());

    let before = Utc::now();
    let response = app.get_with_bearer("/v1/personal_tokens/me", &token).await;
    let after = Utc::now();

    assert_eq!(response.status(), StatusCode::OK);
    let used = only_token(&app, user.id)
        .await
        .last_used_at
        .expect("使うと last_used_at が入る")
        .with_timezone(&Utc);
    assert!(before - Duration::seconds(1) <= used && used <= after + Duration::seconds(1));

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn used_token_shows_last_used_at_in_the_list_while_unused_one_does_not() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let token_a = app.insert_pat(user.id, tenant_id, vec![], None).await;
    let _token_b = app.insert_pat(user.id, tenant_id, vec![], None).await;
    let tokens = tokens_for_user(&app, user.id).await;
    let used_id = tokens
        .iter()
        .find(|t| t.token_last_four == token_a[token_a.len() - 4..])
        .map(|t| t.id)
        .expect("発行した PAT の行");

    // セッションだけの一覧は、PAT を使っていないので全件 null
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let list = json_body(app.get_with_session("/v1/personal_tokens").await).await;
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .all(|t| t["last_used_at"].is_null())
    );

    assert_eq!(
        app.get_with_bearer("/v1/personal_tokens/me", &token_a)
            .await
            .status(),
        StatusCode::OK
    );

    // 別の PAT は動かない（A は Some、B は None）
    let rows = tokens_for_user(&app, user.id).await;
    assert_eq!(rows.len(), 2);
    for row in &rows {
        if row.id == used_id {
            assert!(row.last_used_at.is_some(), "使った PAT は記録される");
        } else {
            assert!(row.last_used_at.is_none(), "使っていない PAT は変わらない");
        }
    }
    let list = json_body(app.get_with_session("/v1/personal_tokens").await).await;
    for item in list.as_array().unwrap() {
        let id: Uuid = item["id"].as_str().unwrap().parse().unwrap();
        assert_eq!(!item["last_used_at"].is_null(), id == used_id);
    }

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn forbidden_by_scope_after_authentication_still_counts_as_use() {
    let app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let token = app.insert_pat(user.id, tenant_id, vec![], None).await;

    let response = app
        .get_with_bearer(&format!("/v1/tenants/{tenant_id}/projects"), &token)
        .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        only_token(&app, user.id).await.last_used_at.is_some(),
        "認証に成功した後の認可 403 は使用として記録する"
    );

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn reuse_within_five_minutes_does_not_rewrite_but_after_five_minutes_does() {
    let app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let token = app.insert_pat(user.id, tenant_id, vec![], None).await;

    // 1 分前: 書かない
    let recent = Utc::now() - Duration::minutes(1);
    set_last_used(&app, only_token(&app, user.id).await, recent).await;
    let written = only_token(&app, user.id).await.last_used_at;
    assert_eq!(
        app.get_with_bearer("/v1/personal_tokens/me", &token)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        only_token(&app, user.id).await.last_used_at,
        written,
        "5 分未満の再使用では書き直さない"
    );

    // 6 分前: 書き直す
    let old = Utc::now() - Duration::minutes(6);
    set_last_used(&app, only_token(&app, user.id).await, old).await;
    let old_value = only_token(&app, user.id)
        .await
        .last_used_at
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(
        app.get_with_bearer("/v1/personal_tokens/me", &token)
            .await
            .status(),
        StatusCode::OK
    );
    let new_value = only_token(&app, user.id)
        .await
        .last_used_at
        .unwrap()
        .with_timezone(&Utc);
    assert!(
        new_value - old_value >= Duration::minutes(5),
        "5 分以上空いたら書き直す"
    );

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn rejected_authentication_does_not_record_use() {
    let app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;

    // 失効
    let revoked_token = app.insert_pat(user.id, tenant_id, vec![], None).await;
    let mut revoked: personal_tokens::ActiveModel = only_token(&app, user.id).await.into();
    revoked.revoked = Set(true);
    revoked.update(&app.state.db).await.expect("revoke");
    let response = app
        .get_with_bearer("/v1/personal_tokens/me", &revoked_token)
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(only_token(&app, user.id).await.last_used_at.is_none());

    // 期限切れ
    let user2 = app.insert_user_default().await;
    let tenant2 = insert_tenant(&app.state.db, user2.id).await;
    let expired_token = app.insert_pat(user2.id, tenant2, vec![], None).await;
    let mut expired: personal_tokens::ActiveModel = only_token(&app, user2.id).await.into();
    expired.expires_at = Set(Some((Utc::now() - Duration::minutes(1)).into()));
    expired.update(&app.state.db).await.expect("expire");
    let response = app
        .get_with_bearer("/v1/personal_tokens/me", &expired_token)
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(only_token(&app, user2.id).await.last_used_at.is_none());

    // 凍結した利用者
    let suspended = insert_user(&app.state.db, false, true).await;
    let tenant3 = insert_tenant(&app.state.db, suspended.id).await;
    let suspended_token = app.insert_pat(suspended.id, tenant3, vec![], None).await;
    let response = app
        .get_with_bearer("/v1/personal_tokens/me", &suspended_token)
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(only_token(&app, suspended.id).await.last_used_at.is_none());

    app.cleanup_user(user.id).await;
    app.cleanup_user(user2.id).await;
    app.cleanup_user(suspended.id).await;
}

#[tokio::test]
async fn bot_token_use_records_last_used_at() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let tenant = app.insert_tenant_project(owner.id).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let created = json_body(
        app.post_json_with_session(
            &format!("/v1/tenants/{}/bots", tenant.tenant_id),
            json!({"username": "last-used-bot", "role": "Member"}),
        )
        .await,
    )
    .await;
    let bot: Uuid = created["id"].as_str().unwrap().parse().unwrap();
    let issued = json_body(
        app.post_json_with_session(
            &format!("/v1/tenants/{}/bots/{bot}/tokens", tenant.tenant_id),
            json!({
                "name": "pipeline",
                "scopes": ["read:task"],
                "expires_at": Utc::now() + Duration::days(30),
            }),
        )
        .await,
    )
    .await;
    let token = issued["token"].as_str().unwrap().to_owned();
    assert!(only_token(&app, bot).await.last_used_at.is_none());

    let response = app.get_with_bearer("/v1/personal_tokens/me", &token).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        only_token(&app, bot).await.last_used_at.is_some(),
        "BOT のトークンでも使用が記録される"
    );

    app.cleanup_user(owner.id).await;
}
