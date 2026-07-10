mod common;

use axum::http::StatusCode;
use common::TestApp;
use entity::users;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

const REGISTER_SUCCESS_MESSAGE: &str = "Register successful";

#[tokio::test]
async fn register_integration_suite() {
    let app = TestApp::new().await;

    // Test 1: 未使用のメールアドレスでの登録は 201 + 成功メッセージ、ユーザー行が作られる
    {
        let email = format!("register-new-{}@example.com", uuid::Uuid::new_v4());
        let resp = app
            .post_json(
                "/v1/auth/register",
                serde_json::json!({
                    "username": "register_new_user",
                    "email": email,
                    "password": "TestPassword123!"
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body: String = resp.json().await.expect("json");
        assert_eq!(body, REGISTER_SUCCESS_MESSAGE);

        let user = users::Entity::find()
            .filter(users::Column::Email.eq(&email))
            .one(&app.state.db)
            .await
            .expect("query user")
            .expect("user row exists after register");

        app.cleanup_user(user.id).await;
    }

    // Test 2 (#26): 既に使用されているメールアドレスでも、未使用時と同一の 201 +
    // 同一メッセージを返す（メールアドレス列挙対策）。新規ユーザー行は作られない。
    {
        let existing = app.insert_user_default().await;

        let resp = app
            .post_json(
                "/v1/auth/register",
                serde_json::json!({
                    "username": "register_duplicate_attempt",
                    "email": existing.email,
                    "password": "AnotherPassword456!"
                }),
            )
            .await;
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "duplicate email must return the same status as a new registration"
        );
        let body: String = resp.json().await.expect("json");
        assert_eq!(
            body, REGISTER_SUCCESS_MESSAGE,
            "duplicate email must return the same body as a new registration"
        );

        let count = users::Entity::find()
            .filter(users::Column::Email.eq(&existing.email))
            .count(&app.state.db)
            .await
            .expect("count users by email");
        assert_eq!(count, 1, "no duplicate user row should be created");

        app.cleanup_user(existing.id).await;
    }
}
