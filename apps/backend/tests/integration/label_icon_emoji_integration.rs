use crate::common::TestApp;
use axum::http::StatusCode;
use serde_json::{Value, json};

/// ラベルの絵文字（`icon_emoji`）の作成・更新・検証・タスク応答・export / import の統合テスト。
#[tokio::test]
async fn label_icon_emoji_suite() {
    let mut app = TestApp::new().await;
    let user = app.insert_user(true, false).await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tp = app.insert_tenant_project(user.id).await;

    let labels_path = format!(
        "/v1/tenants/{}/projects/{}/labels",
        tp.tenant_id, tp.project_id
    );

    // 作成: 絵文字ありは応答に入り、なしは null（対照）
    let created = app
        .post_json_with_session(
            &labels_path,
            json!({ "name": "デザイン", "color": "#e11d48", "icon_emoji": "🫟" }),
        )
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: Value = created.json().await.expect("label json");
    assert_eq!(created["icon_emoji"], "🫟");
    assert_eq!(created["color"], "#e11d48");
    let emoji_id = created["id"].as_str().expect("id").to_string();

    let plain = app
        .post_json_with_session(&labels_path, json!({ "name": "bug", "color": "#3b82f6" }))
        .await;
    assert_eq!(plain.status(), StatusCode::CREATED);
    let plain: Value = plain.json().await.expect("label json");
    assert!(plain["icon_emoji"].is_null());
    let plain_id = plain["id"].as_str().expect("id").to_string();

    let list: Value = app
        .get_with_session(&labels_path)
        .await
        .json()
        .await
        .expect("list json");
    let find = |list: &Value, id: &str| -> Value {
        list.as_array()
            .expect("labels array")
            .iter()
            .find(|l| l["id"] == id)
            .cloned()
            .expect("label in list")
    };
    assert_eq!(find(&list, &emoji_id)["icon_emoji"], "🫟");
    assert!(find(&list, &plain_id)["icon_emoji"].is_null());

    // 拒否: 空・空白入り・65 バイト以上は 400
    let too_long = "😀".repeat(17); // 4 バイト × 17 = 68 バイト
    for (i, bad) in ["", "a b", too_long.as_str()].iter().enumerate() {
        let resp = app
            .post_json_with_session(
                &labels_path,
                json!({ "name": format!("bad-{i}"), "color": "#000000", "icon_emoji": bad }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "icon_emoji={bad:?}");
    }
    // 対照: 64 バイト以下の複合の絵文字は通る
    let family = app
        .post_json_with_session(
            &labels_path,
            json!({ "name": "family", "color": "#000000", "icon_emoji": "👨‍👩‍👧‍👦" }),
        )
        .await;
    assert_eq!(family.status(), StatusCode::CREATED);
    let family: Value = family.json().await.expect("label json");
    assert_eq!(family["icon_emoji"], "👨‍👩‍👧‍👦");

    // 更新: 入れる → 保つ → 外す
    let plain_path = format!("{labels_path}/{plain_id}");
    let set = app
        .put_json_with_session(&plain_path, json!({ "icon_emoji": "🐛" }))
        .await;
    assert_eq!(set.status(), StatusCode::OK);
    let set: Value = set.json().await.expect("label json");
    assert_eq!(set["icon_emoji"], "🐛");

    let kept = app
        .put_json_with_session(&plain_path, json!({ "description": "不具合" }))
        .await;
    assert_eq!(kept.status(), StatusCode::OK);
    let kept: Value = kept.json().await.expect("label json");
    assert_eq!(kept["icon_emoji"], "🐛");
    assert_eq!(kept["description"], "不具合");

    let bad_update = app
        .put_json_with_session(&plain_path, json!({ "icon_emoji": "a b" }))
        .await;
    assert_eq!(bad_update.status(), StatusCode::BAD_REQUEST);

    let cleared = app
        .put_json_with_session(&plain_path, json!({ "clear_icon_emoji": true }))
        .await;
    assert_eq!(cleared.status(), StatusCode::OK);
    let cleared: Value = cleared.json().await.expect("label json");
    assert!(cleared["icon_emoji"].is_null());
    assert_eq!(cleared["color"], "#3b82f6");

    // タスクの応答の labels[] に出る
    let status_path = format!(
        "/v1/tenants/{}/projects/{}/statuses",
        tp.tenant_id, tp.project_id
    );
    let status = app
        .post_json_with_session(
            &status_path,
            json!({ "name": "Backlog", "color": "#336699", "position": 0, "is_default": true }),
        )
        .await;
    assert_eq!(status.status(), StatusCode::CREATED);
    let status: Value = status.json().await.expect("status json");
    let tasks_path = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let task = app
        .post_json_with_session(
            &tasks_path,
            json!({
                "title": "絵文字のラベル",
                "status_id": status["id"],
                "label_ids": [emoji_id, plain_id],
            }),
        )
        .await;
    assert_eq!(task.status(), StatusCode::CREATED);
    let task: Value = task.json().await.expect("task json");
    let task_labels = &task["labels"];
    assert_eq!(find(task_labels, &emoji_id)["icon_emoji"], "🫟");
    assert!(find(task_labels, &plain_id)["icon_emoji"].is_null());

    // export に出る
    let export: Value = app
        .get_with_session(&format!("{labels_path}/export"))
        .await
        .json()
        .await
        .expect("export json");
    let exported = export["labels"].as_array().expect("export labels");
    let exported_design = exported
        .iter()
        .find(|l| l["name"] == "デザイン")
        .expect("design in export");
    assert_eq!(exported_design["icon_emoji"], "🫟");
    let exported_bug = exported
        .iter()
        .find(|l| l["name"] == "bug")
        .expect("bug in export");
    assert!(exported_bug["icon_emoji"].is_null());

    // import: 新規に入る。skip は既存を変えない
    let import_path = format!("{labels_path}/import");
    let imported = app
        .post_json_with_session(
            &import_path,
            json!({
                "version": 1,
                "labels": [
                    { "name": "新規", "color": "#111111", "description": "", "icon_emoji": "✨" },
                    { "name": "デザイン", "color": "#222222", "description": "", "icon_emoji": "🎨" },
                ],
            }),
        )
        .await;
    assert_eq!(imported.status(), StatusCode::OK);
    let imported: Value = imported.json().await.expect("import json");
    let by_name = |list: &Value, name: &str| -> Value {
        list.as_array()
            .expect("labels array")
            .iter()
            .find(|l| l["name"] == name)
            .cloned()
            .expect("label by name")
    };
    assert_eq!(by_name(&imported, "新規")["icon_emoji"], "✨");
    assert_eq!(by_name(&imported, "デザイン")["icon_emoji"], "🫟");
    assert_eq!(by_name(&imported, "デザイン")["color"], "#e11d48");

    // overwrite: 絵文字が置き換わる
    let overwritten = app
        .post_json_with_session(
            &import_path,
            json!({
                "version": 1,
                "on_conflict": "overwrite",
                "labels": [
                    { "name": "デザイン", "color": "#222222", "description": "", "icon_emoji": "🎨" },
                ],
            }),
        )
        .await;
    assert_eq!(overwritten.status(), StatusCode::OK);
    let overwritten: Value = overwritten.json().await.expect("import json");
    assert_eq!(by_name(&overwritten, "デザイン")["icon_emoji"], "🎨");
    assert_eq!(by_name(&overwritten, "デザイン")["color"], "#222222");

    // overwrite でも、欄の無いファイル（絵文字を足す前に書き出したもの）は絵文字を残す
    let without_field = app
        .post_json_with_session(
            &import_path,
            json!({
                "version": 1,
                "on_conflict": "overwrite",
                "labels": [ { "name": "デザイン", "color": "#222222", "description": "" } ],
            }),
        )
        .await;
    assert_eq!(without_field.status(), StatusCode::OK);
    let without_field: Value = without_field.json().await.expect("import json");
    assert_eq!(by_name(&without_field, "デザイン")["icon_emoji"], "🎨");

    // overwrite で欄が null なら絵文字を外す
    let null_field = app
        .post_json_with_session(
            &import_path,
            json!({
                "version": 1,
                "on_conflict": "overwrite",
                "labels": [
                    { "name": "デザイン", "color": "#222222", "description": "", "icon_emoji": null },
                ],
            }),
        )
        .await;
    assert_eq!(null_field.status(), StatusCode::OK);
    let null_field: Value = null_field.json().await.expect("import json");
    assert!(by_name(&null_field, "デザイン")["icon_emoji"].is_null());

    // import の項目の絵文字が不正なら 400
    let bad_import = app
        .post_json_with_session(
            &import_path,
            json!({
                "version": 1,
                "labels": [ { "name": "不正", "color": "#111111", "description": "", "icon_emoji": "a b" } ],
            }),
        )
        .await;
    assert_eq!(bad_import.status(), StatusCode::BAD_REQUEST);
}
