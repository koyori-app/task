mod common;

use axum::http::StatusCode;
use backend::error::AppError;
use backend::handlers::admin_users::ensure_not_last_admin;
use common::{TestApp, insert_personal_token_for_test, insert_tenant, insert_user};
use entity::{audit_logs, project_statuses, tasks, users};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

#[tokio::test]
async fn admin_users_integration_suite() {
    let mut app = TestApp::new().await;

    // Test 1: 非 admin ユーザーは AdminUser 保護 API で 403
    {
        let user = app.insert_user(false, false).await;
        app.reset_session_client();
        app.login_session(&user.email, &user.password).await;
        let response = app.get_with_session("/v1/admin/users").await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = response.text().await.expect("body");
        assert!(body.contains("forbidden"));
        app.cleanup_user(user.id).await;
    }

    // Test 2: is_suspended ユーザーはセッション / PAT とも 403
    {
        let user = insert_user(&app.state.db, false, false).await;
        let tenant_id = insert_tenant(&app.state.db, user.id).await;
        app.reset_session_client();
        app.login_session(&user.email, &user.password).await;
        let pat = insert_personal_token_for_test(
            &app.state.db,
            user.id,
            tenant_id,
            &app.state.settings.personal_token_secret,
        )
        .await;

        let active = users::Entity::find_by_id(user.id)
            .one(&app.state.db)
            .await
            .expect("load user")
            .expect("user exists");
        let mut active: users::ActiveModel = active.into();
        active.is_suspended = sea_orm::ActiveValue::Set(true);
        active.update(&app.state.db).await.expect("suspend user");

        let session_response = app.get_with_session("/v1/admin/users").await;
        assert_eq!(session_response.status(), StatusCode::FORBIDDEN);
        let session_body = session_response.text().await.expect("session body");
        assert!(session_body.contains("account-suspended"));

        let pat_response = app.get_with_bearer("/v1/tenants", &pat).await;
        assert_eq!(pat_response.status(), StatusCode::FORBIDDEN);
        let pat_body = pat_response.text().await.expect("pat body");
        assert!(pat_body.contains("account-suspended"));
        app.cleanup_user(user.id).await;
    }

    // Test 3: 最後の admin 削除/降格ガード
    {
        let sole_admin = app.insert_user(true, false).await;
        let saved_admins: Vec<(uuid::Uuid, bool)> = users::Entity::find()
            .filter(users::Column::IsAdmin.eq(true))
            .all(&app.state.db)
            .await
            .expect("list admins")
            .into_iter()
            .map(|u| (u.id, u.is_admin))
            .collect();

        for (id, _) in &saved_admins {
            if *id != sole_admin.id
                && let Some(user) = users::Entity::find_by_id(*id)
                    .one(&app.state.db)
                    .await
                    .expect("load admin")
            {
                let mut active: users::ActiveModel = user.into();
                active.is_admin = sea_orm::ActiveValue::Set(false);
                active.update(&app.state.db).await.expect("demote admin");
            }
        }

        let result = ensure_not_last_admin(&app.state.db, sole_admin.id).await;
        assert!(matches!(result, Err(AppError::Forbidden)));

        for (id, was_admin) in saved_admins {
            if let Some(user) = users::Entity::find_by_id(id)
                .one(&app.state.db)
                .await
                .expect("restore load")
            {
                let mut active: users::ActiveModel = user.into();
                active.is_admin = sea_orm::ActiveValue::Set(was_admin);
                active.update(&app.state.db).await.expect("restore admin");
            }
        }
        app.cleanup_user(sole_admin.id).await;
    }

    // Test 4: admin ユーザー作成時に監査ログが記録される
    {
        let admin = app.insert_user(true, false).await;
        app.reset_session_client();
        app.login_session(&admin.email, &admin.password).await;
        let new_email = format!("created-{}@example.com", uuid::Uuid::new_v4());
        let payload = serde_json::json!({
            "username": "audit_test_user",
            "email": new_email,
            "password": "TestPassword123!",
            "is_admin": false,
            "email_verified": true
        });

        let response = app
            .post_json_with_session_and_headers(
                "/v1/admin/users",
                payload,
                "203.0.113.42",
                "AdminIntegrationTest/1.0",
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let log = audit_logs::Entity::find()
            .filter(audit_logs::Column::Action.eq("user.create"))
            .filter(audit_logs::Column::ActorId.eq(Some(admin.id)))
            .filter(audit_logs::Column::IpAddress.eq(Some("203.0.113.42".to_string())))
            .filter(audit_logs::Column::UserAgent.eq(Some("AdminIntegrationTest/1.0".to_string())))
            .one(&app.state.db)
            .await
            .expect("query audit log")
            .expect("audit log row");

        assert_eq!(log.actor_id, Some(admin.id));
        assert_eq!(log.action, "user.create");
        assert_eq!(log.resource_type, "user");

        if let Ok(created_id) = uuid::Uuid::parse_str(&log.resource_id) {
            app.cleanup_user(created_id).await;
        }
        app.cleanup_user(admin.id).await;
    }

    // Test 5: 管理者 suspend でセッション無効化 + PAT revoke
    {
        let target = app.insert_user(false, false).await;
        let tenant_id = insert_tenant(&app.state.db, target.id).await;
        app.reset_session_client();
        app.login_session(&target.email, &target.password).await;
        let target_session_client = app.session_client();
        let pat = insert_personal_token_for_test(
            &app.state.db,
            target.id,
            tenant_id,
            &app.state.settings.personal_token_secret,
        )
        .await;

        let admin = app.insert_user(true, false).await;
        app.reset_session_client();
        app.login_session(&admin.email, &admin.password).await;

        let suspend_response = app
            .patch_json_with_session(
                &format!("/v1/admin/users/{}", target.id),
                serde_json::json!({ "is_suspended": true }),
            )
            .await;
        assert_eq!(suspend_response.status(), StatusCode::OK);

        let session_response = target_session_client
            .get(format!("{}/v1/tenants", app.base_url()))
            .send()
            .await
            .expect("target session request");
        assert_eq!(session_response.status(), StatusCode::UNAUTHORIZED);

        let pat_response = app.get_with_bearer("/v1/tenants", &pat).await;
        assert_eq!(pat_response.status(), StatusCode::UNAUTHORIZED);

        let token_row = entity::personal_tokens::Entity::find()
            .filter(entity::personal_tokens::Column::UserId.eq(target.id))
            .one(&app.state.db)
            .await
            .expect("load pat")
            .expect("pat exists");
        assert!(token_row.revoked);

        let user_row = users::Entity::find_by_id(target.id)
            .one(&app.state.db)
            .await
            .expect("load user")
            .expect("user exists");
        assert!(user_row.sessions_revoked_at.is_some());

        app.cleanup_user(target.id).await;
        app.cleanup_user(admin.id).await;
    }

    // Test 6: 管理者削除は tasks を soft-delete しユーザーを匿名化（物理削除しない）
    {
        let target = app.insert_user(false, false).await;
        let tp = app.insert_tenant_project(target.id).await;
        let status_id = Uuid::new_v4();
        project_statuses::ActiveModel {
            id: Set(status_id),
            project_id: Set(tp.project_id),
            name: Set("Todo".into()),
            color: Set("#808080".into()),
            position: Set(0),
            is_default: Set(true),
            is_done_state: Set(false),
            created_at: Set(chrono::Utc::now().into()),
        }
        .insert(&app.state.db)
        .await
        .expect("insert status");

        let task_id = Uuid::new_v4();
        tasks::ActiveModel {
            id: Set(task_id),
            project_id: Set(tp.project_id),
            seq_id: Set(1),
            title: Set("admin delete test".into()),
            description: Set(None),
            status_id: Set(status_id),
            priority: Set(tasks::TaskPriority::Medium),
            progress_pct: Set(0),
            parent_task_id: Set(None),
            milestone_id: Set(None),
            sprint_id: Set(None),
            soft_deadline: Set(None),
            hard_deadline: Set(None),
            estimated_minutes: Set(None),
            is_archived: Set(false),
            created_by: Set(target.id),
            created_at: Set(chrono::Utc::now().into()),
            updated_at: Set(chrono::Utc::now().into()),
            completed_at: Set(None),
            deleted_at: Set(None),
        }
        .insert(&app.state.db)
        .await
        .expect("insert task");

        let admin = app.insert_user(true, false).await;
        app.reset_session_client();
        app.login_session(&admin.email, &admin.password).await;

        let delete_response = app
            .delete_with_session(&format!("/v1/admin/users/{}", target.id))
            .await;
        assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

        let task_row = tasks::Entity::find_by_id(task_id)
            .one(&app.state.db)
            .await
            .expect("load task")
            .expect("task still exists (not physically deleted)");
        assert_eq!(task_row.created_by, target.id);
        assert!(
            task_row.deleted_at.is_some(),
            "cascade must soft-delete owned tasks"
        );

        let user_row = users::Entity::find_by_id(target.id)
            .one(&app.state.db)
            .await
            .expect("load user")
            .expect("user still exists");
        assert!(user_row.email.starts_with("deleted+"));
        assert!(user_row.is_suspended);
        assert!(user_row.password_hash.is_none());

        app.cleanup_user(target.id).await;
        app.cleanup_user(admin.id).await;
    }

    // Test 7: 2FA 強制リセットは 2FA 未設定ユーザーに対しても 204 を返す
    // （information_schema 検査の `?` プレースホルダ未変換で常に 500 だった回帰の検知）
    {
        let target = app.insert_user(false, false).await;
        let admin = app.insert_user(true, false).await;
        app.reset_session_client();
        app.login_session(&admin.email, &admin.password).await;

        let response = app
            .post_json_with_session(
                &format!("/v1/admin/users/{}/reset-2fa", target.id),
                serde_json::json!({}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        app.cleanup_user(target.id).await;
        app.cleanup_user(admin.id).await;
    }
}
