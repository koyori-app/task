use crate::common::TestApp;
use axum::http::StatusCode;
use entity::scopes::Scope;
use serde_json::Value;

#[tokio::test]
async fn task_notifications_integration_suite() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;

    let status_path = format!(
        "/v1/tenants/{}/projects/{}/statuses",
        tp.tenant_id, tp.project_id
    );
    let status_resp = app
        .post_json_with_session(
            &status_path,
            serde_json::json!({"name":"Backlog","color":"#336699","position":0,"is_default":true}),
        )
        .await;
    assert_eq!(status_resp.status(), StatusCode::CREATED);
    let status_id = status_resp.json::<serde_json::Value>().await.expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();

    let tasks_path = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let task_resp = app
        .post_json_with_session(
            &tasks_path,
            serde_json::json!({"title":"Notify task","status_id":status_id}),
        )
        .await;
    assert_eq!(task_resp.status(), StatusCode::CREATED);
    let task_id = task_resp.json::<serde_json::Value>().await.expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();

    let assignee = app.insert_user(false, false).await;
    let assignee_username = format!("test_{}", &assignee.id.to_string()[..8]);

    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, assignee.id)
        .await;
    let member_resp = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/members",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({"user_id": assignee.id, "role": "Member"}),
        )
        .await;
    assert_eq!(member_resp.status(), StatusCode::CREATED);

    let task_base = format!(
        "/v1/tenants/{}/projects/{}/tasks/{}",
        tp.tenant_id, tp.project_id, task_id
    );
    let assign_resp = app
        .post_json_with_session(
            &format!("{task_base}/assignees"),
            serde_json::json!({"user_id": assignee.id, "role": "primary"}),
        )
        .await;
    assert_eq!(assign_resp.status(), StatusCode::CREATED);

    let watchers = app.get_with_session(&format!("{task_base}/watchers")).await;
    assert_eq!(watchers.status(), StatusCode::OK);
    assert_eq!(
        watchers.json::<serde_json::Value>().await.expect("json")["watchers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    app.reset_session_client();
    app.login_session_no_content(&assignee.email, &assignee.password)
        .await;
    let notif = app.get_with_session("/v1/users/me/notifications").await;
    assert_eq!(notif.status(), StatusCode::OK);
    let body: Value = notif.json::<serde_json::Value>().await.expect("json");
    assert_eq!(body["unread_count"].as_u64(), Some(1));

    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    assert_eq!(
        app.post_json_with_session(&format!("{task_base}/watch"), serde_json::json!({}))
            .await
            .status(),
        StatusCode::CREATED
    );
    let comment = app
        .post_json_with_session(
            &format!("{task_base}/comments"),
            serde_json::json!({"body": format!("@{assignee_username} review")}),
        )
        .await;
    assert_eq!(comment.status(), StatusCode::CREATED);

    app.reset_session_client();
    app.login_session_no_content(&assignee.email, &assignee.password)
        .await;
    let unread = app
        .get_with_session("/v1/users/me/notifications?unread=true")
        .await;
    let unread_body: Value = unread.json::<Value>().await.expect("json");
    let types: Vec<&str> = unread_body["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["notification_type"].as_str())
        .collect();
    assert!(types.contains(&"mentioned"));

    // 手動ウォッチは一覧に出て、解除すると消える
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let owner_id = owner.id.to_string();
    let watching: Value = app
        .get_with_session(&format!("{task_base}/watchers"))
        .await
        .json()
        .await
        .expect("json");
    assert!(
        watching["watchers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["id"].as_str() == Some(owner_id.as_str())),
        "手動ウォッチした本人が一覧に出る"
    );

    assert_eq!(
        app.delete_with_session(&format!("{task_base}/watch"))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let unwatched: Value = app
        .get_with_session(&format!("{task_base}/watchers"))
        .await
        .json()
        .await
        .expect("json");
    assert!(
        !unwatched["watchers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["id"].as_str() == Some(owner_id.as_str())),
        "解除すると一覧から消える"
    );
}

#[tokio::test]
async fn mark_notification_read_and_read_all() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;

    let status_resp = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/statuses",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({"name":"Todo","color":"#aabbcc","position":0,"is_default":true}),
        )
        .await;
    let status_id = status_resp.json::<Value>().await.expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();

    let assignee = app.insert_user(false, false).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, assignee.id)
        .await;
    app.post_json_with_session(
        &format!(
            "/v1/tenants/{}/projects/{}/members",
            tp.tenant_id, tp.project_id
        ),
        serde_json::json!({"user_id": assignee.id, "role": "Member"}),
    )
    .await;

    // 2件通知を生成（担当者追加×2タスク）
    for title in ["Task A", "Task B"] {
        let task_resp = app
            .post_json_with_session(
                &format!(
                    "/v1/tenants/{}/projects/{}/tasks",
                    tp.tenant_id, tp.project_id
                ),
                serde_json::json!({"title": title, "status_id": status_id}),
            )
            .await;
        let task_id = task_resp.json::<Value>().await.expect("json")["id"]
            .as_str()
            .unwrap()
            .to_string();
        app.post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/tasks/{}/assignees",
                tp.tenant_id, tp.project_id, task_id
            ),
            serde_json::json!({"user_id": assignee.id, "role": "primary"}),
        )
        .await;
    }

    app.reset_session_client();
    app.login_session_no_content(&assignee.email, &assignee.password)
        .await;

    let notifs: Value = app
        .get_with_session("/v1/users/me/notifications")
        .await
        .json()
        .await
        .expect("json");
    assert_eq!(notifs["unread_count"].as_u64(), Some(2));
    let notif_id = notifs["notifications"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // 1件既読
    let read_resp = app
        .patch_json_with_session(
            &format!("/v1/users/me/notifications/{}/read", notif_id),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(read_resp.status(), StatusCode::OK);
    let read_body: Value = read_resp.json().await.expect("json");
    assert!(read_body["read_at"].as_str().is_some());

    let notifs2: Value = app
        .get_with_session("/v1/users/me/notifications")
        .await
        .json()
        .await
        .expect("json");
    assert_eq!(notifs2["unread_count"].as_u64(), Some(1));

    // 全件既読
    let all_read = app
        .patch_json_with_session("/v1/users/me/notifications/read-all", serde_json::json!({}))
        .await;
    assert_eq!(all_read.status(), StatusCode::NO_CONTENT);

    let notifs3: Value = app
        .get_with_session("/v1/users/me/notifications")
        .await
        .json()
        .await
        .expect("json");
    assert_eq!(notifs3["unread_count"].as_u64(), Some(0));
}

#[tokio::test]
async fn status_changed_notification_to_watcher() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;

    let status_path = format!(
        "/v1/tenants/{}/projects/{}/statuses",
        tp.tenant_id, tp.project_id
    );
    let status_a_id = app
        .post_json_with_session(
            &status_path,
            serde_json::json!({"name":"Todo","color":"#aaaaaa","position":0,"is_default":true}),
        )
        .await
        .json::<Value>()
        .await
        .expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let status_b_id = app
        .post_json_with_session(
            &status_path,
            serde_json::json!({"name":"Done","color":"#bbbbbb","position":1,"is_default":false}),
        )
        .await
        .json::<Value>()
        .await
        .expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();

    let watcher = app.insert_user(false, false).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, watcher.id).await;
    app.post_json_with_session(
        &format!(
            "/v1/tenants/{}/projects/{}/members",
            tp.tenant_id, tp.project_id
        ),
        serde_json::json!({"user_id": watcher.id, "role": "Member"}),
    )
    .await;

    let task_id = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/tasks",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({"title":"Status test","status_id":status_a_id}),
        )
        .await
        .json::<Value>()
        .await
        .expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let task_base = format!(
        "/v1/tenants/{}/projects/{}/tasks/{}",
        tp.tenant_id, tp.project_id, task_id
    );

    // ウォッチャーをタスクに追加
    app.post_json_with_session(
        &format!(
            "/v1/tenants/{}/projects/{}/tasks/{}/assignees",
            tp.tenant_id, tp.project_id, task_id
        ),
        serde_json::json!({"user_id": watcher.id, "role": "primary"}),
    )
    .await;

    // ステータス変更（owner が実行）
    let update_resp = app
        .put_json_with_session(&task_base, serde_json::json!({"status_id": status_b_id}))
        .await;
    assert_eq!(update_resp.status(), StatusCode::OK);

    // ウォッチャーに status_changed 通知が届いていることを確認
    app.reset_session_client();
    app.login_session_no_content(&watcher.email, &watcher.password)
        .await;
    let notifs: Value = app
        .get_with_session("/v1/users/me/notifications")
        .await
        .json()
        .await
        .expect("json");
    let types: Vec<&str> = notifs["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["notification_type"].as_str())
        .collect();
    assert!(
        types.contains(&"status_changed"),
        "status_changed notification not found: {:?}",
        types
    );
}

