mod common;

use axum::http::StatusCode;
use common::TestApp;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use serde_json::{Value, json};
use uuid::Uuid;

// 本体だけでなく、履歴・通知・ウォッチャーもトランザクションに含まれることを検証する。
async fn snapshot(app: &TestApp, task_id: Uuid) -> Value {
    app.state.db.query_one_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT jsonb_build_object(
            'task', (SELECT to_jsonb(t) FROM tasks t WHERE id = $1),
            'labels', (SELECT jsonb_agg(to_jsonb(l) ORDER BY label_id) FROM task_labels l WHERE task_id = $1),
            'assignees', (SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM task_assignees a WHERE task_id = $1),
            'activities', (SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM task_activities a WHERE task_id = $1),
            'notifications', (SELECT jsonb_agg(to_jsonb(n) ORDER BY id) FROM notifications n WHERE task_id = $1),
            'watchers', (SELECT jsonb_agg(to_jsonb(w) ORDER BY user_id) FROM task_watchers w WHERE task_id = $1)
        ) AS state",
        [task_id.into()],
    )).await.unwrap().unwrap().try_get("", "state").unwrap()
}

#[tokio::test]
async fn task_update_commits_or_rolls_back_fields_labels_and_assignees_together() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    let member = app.insert_user(false, false).await;
    let outsider = app.insert_user(false, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let project_path = format!("/v1/tenants/{}/projects/{}", tp.tenant_id, tp.project_id);
    let response = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/members", tp.tenant_id),
            json!({ "user_id": member.id, "role": "Member" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = app
        .post_json_with_session(
            &format!("{project_path}/statuses"),
            json!({
                "name": "Todo", "color": "#336699", "position": 0, "is_default": true,
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let status: Value = response.json().await.unwrap();
    let mut labels = vec![];
    for name in ["old", "new", "keep"] {
        let response = app
            .post_json_with_session(
                &format!("{project_path}/labels"),
                json!({
                    "name": name, "color": "#336699",
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let label: Value = response.json().await.unwrap();
        labels.push(label["id"].clone());
    }
    let response = app
        .post_json_with_session(
            &format!("{project_path}/tasks"),
            json!({
                "title": "original", "description": "before", "status_id": status["id"],
                "soft_deadline": "2026-09-01T00:00:00Z", "hard_deadline": "2026-09-10T00:00:00Z",
                "label_ids": [labels[0], labels[2]],
                "assignees": [{ "user_id": owner.id, "role": "reviewer" }],
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: Value = response.json().await.unwrap();
    let task_id: Uuid = created["id"].as_str().unwrap().parse().unwrap();
    let task_path = format!("{project_path}/tasks/{task_id}");
    let before = snapshot(&app, task_id).await;
    let update = json!({
        "title": "updated", "description": "after", "progress_pct": 40,
        "soft_deadline": "2026-10-01T00:00:00Z", "hard_deadline": "2026-10-10T00:00:00Z",
        "add_label_ids": [labels[1], labels[1]], "remove_label_ids": [labels[0]],
        "assignees": [{ "user_id": member.id, "role": "assignee" }],
    });

    // 不在・認可・入力拒否でも、複合更新は一切適用されない。
    for (field, value, expected) in [
        (
            "add_label_ids",
            json!([Uuid::new_v4()]),
            StatusCode::BAD_REQUEST,
        ),
        (
            "assignees",
            json!([{ "user_id": outsider.id, "role": "assignee" }]),
            StatusCode::FORBIDDEN,
        ),
        (
            "assignees",
            json!([{ "user_id": member.id, "role": "" }]),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let mut rejected = update.clone();
        rejected[field] = value;
        let response = app.put_json_with_session(&task_path, rejected).await;
        assert_eq!(response.status(), expected);
        assert_eq!(snapshot(&app, task_id).await, before);
    }
    let response = app
        .put_json_with_session(
            &format!("{project_path}/tasks/{}", Uuid::new_v4()),
            update.clone(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // 新しい担当者の追加と通知まで成功した後、元の担当者の削除を DB で失敗させる。
    let trigger = format!("fail_assignee_delete_{}", task_id.simple());
    app.state
        .db
        .execute_unprepared(&format!(
            "CREATE FUNCTION {trigger}() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN RAISE EXCEPTION 'injected assignee deletion failure'; END $$;
         CREATE TRIGGER {trigger} BEFORE DELETE ON task_assignees
         FOR EACH ROW WHEN (OLD.task_id = '{task_id}'::uuid) EXECUTE FUNCTION {trigger}();"
        ))
        .await
        .unwrap();
    let response = app.put_json_with_session(&task_path, update.clone()).await;
    app.state
        .db
        .execute_unprepared(&format!(
            "DROP TRIGGER {trigger} ON task_assignees; DROP FUNCTION {trigger}();"
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(snapshot(&app, task_id).await, before);

    // 同じ要求を成功させると、既存の無関係なラベルを維持し、全項目が応答に載る。
    let response = app.put_json_with_session(&task_path, update).await;
    assert_eq!(response.status(), StatusCode::OK);
    let updated: Value = response.json().await.unwrap();
    assert_eq!(updated["title"], "updated");
    assert_eq!(updated["description"], "after");
    assert_eq!(updated["progress_pct"], 40);
    assert_eq!(updated["soft_deadline"], "2026-10-01T00:00:00Z");
    assert_eq!(updated["hard_deadline"], "2026-10-10T00:00:00Z");
    let actual_labels: Vec<_> = updated["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].clone())
        .collect();
    assert_eq!(actual_labels.len(), 2);
    assert!(actual_labels.contains(&labels[1]) && actual_labels.contains(&labels[2]));
    assert_eq!(updated["assignees"].as_array().unwrap().len(), 1);
    assert_eq!(updated["assignees"][0]["user"]["id"], member.id.to_string());
    let committed = snapshot(&app, task_id).await;
    assert_eq!(
        committed["notifications"].as_array().unwrap().len(),
        before["notifications"].as_array().map_or(0, Vec::len) + 1
    );

    // 同じ利用者の重複指定は通知を増やさず、既存の役割を変更しない。
    let response = app
        .put_json_with_session(
            &task_path,
            json!({
                "assignees": [
                    { "user_id": member.id, "role": "reviewer" },
                    { "user_id": member.id, "role": "reviewer" },
                ],
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let after_noop = snapshot(&app, task_id).await;
    for field in ["assignees", "activities", "notifications", "watchers"] {
        assert_eq!(after_noop[field], committed[field], "{field}");
    }
    // 全置換後に差分を当てる場合も、追加と削除の重複は削除が優先される。
    let response = app.put_json_with_session(&task_path, json!({
        "label_ids": [labels[0]], "add_label_ids": [labels[1]], "remove_label_ids": [labels[1]],
        "assignees": [],
    })).await;
    assert_eq!(response.status(), StatusCode::OK);
    let cleared: Value = response.json().await.unwrap();
    assert_eq!(cleared["assignees"], json!([]));
    assert_eq!(cleared["labels"].as_array().unwrap().len(), 1);
    assert_eq!(cleared["labels"][0]["id"], labels[0]);
}
