mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::Value;

#[tokio::test]
async fn custom_fields_integration_suite() {
    let mut app = TestApp::new().await;
    let user = app.insert_user(true, false).await;
    app.login_session_no_content(&user.email, &user.password).await;
    let tp = app.insert_tenant_project(user.id).await;

    let status_path = format!("/v1/tenants/{}/projects/{}/statuses", tp.tenant_id, tp.project_id);
    let status_resp = app.post_json_with_session(&status_path, serde_json::json!({
        "name": "Todo", "color": "#336699", "position": 0, "is_default": true
    })).await;
    assert_eq!(status_resp.status(), StatusCode::CREATED);
    let status_id = status_resp.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    let fields_base = format!("/v1/tenants/{}/projects/{}/custom-fields", tp.tenant_id, tp.project_id);
    let tasks_base = format!("/v1/tenants/{}/projects/{}/tasks", tp.tenant_id, tp.project_id);

    // --- フィールド定義作成 ---
    let number = app.post_json_with_session(&fields_base, serde_json::json!({
        "name": "Points", "field_type": "number", "is_required": true
    })).await;
    assert_eq!(number.status(), StatusCode::CREATED);
    let number_id = number.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    let select = app.post_json_with_session(&fields_base, serde_json::json!({
        "name": "Size", "field_type": "select",
        "options": [{"label": "M", "value": "m"}, {"label": "L", "value": "l"}]
    })).await;
    assert_eq!(select.status(), StatusCode::CREATED);
    let select_id = select.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    let text_field = app.post_json_with_session(&fields_base, serde_json::json!({
        "name": "Note", "field_type": "text"
    })).await;
    assert_eq!(text_field.status(), StatusCode::CREATED);
    let text_id = text_field.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    let date_field = app.post_json_with_session(&fields_base, serde_json::json!({
        "name": "Due", "field_type": "date"
    })).await;
    assert_eq!(date_field.status(), StatusCode::CREATED);
    let date_id = date_field.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    let url_field = app.post_json_with_session(&fields_base, serde_json::json!({
        "name": "Link", "field_type": "url"
    })).await;
    assert_eq!(url_field.status(), StatusCode::CREATED);
    let url_id = url_field.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    let checkbox_field = app.post_json_with_session(&fields_base, serde_json::json!({
        "name": "Done", "field_type": "checkbox"
    })).await;
    assert_eq!(checkbox_field.status(), StatusCode::CREATED);
    let checkbox_id = checkbox_field.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    // --- バリデーション: select で options 外の値 → 400 ---
    let bad_select = app.post_json_with_session(&tasks_base, serde_json::json!({
        "title": "bad-select", "status_id": status_id,
        "custom_field_values": [
            {"field_id": number_id, "value": "1"},
            {"field_id": select_id, "value": "xl"}
        ]
    })).await;
    assert_eq!(bad_select.status(), StatusCode::BAD_REQUEST);

    // --- バリデーション: number に非数値 → 400 ---
    let bad_number = app.post_json_with_session(&tasks_base, serde_json::json!({
        "title": "bad-number", "status_id": status_id,
        "custom_field_values": [{"field_id": number_id, "value": "abc"}]
    })).await;
    assert_eq!(bad_number.status(), StatusCode::BAD_REQUEST);

    // --- バリデーション: date に無効な日付 (2024-02-30) → 400 ---
    let bad_date = app.post_json_with_session(&tasks_base, serde_json::json!({
        "title": "bad-date", "status_id": status_id,
        "custom_field_values": [
            {"field_id": number_id, "value": "1"},
            {"field_id": date_id, "value": "2024-02-30"}
        ]
    })).await;
    assert_eq!(bad_date.status(), StatusCode::BAD_REQUEST);

    // --- バリデーション: checkbox に true/false 以外 → 400 ---
    let bad_checkbox = app.post_json_with_session(&tasks_base, serde_json::json!({
        "title": "bad-checkbox", "status_id": status_id,
        "custom_field_values": [
            {"field_id": number_id, "value": "1"},
            {"field_id": checkbox_id, "value": "yes"}
        ]
    })).await;
    assert_eq!(bad_checkbox.status(), StatusCode::BAD_REQUEST);

    // --- バリデーション: url に無効な URL → 400 ---
    let bad_url = app.post_json_with_session(&tasks_base, serde_json::json!({
        "title": "bad-url", "status_id": status_id,
        "custom_field_values": [
            {"field_id": number_id, "value": "1"},
            {"field_id": url_id, "value": "not-a-url"}
        ]
    })).await;
    assert_eq!(bad_url.status(), StatusCode::BAD_REQUEST);

    // --- バリデーション: is_required フィールドに明示的 null → 400 ---
    // ensure_required_custom_fields は done 状態時のみ呼ばれる設計のため、
    // 非 done ステータスで空配列を送っても 400 にはならない。
    // 必須チェックは値を明示的に null/空で送った場合に upsert_task_custom_field_values が検出する。
    let missing_required = app.post_json_with_session(&tasks_base, serde_json::json!({
        "title": "missing-required", "status_id": status_id,
        "custom_field_values": [{"field_id": number_id, "value": null}]
    })).await;
    assert_eq!(missing_required.status(), StatusCode::BAD_REQUEST);

    // --- 正常作成: 全フィールド型を含むタスク ---
    let task = app.post_json_with_session(&tasks_base, serde_json::json!({
        "title": "ok", "status_id": status_id,
        "custom_field_values": [
            {"field_id": number_id, "value": "5"},
            {"field_id": select_id, "value": "m"},
            {"field_id": text_id, "value": "hello"},
            {"field_id": date_id, "value": "2025-01-15"},
            {"field_id": url_id, "value": "https://example.com"},
            {"field_id": checkbox_id, "value": "true"}
        ]
    })).await;
    assert_eq!(task.status(), StatusCode::CREATED);
    let task_id = task.json::<Value>().await.expect("json")["id"]
        .as_str().unwrap().to_string();

    // --- GET: custom_field_values が全フィールド分返る ---
    let get = app.get_with_session(&format!("{tasks_base}/{task_id}")).await;
    assert_eq!(get.status(), StatusCode::OK);
    let body = get.json::<Value>().await.expect("json");
    let values = body["custom_field_values"]
        .as_array()
        .expect("custom_field_values should be array");
    assert_eq!(values.len(), 6);
    assert!(values.iter().any(|v| v["field"]["id"] == number_id && v["value"] == "5"));
    assert!(values.iter().any(|v| v["field"]["id"] == select_id && v["value"] == "m" && v["display_value"] == "M"));
    assert!(values.iter().any(|v| v["field"]["id"] == text_id && v["value"] == "hello"));
    assert!(values.iter().any(|v| v["field"]["id"] == date_id && v["value"] == "2025-01-15"));
    assert!(values.iter().any(|v| v["field"]["id"] == url_id && v["value"] == "https://example.com"));
    assert!(values.iter().any(|v| v["field"]["id"] == checkbox_id && v["value"] == "true"));

    // --- フィールド更新: PATCH ---
    let patch = app.patch_json_with_session(
        &format!("{fields_base}/{select_id}"),
        serde_json::json!({"options": [{"label": "M", "value": "m"}, {"label": "XL", "value": "xl"}]}),
    ).await;
    assert_eq!(patch.status(), StatusCode::OK);

    // --- フィールド削除 → タスク値も CASCADE 削除される ---
    let delete = app.delete_with_session(&format!("{fields_base}/{text_id}")).await;
    assert_eq!(delete.status(), StatusCode::NO_CONTENT);

    let get2 = app.get_with_session(&format!("{tasks_base}/{task_id}")).await;
    assert_eq!(get2.status(), StatusCode::OK);
    let body2 = get2.json::<Value>().await.expect("json");
    let values2 = body2["custom_field_values"].as_array().expect("array");
    assert_eq!(values2.len(), 5, "text フィールド削除後は5件");
    assert!(!values2.iter().any(|v| v["field"]["id"] == text_id), "削除済みフィールドは返らない");
}