#[tokio::test]
async fn notification_settings_get_update_and_validation() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let settings_path = format!("/v1/users/me/notification-settings/{}", tp.project_id);

    // デフォルト設定の取得
    let defaults: Value = app
        .get_with_session(&settings_path)
        .await
        .json()
        .await
        .expect("json");
    assert!(
        defaults["in_app_events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str() == Some("assigned"))
    );

    // 有効な設定の更新
    let update_resp = app
        .put_json_with_session(
            &settings_path,
            serde_json::json!({
                "email_events": [],
                "in_app_events": ["assigned", "mentioned"]
            }),
        )
        .await;
    assert_eq!(update_resp.status(), StatusCode::OK);
    let updated: Value = update_resp.json().await.expect("json");
    assert_eq!(updated["in_app_events"].as_array().unwrap().len(), 2);

    // 変更が永続化されていることを確認
    let fetched: Value = app
        .get_with_session(&settings_path)
        .await
        .json()
        .await
        .expect("json");
    assert_eq!(fetched["in_app_events"], updated["in_app_events"]);

    // 不正なイベントタイプ → 422
    let invalid_resp = app
        .put_json_with_session(
            &settings_path,
            serde_json::json!({
                "email_events": [],
                "in_app_events": ["assigned", "invalid_event_xyz"]
            }),
        )
        .await;
    assert_eq!(invalid_resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn mention_notifies_tenant_owner_non_member() {
    let mut app = TestApp::new().await;

    // owner（tenant owner、project member にはしない）
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let owner_username = format!("test_{}", &owner.id.to_string()[..8]);

    // project member を作成
    let member = app.insert_user(false, false).await;

    // owner でログインして member を project に追加
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    let member_resp = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/members",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({"user_id": member.id, "role": "Member"}),
        )
        .await;
    assert_eq!(member_resp.status(), StatusCode::CREATED);

    // ステータスとタスクを作成
    let status_resp = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/statuses",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({"name":"Backlog","color":"#336699","position":0,"is_default":true}),
        )
        .await;
    assert_eq!(status_resp.status(), StatusCode::CREATED);
    let status_id = status_resp.json::<Value>().await.expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();

    let task_resp = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/tasks",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({"title":"Mention owner test","status_id":status_id}),
        )
        .await;
    assert_eq!(task_resp.status(), StatusCode::CREATED);
    let task_id = task_resp.json::<Value>().await.expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();

    // member でログインして @owner_username を含むコメントを投稿
    app.reset_session_client();
    app.login_session_no_content(&member.email, &member.password)
        .await;
    let comment_resp = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/tasks/{}/comments",
                tp.tenant_id, tp.project_id, task_id
            ),
            serde_json::json!({"body": format!("@{} please review", owner_username)}),
        )
        .await;
    assert_eq!(comment_resp.status(), StatusCode::CREATED);

    // owner でログインして通知を確認
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let notifs: Value = app
        .get_with_session("/v1/users/me/notifications")
        .await
        .json()
        .await
        .expect("json");

    let types: Vec<&str> = notifs["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["notification_type"].as_str())
        .collect();
    assert!(
        types.contains(&"mentioned"),
        "tenant owner should receive mention notification even if not a project member, got: {:?}",
        types
    );
}

