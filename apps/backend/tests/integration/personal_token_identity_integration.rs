use crate::common::{TestApp, insert_tenant};
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use entity::{personal_tokens, scopes::Scope, users};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

async fn token_for_user(app: &TestApp, user_id: Uuid) -> personal_tokens::Model {
    personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(user_id))
        .one(&app.state.db)
        .await
        .expect("query token")
        .expect("inserted token")
}

async fn username_for_user(app: &TestApp, user_id: Uuid) -> String {
    users::Entity::find_by_id(user_id)
        .one(&app.state.db)
        .await
        .expect("query user")
        .expect("inserted user")
        .username
}

#[tokio::test]
async fn token_without_scopes_can_identify_itself() {
    let app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let token = app.insert_pat(user.id, tenant_id, vec![], None).await;
    let record = token_for_user(&app, user.id).await;
    let username = username_for_user(&app, user.id).await;

    let response = app.get_with_bearer("/v1/personal_tokens/me", &token).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .json::<serde_json::Value>()
        .await
        .expect("identity body");
    assert_eq!(body["id"], record.id.to_string());
    assert_eq!(body["name"], record.name);
    assert_eq!(body["user_id"], user.id.to_string());
    assert_eq!(body["username"], username);
    assert_eq!(body["tenant_id"], tenant_id.to_string());
    assert_eq!(body["scopes"], serde_json::json!([]));
    assert_eq!(body["allowed_project_ids"], serde_json::Value::Null);
    assert_eq!(body["expires_at"], serde_json::Value::Null);
    assert!(body.get("email").is_none());
    assert!(body.get("has_password").is_none());
    assert!(body.get("totp_enabled").is_none());
    assert!(body.get("is_admin").is_none());

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn scoped_and_project_bound_token_reports_its_bounds() {
    let app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let project_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
    let token = app
        .insert_pat(
            user.id,
            tenant_id,
            vec![Scope::ReadTask],
            Some(project_ids.clone()),
        )
        .await;

    let response = app.get_with_bearer("/v1/personal_tokens/me", &token).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .json::<serde_json::Value>()
        .await
        .expect("identity body");
    assert_eq!(body["scopes"], serde_json::json!(["read:task"]));
    assert_eq!(body["allowed_project_ids"], serde_json::json!(project_ids));

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn identity_returns_only_the_tenant_bound_to_the_token() {
    let app = TestApp::new().await;
    let user_a = app.insert_user_default().await;
    let tenant_a = insert_tenant(&app.state.db, user_a.id).await;
    let token_a = app.insert_pat(user_a.id, tenant_a, vec![], None).await;
    let user_b = app.insert_user_default().await;
    let tenant_b = insert_tenant(&app.state.db, user_b.id).await;
    let token_b = app.insert_pat(user_b.id, tenant_b, vec![], None).await;

    // それぞれの鍵は、己のバインド先だけを名乗る——他方の陣は現れぬ
    let body_a = app
        .get_with_bearer("/v1/personal_tokens/me", &token_a)
        .await
        .json::<serde_json::Value>()
        .await
        .expect("identity body a");
    assert_eq!(body_a["tenant_id"], tenant_a.to_string());
    assert_ne!(body_a["tenant_id"], tenant_b.to_string());

    let body_b = app
        .get_with_bearer("/v1/personal_tokens/me", &token_b)
        .await
        .json::<serde_json::Value>()
        .await
        .expect("identity body b");
    assert_eq!(body_b["tenant_id"], tenant_b.to_string());
    assert_ne!(body_b["tenant_id"], tenant_a.to_string());

    app.cleanup_user(user_a.id).await;
    app.cleanup_user(user_b.id).await;
}

#[tokio::test]
async fn missing_session_revoked_and_expired_credentials_are_rejected() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;

    let missing = app.get_with_session("/v1/personal_tokens/me").await;
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    app.login_session_no_content(&user.email, &user.password)
        .await;
    let session = app.get_with_session("/v1/personal_tokens/me").await;
    assert_eq!(session.status(), StatusCode::UNAUTHORIZED);
    app.reset_session_client();

    let revoked_token = app
        .insert_pat(user.id, tenant_id, vec![Scope::ReadTask], None)
        .await;
    let mut revoked: personal_tokens::ActiveModel = token_for_user(&app, user.id).await.into();
    revoked.revoked = Set(true);
    revoked.update(&app.state.db).await.expect("revoke token");
    let response = app
        .get_with_bearer("/v1/personal_tokens/me", &revoked_token)
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let expired_token = app
        .insert_pat(user.id, tenant_id, vec![Scope::ReadTask], None)
        .await;
    let mut expired: personal_tokens::ActiveModel = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(user.id))
        .filter(personal_tokens::Column::Revoked.eq(false))
        .one(&app.state.db)
        .await
        .expect("query active token")
        .expect("inserted active token")
        .into();
    expired.expires_at = Set(Some((Utc::now() - Duration::minutes(1)).into()));
    expired.update(&app.state.db).await.expect("expire token");
    let response = app
        .get_with_bearer("/v1/personal_tokens/me", &expired_token)
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    app.cleanup_user(user.id).await;
}
