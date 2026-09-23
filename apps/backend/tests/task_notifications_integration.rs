mod common;

use axum::http::StatusCode;
use backend::utils::notifications::{
    NewNotification, NotificationTarget, create_notification, delete_older_than,
};
use common::TestApp;
use entity::{notifications, tenant_members};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
};
use serde_json::Value;
use uuid::Uuid;

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

    common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, assignee.id).await;
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
    common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, assignee.id).await;
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
    common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, watcher.id).await;
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
    common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
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

/// 通知を DB へ直に挿す。作成時刻を揃えてカーソルのタイブレーカーを試すため、
/// 生成経路（`create_notification` は現在時刻を使う）を通さない。
async fn insert_notification(
    app: &TestApp,
    user_id: Uuid,
    project_id: Uuid,
    notification_type: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> Uuid {
    let id = Uuid::new_v4();
    notifications::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        task_id: Set(None),
        notification_type: Set(notification_type.to_string()),
        payload: Set(serde_json::json!({})),
        read_at: Set(None),
        created_at: Set(created_at.into()),
        project_id: Set(project_id),
        target: Set(serde_json::json!({"type": "task", "task_id": Uuid::new_v4()})),
        dedupe_key: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert notification");
    id
}

fn ids(page: &Value) -> Vec<String> {
    page["notifications"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect()
}

/// `path` を先頭に `key=next_cursor` で最後まで読み、(id 列, 各ページの件数) を返す
async fn read_pages(app: &TestApp, path: &str, key: &str) -> (Vec<String>, Vec<usize>) {
    let mut got = Vec::new();
    let mut sizes = Vec::new();
    let mut next: Option<String> = None;
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
    let user = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(user.id).await;
    app.login_session_no_content(&user.email, &user.password)
        .await;

    // 上限 100 を越える件数。3 件ずつ同時刻にして id のタイブレーカーを通す
    let base = chrono::Utc::now() - chrono::Duration::hours(1);
    let mut expected = Vec::new();
    for i in 0..120i64 {
        let at = base + chrono::Duration::seconds(i / 3);
        let kind = if i % 4 == 0 {
            "review.round_created"
        } else {
            "assigned"
        };
        let id = insert_notification(&app, user.id, tp.project_id, kind, at).await;
        expected.push((at, id, kind));
    }
    // created_at DESC, id DESC
    expected.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    let expected_ids: Vec<String> = expected.iter().map(|(_, id, _)| id.to_string()).collect();

    // cursor で欠落・重複なく最後まで読める
    let (got, sizes) = read_pages(&app, "/v1/users/me/notifications?limit=50", "cursor").await;
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

    // 各行の project と target
    let row = &first["notifications"][0];
    assert_eq!(row["project"]["id"], tp.project_id.to_string());
    assert_eq!(row["project"]["tenant_id"], tp.tenant_id.to_string());
    assert!(row["project"]["key"].is_string());
    assert_eq!(row["target"]["type"], "task");

    // after: 30 番目に新しい行より新しい 29 件だけを、古い順に next_cursor で取り切る
    let anchor = first["notifications"][29]["cursor"].as_str().unwrap();
    let (newer, sizes) = read_pages(
        &app,
        &format!("/v1/users/me/notifications?limit=10&after={anchor}"),
        "after",
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
    assert_eq!(ids(&none).len(), 0);
    assert!(none["next_cursor"].is_null());

    // kind: review. 接頭辞で分ける
    let review = common::json_body(
        app.get_with_session("/v1/users/me/notifications?kind=review&limit=100")
            .await,
    )
    .await;
    let review_ids = ids(&review);
    assert_eq!(review_ids.len(), 30);
    let want_review: Vec<String> = expected
        .iter()
        .filter(|(_, _, k)| k.starts_with("review."))
        .map(|(_, id, _)| id.to_string())
        .collect();
    assert_eq!(review_ids, want_review);
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
    let (after_read, _) = read_pages(&app, "/v1/users/me/notifications?limit=50", "cursor").await;
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

#[tokio::test]
async fn notifications_hidden_after_losing_project_access() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let member = app.insert_user(false, false).await;
    // メンバー未指定のプロジェクトはテナントメンバー全員が入れる
    common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    // 対照: 自分がオーナーのプロジェクトはアクセスを保つ
    let own = app.insert_tenant_project(member.id).await;

    let now = chrono::Utc::now();
    let lost = insert_notification(&app, member.id, tp.project_id, "assigned", now).await;
    let kept = insert_notification(&app, member.id, own.project_id, "assigned", now).await;

    app.login_session_no_content(&member.email, &member.password)
        .await;
    let before = common::json_body(app.get_with_session("/v1/users/me/notifications").await).await;
    assert_eq!(before["unread_count"].as_u64(), Some(2));
    assert_eq!(ids(&before).len(), 2);

    tenant_members::Entity::delete_many()
        .filter(tenant_members::Column::TenantId.eq(tp.tenant_id))
        .filter(tenant_members::Column::UserId.eq(member.id))
        .exec(&app.state.db)
        .await
        .expect("remove tenant member");

    let after = common::json_body(app.get_with_session("/v1/users/me/notifications").await).await;
    assert_eq!(after["unread_count"].as_u64(), Some(1));
    assert_eq!(ids(&after), vec![kept.to_string()]);
    assert_eq!(
        app.patch_json_with_session(
            &format!("/v1/users/me/notifications/{lost}/read"),
            serde_json::json!({})
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.patch_json_with_session("/v1/users/me/notifications/read-all", serde_json::json!({}))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let lost_row = notifications::Entity::find_by_id(lost)
        .one(&app.state.db)
        .await
        .expect("find")
        .expect("row remains");
    assert!(lost_row.read_at.is_none(), "見えない通知は既読化しない");

    // 他人の通知は 404、本人なら 200
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    assert_eq!(
        app.patch_json_with_session(
            &format!("/v1/users/me/notifications/{kept}/read"),
            serde_json::json!({})
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    app.reset_session_client();
    app.login_session_no_content(&member.email, &member.password)
        .await;
    assert_eq!(
        app.patch_json_with_session(
            &format!("/v1/users/me/notifications/{kept}/read"),
            serde_json::json!({})
        )
        .await
        .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn dedupe_key_and_retention() {
    let app = TestApp::new().await;
    let user = app.insert_user(false, false).await;
    let other = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(user.id).await;
    let db = &app.state.db;

    let new = |user_id: Uuid, key: Option<&str>| NewNotification {
        user_id,
        project_id: tp.project_id,
        task_id: None,
        notification_type: "assigned",
        payload: serde_json::json!({}),
        target: NotificationTarget::Task {
            task_id: Uuid::new_v4(),
        },
        dedupe_key: key.map(String::from),
    };
    let count = |user_id: Uuid| async move {
        notifications::Entity::find()
            .filter(notifications::Column::UserId.eq(user_id))
            .count(db)
            .await
            .expect("count")
    };

    create_notification(db, new(user.id, Some("k1")))
        .await
        .expect("1st");
    create_notification(db, new(user.id, Some("k1")))
        .await
        .expect("2nd");
    assert_eq!(count(user.id).await, 1, "同じキーの 2 件目は作らない");
    // 対照: 別のキー・別の受信者・キー無しは重ならない
    create_notification(db, new(user.id, Some("k2")))
        .await
        .expect("k2");
    create_notification(db, new(other.id, Some("k1")))
        .await
        .expect("other");
    create_notification(db, new(user.id, None))
        .await
        .expect("none 1");
    create_notification(db, new(user.id, None))
        .await
        .expect("none 2");
    assert_eq!(count(user.id).await, 4);
    assert_eq!(count(other.id).await, 1);

    // Retention: 境界より古い行だけを消す
    let now = chrono::Utc::now();
    let old = insert_notification(
        &app,
        other.id,
        tp.project_id,
        "assigned",
        now - chrono::Duration::days(91),
    )
    .await;
    let young = insert_notification(
        &app,
        other.id,
        tp.project_id,
        "assigned",
        now - chrono::Duration::days(89),
    )
    .await;
    delete_older_than(db, now - chrono::Duration::days(90))
        .await
        .expect("retention");
    assert!(
        notifications::Entity::find_by_id(old)
            .one(db)
            .await
            .expect("find")
            .is_none()
    );
    assert!(
        notifications::Entity::find_by_id(young)
            .one(db)
            .await
            .expect("find")
            .is_some()
    );
}