// ---------------------------------------------------------------------------
// PAT からの通知 API（TASK-225）。CLI は PAT で叩くので、視界を
// 「PAT のテナントのプロジェクト ∩ allowed_project_ids」に絞れているかを固定する。
// 規則は docs/features/tasks/5.notifications.md §4。
// ---------------------------------------------------------------------------

/// 通知を 1 件直に挿す（どの経路で作られたかは視界の判定に関係しない）。
async fn insert_notification(
    app: &TestApp,
    user_id: uuid::Uuid,
    project_id: Option<uuid::Uuid>,
    notification_type: &str,
) -> uuid::Uuid {
    use sea_orm::ActiveModelTrait;
    use sea_orm::ActiveValue::Set;

    let id = uuid::Uuid::new_v4();
    entity::notifications::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        task_id: Set(None),
        project_id: Set(project_id),
        notification_type: Set(notification_type.into()),
        payload: Set(serde_json::json!({ "pr_number": 618, "round": 1 })),
        read_at: Set(None),
        created_at: Set(chrono::Utc::now().into()),
        email_queued_at: Set(None),
        emailed_at: Set(None),
        email_attempts: Set(0),
    }
    .insert(&app.state.db)
    .await
    .expect("insert notification");
    id
}

/// 2 つのテナントにまたがる通知を持つ利用者。戻り値は
/// (テナント A の 2 プロジェクト, テナント B のプロジェクト, プロジェクト無しの通知)。
struct PatFixture {
    app: TestApp,
    user: crate::common::TestUser,
    tenant_a: uuid::Uuid,
    project_a1: uuid::Uuid,
    project_a2: uuid::Uuid,
    project_b: uuid::Uuid,
    notification_a1: uuid::Uuid,
    notification_a2: uuid::Uuid,
    notification_b: uuid::Uuid,
}

