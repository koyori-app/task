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

async fn query(app: &TestApp, tp: &TestTenantProject, expr: &str) -> Value {
    let r = app
        .get_with_session(&format!("{}?q={}", base(tp), urlencoding::encode(expr)))
        .await;
    assert_eq!(r.status(), StatusCode::OK, "{expr}");
    r.json().await.unwrap()
}
fn ids(body: &Value) -> Vec<String> {
    body["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect()
}
async fn add_status(app: &TestApp, tp: &TestTenantProject, name: &str, position: i32) -> String {
    let r = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/statuses",
                tp.tenant_id, tp.project_id
            ),
            json!({"name":name,"color":"#336699","position":position}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let b: Value = r.json().await.unwrap();
    b["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn task_query_treats_like_wildcards_as_literal_characters() {
    let (app, tp, status) = fixture().await;
    let percent = task(&app, &tp, &status, "達成率 100% です").await;
    let plain = task(&app, &tp, &status, "達成率 1000 です").await;
    let underscore = task(&app, &tp, &status, "snake_case").await;
    let other = task(&app, &tp, &status, "snakeXcase").await;
    let backslash = task(&app, &tp, &status, r"C:\temp").await;
    let slash = task(&app, &tp, &status, "C:/temp").await;
    for (expr, expected, excluded) in [
        ("100%", &percent, &plain),
        ("snake_case", &underscore, &other),
        (r#""C:\\temp""#, &backslash, &slash),
    ] {
        let found = ids(&query(&app, &tp, expr).await);
        assert_eq!(found, std::slice::from_ref(expected), "{expr}");
        assert!(!found.contains(excluded), "{expr}");
    }
    // `%` だけの語は全件ではなく、`%` を含むものだけに当たる
    assert_eq!(ids(&query(&app, &tp, "%").await), [percent]);
}

#[tokio::test]
async fn task_query_at_name_matches_comment_markers_inside_the_tenant_only() {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
    use uuid::Uuid;
    let (app, tp, status) = fixture().await;
    let name = format!("mina{}", &Uuid::new_v4().simple().to_string()[..8]);
    let member = app.insert_user(false, false).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    let outsider = app.insert_user(false, false).await;
    for (user, username) in [(&member, name.clone()), (&outsider, format!("{name}x"))] {
        let mut model: entity::users::ActiveModel = entity::users::Entity::find_by_id(user.id)
            .one(&app.state.db)
            .await
            .unwrap()
            .unwrap()
            .into();
        model.username = Set(username);
        model.update(&app.state.db).await.unwrap();
    }
    let by_member = task(&app, &tp, &status, "メンバーへのコメント").await;
    let by_outsider = task(&app, &tp, &status, "外部の人へのコメント").await;
    for (id, user) in [(&by_member, &member), (&by_outsider, &outsider)] {
        let r = app
            .post_json_with_session(
                &format!("{}/{id}/comments", base(&tp)),
                json!({"body":format!("確認お願いします <@{}>", user.id)}),
            )
            .await;
        assert_eq!(r.status(), StatusCode::CREATED);
    }
    // 別のプロジェクトの同じ名前のコメントは探さない
    let other_project = crate::common::insert_extra_project(&app, tp.tenant_id).await;
    let other_base = format!(
        "/v1/tenants/{}/projects/{other_project}/tasks",
        tp.tenant_id
    );
    let r = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{other_project}/statuses",
                tp.tenant_id
            ),
            json!({"name":"Todo","color":"#336699","position":0,"is_default":true}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let other_status: Value = r.json().await.unwrap();
    let r = app
        .post_json_with_session(
            &other_base,
            json!({"title":"別プロジェクト","status_id":other_status["id"]}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let foreign: Value = r.json().await.unwrap();
    let r = app
        .post_json_with_session(
            &format!("{other_base}/{}/comments", foreign["id"].as_str().unwrap()),
            json!({"body":format!("確認お願いします <@{}>", member.id)}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let body = query(&app, &tp, &format!("@{name}")).await;
    assert_eq!(ids(&body), std::slice::from_ref(&by_member));
    assert_eq!(
        body["matches"][&by_member]["snippets"][0]["source"],
        "comment"
    );
    // テナントに属さない人の名前では、印が付いたコメントにも当たらない
    assert!(ids(&query(&app, &tp, &format!("@{name}x")).await).is_empty());
    let other = query_in(&app, &other_base, &format!("@{name}")).await;
    assert_eq!(other["total"], 1);
}
async fn query_in(app: &TestApp, base: &str, expr: &str) -> Value {
    let r = app
        .get_with_session(&format!("{base}?q={}", urlencoding::encode(expr)))
        .await;
    assert_eq!(r.status(), StatusCode::OK, "{expr}");
    r.json().await.unwrap()
}

#[tokio::test]
async fn task_query_sorts_by_status_position_and_other_fields() {
    let (app, tp, in_progress) = fixture().await;
    let backlog = add_status(&app, &tp, "Backlog", 0).await;
    let done = add_status(&app, &tp, "Done", 2).await;
    let d1 = task(&app, &tp, &done, "b").await;
    let p1 = task(&app, &tp, &in_progress, "c").await;
    let b1 = task(&app, &tp, &backlog, "a").await;
    let d2 = task(&app, &tp, &done, "d").await;
    let p2 = task(&app, &tp, &in_progress, "e").await;
    // ステータスの並びは position 順。同じステータスの中は id で決まるので、塊の並びだけを比べる
    let group = |id: &String| -> usize {
        if id == &b1 {
            0
        } else if id == &p1 || id == &p2 {
            1
        } else {
            2
        }
    };
    for (expr, expected) in [
        ("sort:status", [0, 1, 1, 2, 2]),
        ("sort:status-asc", [0, 1, 1, 2, 2]),
        ("sort:status-desc", [2, 2, 1, 1, 0]),
    ] {
        let found = ids(&query(&app, &tp, expr).await);
        assert_eq!(
            found.iter().map(group).collect::<Vec<_>>(),
            expected,
            "{expr}"
        );
    }
    for (expr, expected) in [
        ("sort:title", [&b1, &d1, &p1, &d2, &p2]),
        ("sort:title-desc", [&p2, &d2, &p1, &d1, &b1]),
        ("sort:key", [&d1, &p1, &b1, &d2, &p2]),
        ("sort:key-desc", [&p2, &d2, &b1, &p1, &d1]),
    ] {
        let found = ids(&query(&app, &tp, expr).await);
        assert_eq!(found, expected.map(String::clone), "{expr}");
    }
}

#[tokio::test]
async fn task_query_assignee_me_and_facet_counts() {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
    use uuid::Uuid;
    let (app, tp, status) = fixture().await;
    let done = add_status(&app, &tp, "Done", 2).await;
    let mine = task(&app, &tp, &status, "自分").await;
    let both = task(&app, &tp, &done, "二人").await;
    let nobody = task(&app, &tp, &status, "担当なし").await;
    let archived = task(&app, &tp, &status, "アーカイブ済み").await;
    let member = app.insert_user(false, false).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    // ログイン中の利用者は fixture の持ち主。作成者の内訳は DB で書き換えて作る
    let owner = entity::tenants::Entity::find_by_id(tp.tenant_id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap()
        .owner_id;
    let label = {
        let r = app
            .post_json_with_session(
                &format!(
                    "/v1/tenants/{}/projects/{}/labels",
                    tp.tenant_id, tp.project_id
                ),
                json!({"name":"bug","color":"#336699"}),
            )
            .await;
        assert_eq!(r.status(), StatusCode::CREATED);
        let b: Value = r.json().await.unwrap();
        b["id"].as_str().unwrap().parse::<Uuid>().unwrap()
    };
    for (id, user) in [
        (&mine, owner),
        (&both, owner),
        (&both, member.id),
        (&archived, owner),
    ] {
        entity::task_assignees::ActiveModel {
            id: Set(Uuid::new_v4()),
            task_id: Set(id.parse().unwrap()),
            user_id: Set(user),
            role: Set("assignee".into()),
            assigned_at: Set(chrono::Utc::now().into()),
            completed_at: Set(None),
        }
        .insert(&app.state.db)
        .await
        .unwrap();
    }
    for id in [&mine, &archived] {
        entity::task_labels::ActiveModel {
            task_id: Set(id.parse().unwrap()),
            label_id: Set(label),
        }
        .insert(&app.state.db)
        .await
        .unwrap();
    }
    for (id, priority, created_by, is_archived) in [
        (&mine, entity::tasks::TaskPriority::High, None, false),
        (
            &both,
            entity::tasks::TaskPriority::High,
            Some(member.id),
            false,
        ),
        (&nobody, entity::tasks::TaskPriority::Low, None, false),
        (&archived, entity::tasks::TaskPriority::Critical, None, true),
    ] {
        let mut model: entity::tasks::ActiveModel =
            entity::tasks::Entity::find_by_id(id.parse::<Uuid>().unwrap())
                .one(&app.state.db)
                .await
                .unwrap()
                .unwrap()
                .into();
        model.priority = Set(priority);
        if let Some(user) = created_by {
            model.created_by = Set(user);
        }
        model.is_archived = Set(is_archived);
        model.update(&app.state.db).await.unwrap();
    }
    // 成功側: 自分が担当のタスクだけ（アーカイブ済みは既定で出ない）
    let mut found = ids(&query(&app, &tp, "assignee:@me").await);
    found.sort();
    let mut expected = vec![mine.clone(), both.clone()];
    expected.sort();
    assert_eq!(found, expected);
    assert_eq!(
        ids(&query(&app, &tp, "-assignee:@me").await),
        std::slice::from_ref(&nobody)
    );
    assert_eq!(
        ids(&query(&app, &tp, "assignee:@me is:archived").await),
        std::slice::from_ref(&archived)
    );
    // `is:archived -is:archived` は互いに矛盾するので 0 件（後の条件で上書きしない）
    assert_eq!(
        query(&app, &tp, "is:archived -is:archived").await["total"],
        0
    );
    assert_eq!(
        query(&app, &tp, "-is:archived is:archived").await["total"],
        0
    );

    let r = app.get_with_session(&format!("{}/facets", base(&tp))).await;
    assert_eq!(r.status(), StatusCode::OK);
    let b: Value = r.json().await.unwrap();
    let count = |list: &Value, key: &str, id: &str| -> Option<u64> {
        list.as_array()
            .unwrap()
            .iter()
            .find(|e| e[key] == id)
            .map(|e| e["count"].as_u64().unwrap())
    };
    // アーカイブ済み（mine と同じラベル・担当・優先度 critical）は数えない
    assert_eq!(count(&b["statuses"], "id", &status), Some(2));
    assert_eq!(count(&b["statuses"], "id", &done), Some(1));
    assert_eq!(count(&b["labels"], "id", &label.to_string()), Some(1));
    assert_eq!(count(&b["assignees"], "id", &owner.to_string()), Some(2));
    assert_eq!(
        count(&b["assignees"], "id", &member.id.to_string()),
        Some(1)
    );
    assert_eq!(count(&b["authors"], "id", &member.id.to_string()), Some(1));
    assert_eq!(count(&b["authors"], "id", &owner.to_string()), Some(2));
    assert_eq!(count(&b["priorities"], "priority", "High"), Some(2));
    assert_eq!(count(&b["priorities"], "priority", "Low"), Some(1));
    assert_eq!(count(&b["priorities"], "priority", "Critical"), None);
    assert_eq!(b["unassigned"], 1);
    assert_eq!(b["unlabeled"], 2);
}
#[tokio::test]
async fn task_query_words_do_not_match_inside_mention_markers() {
    let (app, tp, status) = fixture().await;
    let member = app.insert_user(false, false).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    // 16 進の文字だけの語。メンションの印 `<@UUID>` の中の UUID の先頭 8 字と同じ字になる（ハイフンをまたがない）。
    let fragment = member.id.simple().to_string()[..8].to_string();
    let in_comment = task(&app, &tp, &status, "コメントに印").await;
    let r = app
        .post_json_with_session(
            &format!("{}/{in_comment}/comments", base(&tp)),
            json!({"body":format!("確認お願いします <@{}>", member.id)}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let r = app
        .post_json_with_session(
            &base(&tp),
            json!({"title":"説明に印","status_id":status,"description":format!("担当は <@{}> です", member.id)}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    // 対照: 同じ語が本文に文字として入っていれば当たる
    let word_in_comment = task(&app, &tp, &status, "コメントに語").await;
    let r = app
        .post_json_with_session(
            &format!("{}/{word_in_comment}/comments", base(&tp)),
            json!({"body":format!("メモ {fragment} を見る")}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let r = app
        .post_json_with_session(
            &base(&tp),
            json!({"title":"説明に語","status_id":status,"description":format!("メモ {fragment} を見る")}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let word_in_description: Value = r.json().await.unwrap();
    let body = query(&app, &tp, &fragment).await;
    let mut found = ids(&body);
    found.sort();
    let mut expected = vec![
        word_in_comment.clone(),
        word_in_description["id"].as_str().unwrap().to_string(),
    ];
    expected.sort();
    assert_eq!(found, expected, "印の中の UUID には当たらない");
    for id in &found {
        let snippets = &body["matches"][id]["snippets"];
        assert!(
            snippets.as_array().is_some_and(|s| !s.is_empty()),
            "当たった行には抜粋がある"
        );
    }
    // 印そのものに当てる `@名` の経路は残る
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
    let name = format!("mina{}", &fragment[..8]);
    let mut model: entity::users::ActiveModel = entity::users::Entity::find_by_id(member.id)
        .one(&app.state.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    model.username = Set(name.clone());
    model.update(&app.state.db).await.unwrap();
    let body = query(&app, &tp, &format!("@{name}")).await;
    assert!(ids(&body).contains(&in_comment));
}
#[tokio::test]
async fn task_query_words_do_not_match_across_mention_markers() {
    let (app, tp, status) = fixture().await;
    let member = app.insert_user(false, false).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    let in_comment = task(&app, &tp, &status, "コメントに印").await;
    let r = app
        .post_json_with_session(
            &format!("{}/{in_comment}/comments", base(&tp)),
            json!({"body":format!("確認<@{}>お願いします", member.id)}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let r = app
        .post_json_with_session(
            &base(&tp),
            json!({"title":"説明に印","status_id":status,"description":format!("確認<@{}>お願いします", member.id)}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let in_description = r.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut both = vec![in_comment.clone(), in_description.clone()];
    both.sort();
    // 印をまたぐ語は、印を消してつないだ文字列には当たらない
    let body = query(&app, &tp, "確認お願い").await;
    assert!(ids(&body).is_empty(), "印をまたいで当たらない: {body}");
    // 除外でも、本文に無い語のせいで落ちない
    let body = query(&app, &tp, "-確認お願い").await;
    let mut found = ids(&body);
    found.sort();
    assert_eq!(found, both, "除外が印をまたいで当たらない");
    // 対照: 印の片側の語には当たり、抜粋も出る
    for word in ["確認", "お願い"] {
        let body = query(&app, &tp, word).await;
        let mut found = ids(&body);
        found.sort();
        assert_eq!(found, both, "{word} は印の片側の語として当たる");
        for id in &found {
            assert!(
                body["matches"][id]["snippets"]
                    .as_array()
                    .is_some_and(|s| !s.is_empty()),
                "{word}: 当たった行には抜粋がある"
            );
        }
    }
}
