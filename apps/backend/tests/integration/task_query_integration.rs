use crate::common::{TestApp, TestTenantProject};
use axum::http::StatusCode;
use serde_json::{Value, json};

fn base(tp: &TestTenantProject) -> String {
    format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    )
}
async fn fixture() -> (TestApp, TestTenantProject, String) {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tp = app.insert_tenant_project(user.id).await;
    let response = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/statuses",
                tp.tenant_id, tp.project_id
            ),
            json!({"name":"In Progress", "color":"#336699", "position":1, "is_default":true}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let status: Value = response.json().await.unwrap();
    (app, tp, status["id"].as_str().unwrap().to_string())
}
async fn task(app: &TestApp, tp: &TestTenantProject, status: &str, title: &str) -> String {
    let response = app
        .post_json_with_session(&base(tp), json!({"title":title,"status_id":status}))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}
#[tokio::test]
async fn task_query_comment_matches_and_text_and() {
    let (app, tp, status) = fixture().await;
    let id = task(&app, &tp, &status, "通知の設定").await;
    task(&app, &tp, &status, "無関係").await;
    for i in 0..31 {
        let response = app
            .post_json_with_session(
                &format!("{}/{id}/comments", base(&tp)),
                json!({"body":format!("配送確認 {i}" )}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let expression = urlencoding::encode("通知 配送確認");
    let response = app
        .get_with_session(&format!("{}?q={expression}", base(&tp)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["total"], 1);
    assert_eq!(body["tasks"][0]["id"], id);
    let found = &body["matches"][&id];
    assert!(
        found["title"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["hit"] == true)
    );
    assert_eq!(found["snippets"][0]["source"], "comment");
    assert!(found["snippets"][0]["comment"]["author"]["id"].is_string());
    let segments = found["snippets"][0]["segments"].as_array().unwrap();
    assert!(
        segments
            .iter()
            .map(|s| s["text"].as_str().unwrap())
            .collect::<String>()
            .ends_with("30")
    );
}
#[tokio::test]
async fn task_query_rejects_invalid_and_accepts_valid_conditions() {
    let (app, tp, status) = fixture().await;
    task(&app, &tp, &status, "通知").await;
    for (q, code) in [
        ("status:無い名前", "unknown-value"),
        ("priority:hgih", "invalid-value"),
        ("label:", "empty-value"),
        ("-sort:title", "not-negatable"),
    ] {
        let response = app
            .get_with_session(&format!("{}?q={}", base(&tp), urlencoding::encode(q)))
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{q}");
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["message"], "invalid-task-query");
        assert_eq!(body["errors"][0]["code"], code);
    }
    let response = app
        .get_with_session(&format!(
            "{}?q={}",
            base(&tp),
            urlencoding::encode("status:\"In Progress\" 通知")
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["total"], 1);
}

#[tokio::test]
async fn task_query_filters_facets_scopes_and_pagination() {
    let (mut app, tp, status) = fixture().await;
    let first = task(&app, &tp, &status, "通知 alpha").await;
    let second = task(&app, &tp, &status, "通知 beta").await;
    let third = task(&app, &tp, &status, "別のタスク").await;
    let labels_path = format!(
        "/v1/tenants/{}/projects/{}/labels",
        tp.tenant_id, tp.project_id
    );
    let mut labels = Vec::new();
    for name in ["bug", "feature", "保留"] {
        let r = app
            .post_json_with_session(&labels_path, json!({"name":name,"color":"#336699"}))
            .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let b: Value = r.json().await.unwrap();
        labels.push(b["id"].as_str().unwrap().to_string());
    }
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    use uuid::Uuid;
    for (id, indices) in [(&first, vec![0, 1]), (&second, vec![1, 2])] {
        for index in indices {
            entity::task_labels::ActiveModel {
                task_id: Set(id.parse::<Uuid>().unwrap()),
                label_id: Set(labels[index].parse().unwrap()),
            }
            .insert(&app.state.db)
            .await
            .unwrap();
        }
    }
    let r = app
        .post_json_with_session(
            &format!("{}/{third}/comments", base(&tp)),
            json!({"body":"コメントのみ"}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    for (expr, total) in [
        ("label:bug,feature", 2),
        ("label:bug label:feature", 1),
        ("status:\"In Progress\" -label:保留 通知", 1),
        ("no:label", 1),
        ("-no:label", 2),
        ("no:assignee", 3),
        ("-no:assignee", 0),
        ("assignee:@me", 0),
        ("author:@me", 3),
        ("in:title コメントのみ", 0),
        ("in:comment コメントのみ", 1),
        ("in:comment 通知", 0),
        ("is:subtask", 0),
        ("is:archived", 0),
    ] {
        let r = app
            .get_with_session(&format!("{}?q={}", base(&tp), urlencoding::encode(expr)))
            .await;
        assert_eq!(r.status(), StatusCode::OK, "{expr}");
        let b: Value = r.json().await.unwrap();
        assert_eq!(b["total"], total, "{expr}");
    }
    for sort in [
        "priority",
        "updated",
        "key",
        "key-desc",
        "status",
        "status-desc",
    ] {
        let mut cursor = None;
        let mut seen = std::collections::HashSet::new();
        loop {
            let mut path = format!("{}?q=sort:{sort}&limit=1", base(&tp));
            if let Some(c) = cursor {
                path.push_str(&format!("&cursor={c}"));
            }
            let r = app.get_with_session(&path).await;
            assert_eq!(r.status(), StatusCode::OK, "{sort}");
            let b: Value = r.json().await.unwrap();
            for task in b["tasks"].as_array().unwrap() {
                assert!(
                    seen.insert(task["id"].as_str().unwrap().to_string()),
                    "duplicate for {sort}"
                );
            }
            cursor = b["next_cursor"].as_str().map(str::to_string);
            if cursor.is_none() {
                break;
            }
            assert!(seen.len() <= 3);
        }
        assert_eq!(seen.len(), 3, "{sort}");
    }
    let r = app
        .get_with_session(&format!("{}?q=sort:title&sort=title_asc", base(&tp)))
        .await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let b: Value = r.json().await.unwrap();
    assert_eq!(b["errors"][0]["code"], "conflicting-sort");
    let r = app.get_with_session(&format!("{}/facets", base(&tp))).await;
    assert_eq!(r.status(), StatusCode::OK);
    let b: Value = r.json().await.unwrap();
    assert_eq!(b["unassigned"], 3);
    assert_eq!(b["unlabeled"], 1);
    assert_eq!(b["statuses"][0]["count"], 3);
    let outsider = app.insert_user_default().await;
    app.reset_session_client();
    app.login_session_no_content(&outsider.email, &outsider.password)
        .await;
    let r = app.get_with_session(&format!("{}/facets", base(&tp))).await;
    assert!(matches!(
        r.status(),
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
}

#[tokio::test]
async fn task_query_deadlines_archived_subtasks_and_comment_boundaries() {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
    use uuid::Uuid;
    let (app, tp, status) = fixture().await;
    let mut ids = Vec::new();
    for (name, days) in [("today", 0), ("six", 6), ("seven", 7), ("overdue", -1)] {
        let id = task(&app, &tp, &status, name).await;
        let model = entity::tasks::Entity::find_by_id(id.parse::<Uuid>().unwrap())
            .one(&app.state.db)
            .await
            .unwrap()
            .unwrap();
        let mut active: entity::tasks::ActiveModel = model.into();
        let date = chrono::Utc::now().date_naive() + chrono::Duration::days(days);
        active.soft_deadline = Set(Some(date.and_hms_opt(0, 0, 0).unwrap().and_utc().into()));
        active.update(&app.state.db).await.unwrap();
        ids.push(id);
    }
    let parent = task(&app, &tp, &status, "parent").await;
    let child = task(&app, &tp, &status, "child 通知").await;
    let mut model: entity::tasks::ActiveModel =
        entity::tasks::Entity::find_by_id(child.parse::<Uuid>().unwrap())
            .one(&app.state.db)
            .await
            .unwrap()
            .unwrap()
            .into();
    model.parent_task_id = Set(Some(parent.parse().unwrap()));
    model.update(&app.state.db).await.unwrap();
    let archived = task(&app, &tp, &status, "archived").await;
    let mut model: entity::tasks::ActiveModel =
        entity::tasks::Entity::find_by_id(archived.parse::<Uuid>().unwrap())
            .one(&app.state.db)
            .await
            .unwrap()
            .unwrap()
            .into();
    model.is_archived = Set(true);
    model.update(&app.state.db).await.unwrap();
    let deleted_comment = app
        .post_json_with_session(
            &format!("{}/{parent}/comments", base(&tp)),
            json!({"body":"deleted-only"}),
        )
        .await;
    assert_eq!(deleted_comment.status(), StatusCode::CREATED);
    let c: Value = deleted_comment.json().await.unwrap();
    let mut model: entity::task_comments::ActiveModel = entity::task_comments::Entity::find_by_id(
        c["id"].as_str().unwrap().parse::<Uuid>().unwrap(),
    )
    .one(&app.state.db)
    .await
    .unwrap()
    .unwrap()
    .into();
    model.deleted_at = Set(Some(chrono::Utc::now().into()));
    model.update(&app.state.db).await.unwrap();
    for (expr, total) in [
        ("due:<7d", 2),
        ("due:<1d", 1),
        ("due:overdue", 1),
        ("is:archived", 1),
        ("-is:archived", 6),
        ("is:subtask", 1),
        ("deleted-only", 0),
    ] {
        let r = app
            .get_with_session(&format!(
                "{}?timezone=UTC&q={}",
                base(&tp),
                urlencoding::encode(expr)
            ))
            .await;
        assert_eq!(r.status(), StatusCode::OK, "{expr}");
        let b: Value = r.json().await.unwrap();
        assert_eq!(b["total"], total, "{expr}");
    }
    let r = app
        .get_with_session(&format!(
            "{}?root_only=true&q={}",
            base(&tp),
            urlencoding::encode("child")
        ))
        .await;
    let b: Value = r.json().await.unwrap();
    assert_eq!(b["tasks"][0]["id"], child);
    // Mark the only overdue task as done: overdue must now be empty.
    let done_path = format!(
        "/v1/tenants/{}/projects/{}/statuses",
        tp.tenant_id, tp.project_id
    );
    let r = app
        .post_json_with_session(
            &done_path,
            json!({"name":"Done","color":"#336699","position":2,"is_done_state":true}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let done: Value = r.json().await.unwrap();
    let mut model: entity::tasks::ActiveModel =
        entity::tasks::Entity::find_by_id(ids[3].parse::<Uuid>().unwrap())
            .one(&app.state.db)
            .await
            .unwrap()
            .unwrap()
            .into();
    model.status_id = Set(done["id"].as_str().unwrap().parse().unwrap());
    model.update(&app.state.db).await.unwrap();
    let r = app
        .get_with_session(&format!("{}?q=due:overdue", base(&tp)))
        .await;
    let b: Value = r.json().await.unwrap();
    assert_eq!(b["total"], 0);
    let r = app
        .get_with_session(&format!("{}?timezone=not-a-timezone", base(&tp)))
        .await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}