async fn pat_fixture() -> PatFixture {
    let app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let a = app.insert_tenant_project(user.id).await;
    let project_a2 = crate::common::insert_extra_project(&app, a.tenant_id).await;
    let b = app.insert_tenant_project(user.id).await;

    let notification_a1 =
        insert_notification(&app, user.id, Some(a.project_id), "review_round_created").await;
    let notification_a2 =
        insert_notification(&app, user.id, Some(project_a2), "review_round_created").await;
    let notification_b =
        insert_notification(&app, user.id, Some(b.project_id), "review_round_created").await;
    // project_id を持たせる前に作られた行。どのプロジェクトのものか分からないので PAT には見せない
    insert_notification(&app, user.id, None, "review_round_created").await;

    PatFixture {
        app,
        user,
        tenant_a: a.tenant_id,
        project_a1: a.project_id,
        project_a2,
        project_b: b.project_id,
        notification_a1,
        notification_a2,
        notification_b,
    }
}

async fn list_with_bearer(app: &TestApp, token: &str) -> Value {
    let res = app
        .get_with_bearer("/v1/users/me/notifications", token)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    res.json::<Value>().await.expect("json")
}

fn project_ids_of(body: &Value) -> Vec<String> {
    body["notifications"]
        .as_array()
        .expect("notifications")
        .iter()
        .map(|n| n["project_id"].as_str().unwrap_or("null").to_string())
        .collect()
}

/// PAT の視界はバインド先テナントのプロジェクトまで。`allowed_project_ids` があれば
/// さらにその中だけ。`project_id` の無い行はセッションにだけ見せる。
#[tokio::test]
async fn pat_sees_only_the_notifications_of_its_tenant_and_projects() {
    let mut fx = pat_fixture().await;

    // 対照: セッションは 4 件すべて見える（過剰に絞っていないこと）
    fx.app
        .login_session_no_content(&fx.user.email, &fx.user.password)
        .await;
    let session = fx.app.get_with_session("/v1/users/me/notifications").await;
    assert_eq!(session.status(), StatusCode::OK);
    let session_body: Value = session.json().await.expect("json");
    assert_eq!(session_body["unread_count"].as_u64(), Some(4));

    // テナント A にバインドした PAT: A の 2 件だけ（B と project_id 無しは数にも入らない）
    let tenant_token = fx
        .app
        .insert_pat(fx.user.id, fx.tenant_a, vec![Scope::ReadReview], None)
        .await;
    let body = list_with_bearer(&fx.app, &tenant_token).await;
    assert_eq!(body["unread_count"].as_u64(), Some(2));
    let mut seen = project_ids_of(&body);
    seen.sort();
    let mut expected = vec![fx.project_a1.to_string(), fx.project_a2.to_string()];
    expected.sort();
    assert_eq!(seen, expected);

    // プロジェクトを絞った PAT: 絞った 1 件だけ
    let project_token = fx
        .app
        .insert_pat(
            fx.user.id,
            fx.tenant_a,
            vec![Scope::ReadReview],
            Some(vec![fx.project_a1]),
        )
        .await;
    let body = list_with_bearer(&fx.app, &project_token).await;
    assert_eq!(body["unread_count"].as_u64(), Some(1));
    assert_eq!(project_ids_of(&body), vec![fx.project_a1.to_string()]);

    // スコープの無い PAT は読めない
    let no_scope = fx
        .app
        .insert_pat(fx.user.id, fx.tenant_a, vec![Scope::ReadDrive], None)
        .await;
    assert_eq!(
        fx.app
            .get_with_bearer("/v1/users/me/notifications", &no_scope)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

/// レビュー通知の既読化は `write:review`。プロジェクトの視界は読み取りと揃える。
#[tokio::test]
async fn pat_marks_read_only_inside_its_own_view() {
    let mut fx = pat_fixture().await;

    let read_only = fx
        .app
        .insert_pat(fx.user.id, fx.tenant_a, vec![Scope::ReadReview], None)
        .await;
    assert_eq!(
        fx.app
            .patch_with_bearer("/v1/users/me/notifications/read-all", &read_only)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "読み取りスコープだけでは既読にできない"
    );

    let write = fx
        .app
        .insert_pat(fx.user.id, fx.tenant_a, vec![Scope::WriteReview], None)
        .await;

    // 視界の外（別テナント）の通知は 1 件既読でも見つからない
    assert_eq!(
        fx.app
            .patch_with_bearer(
                &format!("/v1/users/me/notifications/{}/read", fx.notification_b),
                &write,
            )
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    // 視界の中は既読にできる
    assert_eq!(
        fx.app
            .patch_with_bearer(
                &format!("/v1/users/me/notifications/{}/read", fx.notification_a1),
                &write,
            )
            .await
            .status(),
        StatusCode::OK
    );

    // 全件既読が消すのはバインド先テナントのぶんだけ
    assert_eq!(
        fx.app
            .patch_with_bearer("/v1/users/me/notifications/read-all", &write)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        list_with_bearer(&fx.app, &write).await["unread_count"].as_u64(),
        Some(0)
    );

    // 別テナントの通知と project_id 無しの行は未読のまま（セッションで確かめる）
    fx.app
        .login_session_no_content(&fx.user.email, &fx.user.password)
        .await;
    let session: Value = fx
        .app
        .get_with_session("/v1/users/me/notifications")
        .await
        .json()
        .await
        .expect("json");
    assert_eq!(session["unread_count"].as_u64(), Some(2));
    let unread: Vec<&str> = session["notifications"]
        .as_array()
        .expect("notifications")
        .iter()
        .filter(|n| n["read_at"].is_null())
        .map(|n| n["project_id"].as_str().unwrap_or("null"))
        .collect();
    assert!(
        unread.contains(&fx.project_b.to_string().as_str()),
        "{unread:?}"
    );
    assert!(unread.contains(&"null"), "{unread:?}");
}

/// 一覧・未読数・個別/一括既読でタスクとレビューのスコープを混同しない。
#[tokio::test]
async fn pat_separates_task_and_review_notification_permissions() {
    let mut fx = pat_fixture().await;
    let mut task_ids = Vec::new();
    for kind in [
        "assigned",
        "mentioned",
        "status_changed",
        "comment_added",
        "deadline_soon",
        "pr_merged",
    ] {
        task_ids.push(insert_notification(&fx.app, fx.user.id, Some(fx.project_a1), kind).await);
    }
    let changed = insert_notification(
        &fx.app,
        fx.user.id,
        Some(fx.project_a1),
        "review_finding_changed",
    )
    .await;
    let unknown =
        insert_notification(&fx.app, fx.user.id, Some(fx.project_a1), "future_event").await;
    let review_ids = vec![
        fx.notification_a1.to_string(),
        fx.notification_a2.to_string(),
        changed.to_string(),
    ];
    let reviews = fx
        .app
        .insert_pat(fx.user.id, fx.tenant_a, vec![Scope::ReadReview], None)
        .await;
    let task_ids: Vec<String> = task_ids.iter().map(ToString::to_string).collect();
    let all_ids: Vec<String> = task_ids.iter().chain(&review_ids).cloned().collect();

    for (scopes, mut expected) in [
        (vec![Scope::ReadTask], task_ids.clone()),
        (vec![Scope::ReadReview], review_ids.clone()),
        (vec![Scope::WriteTask], task_ids.clone()),
        (vec![Scope::WriteReview], review_ids.clone()),
        (vec![Scope::ReadTask, Scope::ReadReview], all_ids.clone()),
        (vec![Scope::AdminProject], all_ids.clone()),
        (vec![Scope::AdminTenant], all_ids.clone()),
    ] {
        let token = fx
            .app
            .insert_pat(fx.user.id, fx.tenant_a, scopes, None)
            .await;
        let body = list_with_bearer(&fx.app, &token).await;
        assert_eq!(body["unread_count"].as_u64(), Some(expected.len() as u64));
        let mut actual: Vec<String> = body["notifications"]
            .as_array()
            .expect("notifications")
            .iter()
            .map(|row| row["id"].as_str().expect("id").to_string())
            .collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }

    let tasks = fx
        .app
        .insert_pat(fx.user.id, fx.tenant_a, vec![Scope::WriteTask], None)
        .await;
    // 種別で絞った中でもカーソルで続きを読める（4 件読んだ続きの 2 件）
    let first: Value = fx
        .app
        .get_with_bearer("/v1/users/me/notifications?limit=4", &tasks)
        .await
        .json()
        .await
        .expect("json");
    let cursor = first["next_cursor"].as_str().expect("next_cursor");
    let page = fx
        .app
        .get_with_bearer(
            &format!("/v1/users/me/notifications?limit=2&cursor={cursor}"),
            &tasks,
        )
        .await;
    assert_eq!(page.status(), StatusCode::OK);
    let page: Value = page.json().await.expect("json");
    assert_eq!(page["unread_count"], 6);
    assert_eq!(
        page["notifications"]
            .as_array()
            .expect("notifications")
            .len(),
        2
    );
    assert!(page["notifications"].as_array().unwrap().iter().all(|row| {
        task_ids
            .iter()
            .any(|id| row["id"].as_str() == Some(id.as_str()))
    }));

    // 個別既読のレスポンスからも、権限のない種別の payload を読めない。
    for id in [fx.notification_a1, changed, unknown] {
        let path = format!("/v1/users/me/notifications/{id}/read");
        assert_eq!(
            fx.app.patch_with_bearer(&path, &tasks).await.status(),
            StatusCode::NOT_FOUND
        );
    }
    let read_only = fx
        .app
        .insert_pat(
            fx.user.id,
            fx.tenant_a,
            vec![Scope::ReadTask, Scope::ReadReview],
            None,
        )
        .await;
    for path in [
        format!("/v1/users/me/notifications/{}/read", task_ids[0]),
        "/v1/users/me/notifications/read-all".into(),
    ] {
        assert_eq!(
            fx.app.patch_with_bearer(&path, &read_only).await.status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        fx.app
            .patch_with_bearer(
                &format!("/v1/users/me/notifications/{}/read", task_ids[0]),
                &tasks
            )
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        fx.app
            .patch_with_bearer("/v1/users/me/notifications/read-all", &tasks)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(list_with_bearer(&fx.app, &tasks).await["unread_count"], 0);
    assert_eq!(list_with_bearer(&fx.app, &reviews).await["unread_count"], 3);

    // タスクの読み取り権限とレビューの書き込み権限を持っていても、タスクは既読にできない。
    let mixed = fx
        .app
        .insert_pat(
            fx.user.id,
            fx.tenant_a,
            vec![Scope::ReadTask, Scope::WriteReview],
            None,
        )
        .await;
    let body = list_with_bearer(&fx.app, &mixed).await;
    assert_eq!(body["notifications"].as_array().unwrap().len(), 9);
    assert_eq!(body["unread_count"], 3);
    let new_task = insert_notification(&fx.app, fx.user.id, Some(fx.project_a1), "assigned").await;
    assert_eq!(
        fx.app
            .patch_with_bearer(
                &format!("/v1/users/me/notifications/{new_task}/read"),
                &mixed
            )
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fx.app
            .patch_with_bearer(
                &format!("/v1/users/me/notifications/{changed}/read"),
                &mixed
            )
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        fx.app
            .patch_with_bearer("/v1/users/me/notifications/read-all", &mixed)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(list_with_bearer(&fx.app, &reviews).await["unread_count"], 0);
    assert_eq!(list_with_bearer(&fx.app, &tasks).await["unread_count"], 1);

    fx.app
        .login_session_no_content(&fx.user.email, &fx.user.password)
        .await;
    let session: Value = fx
        .app
        .get_with_session("/v1/users/me/notifications")
        .await
        .json()
        .await
        .expect("json");
    assert_eq!(session["notifications"].as_array().unwrap().len(), 13);
    // 別テナント・プロジェクト無し・未知の種別・タスクの新着は未読のまま。
    assert_eq!(session["unread_count"], 4);
}

// ---------------------------------------------------------------------------
// カーソル・catch-up・kind・Retention。規則は docs/features/tasks/5.notifications.md §4 / §1.2。
// ---------------------------------------------------------------------------

/// 作成時刻と種別を指定して通知を直に挿す。同時刻の行を作ってカーソルのタイブレーカーを試す。
async fn insert_notification_at(
    app: &TestApp,
    user_id: uuid::Uuid,
    project_id: uuid::Uuid,
    notification_type: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> uuid::Uuid {
    use sea_orm::ActiveModelTrait;
    use sea_orm::ActiveValue::Set;

    let id = uuid::Uuid::new_v4();
    entity::notifications::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        task_id: Set(None),
        project_id: Set(Some(project_id)),
        notification_type: Set(notification_type.into()),
        payload: Set(serde_json::json!({})),
        read_at: Set(None),
        created_at: Set(created_at.into()),
        email_queued_at: Set(None),
        emailed_at: Set(None),
        email_attempts: Set(0),
    }
    .insert(&app.state.db)
    .await
    .expect("insert notification");
    id
}

fn ids(page: &Value) -> Vec<String> {
    page["notifications"]
        .as_array()
        .expect("notifications")
        .iter()
        .map(|n| n["id"].as_str().expect("id").to_string())
        .collect()
}

/// `path` から `key=start` で読み始め、以後 `key=next_cursor` で最後まで読む。
/// (id 列, 各ページの件数) を返す
async fn read_pages(
    app: &TestApp,
    path: &str,
    key: &str,
    start: Option<&str>,
) -> (Vec<String>, Vec<usize>) {
    let mut got = Vec::new();
    let mut sizes = Vec::new();
    let mut next: Option<String> = start.map(String::from);
    loop {
        let url = match &next {
            Some(c) => format!("{path}&{key}={c}"),
            None => path.to_string(),
        };
        let res = app.get_with_session(&url).await;
        assert_eq!(res.status(), StatusCode::OK, "{url}");
        let body = common::json_body(res).await;
        let page = ids(&body);
        sizes.push(page.len());
        got.extend(page);
        match body["next_cursor"].as_str() {
            Some(c) => next = Some(c.to_string()),
            None => break,
        }
    }
    (got, sizes)
}

#[tokio::test]
async fn notifications_cursor_pages_after_and_kind() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tp = app.insert_tenant_project(user.id).await;
    app.login_session_no_content(&user.email, &user.password)
        .await;

    // 上限 100 を越える件数。3 件ずつ同時刻にして id のタイブレーカーを通す
    let base = chrono::Utc::now() - chrono::Duration::hours(1);
    let mut expected = Vec::new();
    for i in 0..120i64 {
        let at = base + chrono::Duration::seconds(i / 3);
        let kind = if i % 4 == 0 {
            "review_round_created"
        } else {
            "assigned"
        };
        let id = insert_notification_at(&app, user.id, tp.project_id, kind, at).await;
        expected.push((at, id, kind));
    }
    // created_at DESC, id DESC
    expected.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    let expected_ids: Vec<String> = expected.iter().map(|(_, id, _)| id.to_string()).collect();

    // cursor で欠落・重複なく最後まで読める
    let (got, sizes) =
        read_pages(&app, "/v1/users/me/notifications?limit=50", "cursor", None).await;
    assert_eq!(sizes, vec![50, 50, 20]);
    assert_eq!(got, expected_ids);

    // limit: 既定 50、上限 100 を越える指定は 100 に切る
    let first = common::json_body(app.get_with_session("/v1/users/me/notifications").await).await;
    assert_eq!(ids(&first).len(), 50);
    assert_eq!(first["unread_count"].as_u64(), Some(120));
    let capped = common::json_body(
        app.get_with_session("/v1/users/me/notifications?limit=101")
            .await,
    )
    .await;
    assert_eq!(ids(&capped).len(), 100);
    assert!(capped["next_cursor"].is_string());

    // project 要約（既存の project_id も残る）
    let row = &first["notifications"][0];
    assert_eq!(row["project_id"], tp.project_id.to_string());
    assert_eq!(row["project"]["id"], tp.project_id.to_string());
    assert_eq!(row["project"]["tenant_id"], tp.tenant_id.to_string());
    assert!(row["project"]["key"].is_string());

    // after: 30 番目に新しい行より新しい 29 件だけを、古い順に next_cursor で取り切る
    let anchor = first["notifications"][29]["cursor"].as_str().unwrap();
    let (newer, sizes) = read_pages(
        &app,
        "/v1/users/me/notifications?limit=10",
        "after",
        Some(anchor),
    )
    .await;
    assert_eq!(sizes, vec![10, 10, 9]);
    let mut want: Vec<String> = expected_ids[..29].to_vec();
    want.reverse();
    assert_eq!(newer, want);
    // 最新行より新しいものは無い
    let newest = first["notifications"][0]["cursor"].as_str().unwrap();
    let none = common::json_body(
        app.get_with_session(&format!("/v1/users/me/notifications?after={newest}"))
            .await,
    )
    .await;
    assert!(ids(&none).is_empty());
    assert!(none["next_cursor"].is_null());

    // kind: review_ 接頭辞で分ける
    let review = common::json_body(
        app.get_with_session("/v1/users/me/notifications?kind=review&limit=100")
            .await,
    )
    .await;
    let want_review: Vec<String> = expected
        .iter()
        .filter(|(_, _, k)| k.starts_with("review_"))
        .map(|(_, id, _)| id.to_string())
        .collect();
    assert_eq!(want_review.len(), 30);
    assert_eq!(ids(&review), want_review);
    let task = common::json_body(
        app.get_with_session("/v1/users/me/notifications?kind=task&limit=100")
            .await,
    )
    .await;
    assert_eq!(ids(&task).len(), 90);
    assert!(
        task["notifications"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["notification_type"] == "assigned")
    );

    // 既読化しても順序は変わらない（未読優先をしない）
    let middle = &expected_ids[60];
    assert_eq!(
        app.patch_json_with_session(
            &format!("/v1/users/me/notifications/{middle}/read"),
            serde_json::json!({})
        )
        .await
        .status(),
        StatusCode::OK
    );
    let (after_read, _) =
        read_pages(&app, "/v1/users/me/notifications?limit=50", "cursor", None).await;
    assert_eq!(after_read, expected_ids);
    let unread = common::json_body(
        app.get_with_session("/v1/users/me/notifications?unread=true&limit=100")
            .await,
    )
    .await;
    assert_eq!(unread["unread_count"].as_u64(), Some(119));
    assert!(!ids(&unread).contains(middle));

    // 拒否系: 未知の kind / cursor と after の同時指定 / 壊れたカーソル
    for bad in [
        "/v1/users/me/notifications?kind=bogus".to_string(),
        format!("/v1/users/me/notifications?cursor={anchor}&after={anchor}"),
        "/v1/users/me/notifications?cursor=!!!!".to_string(),
        "/v1/users/me/notifications?after=bm90IGpzb24".to_string(),
    ] {
        assert_eq!(
            app.get_with_session(&bad).await.status(),
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
}

/// 保持期間より古い行だけを消す。メール送信待ちの行は古くても残す。
#[tokio::test]
async fn retention_deletes_old_rows_but_keeps_pending_emails() {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};

    let app = TestApp::new().await;
    let user = app.insert_user_default().await;
    let tp = app.insert_tenant_project(user.id).await;
    let db = &app.state.db;
    let now = chrono::Utc::now();
    let old_at = now - chrono::Duration::days(91);

    let old = insert_notification_at(&app, user.id, tp.project_id, "assigned", old_at).await;
    let young = insert_notification_at(
        &app,
        user.id,
        tp.project_id,
        "assigned",
        now - chrono::Duration::days(89),
    )
    .await;
    // メールの状態を変えた古い行: (送信待ち, 送信済み, 諦めた)
    let mut with_email = Vec::new();
    for (emailed, attempts) in [
        (false, 0),
        (true, 1),
        (false, job::notification_email::MAX_ATTEMPTS),
    ] {
        let id = insert_notification_at(&app, user.id, tp.project_id, "assigned", old_at).await;
        let mut row: entity::notifications::ActiveModel =
            entity::notifications::Entity::find_by_id(id)
                .one(db)
                .await
                .expect("find")
                .expect("row")
                .into();
        row.email_queued_at = Set(Some(old_at.into()));
        row.emailed_at = Set(emailed.then(|| now.into()));
        row.email_attempts = Set(attempts);
        row.update(db).await.expect("update");
        with_email.push(id);
    }

    job::notification_retention::delete_older_than(db, now - chrono::Duration::days(90))
        .await
        .expect("retention");

    let exists = |id: uuid::Uuid| async move {
        entity::notifications::Entity::find_by_id(id)
            .one(db)
            .await
            .expect("find")
            .is_some()
    };
    assert!(!exists(old).await, "古い行は消す");
    assert!(exists(young).await, "期間内は残す");
    assert!(exists(with_email[0]).await, "送信待ちは古くても残す");
    assert!(!exists(with_email[1]).await, "送信済みは消す");
    assert!(!exists(with_email[2]).await, "送信を諦めた行は消す");
}
