//! 通知のコマンド。仕様は `docs/features/tasks/5.notifications.md` §4。
//!
//! 送る内容（絞り込みの query、設定の PUT 本文）と、送る前に弾く入力を軸に確かめる。

mod common;

use common::*;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

fn settings_path() -> String {
    format!("/v1/users/me/notification-settings/{PROJECT_ID}")
}

async fn mount_project_lookup(harness: &Harness) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/projects")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([project_json()])))
        .mount(&harness.server)
        .await;
}

fn notification_json(notification_type: &str, payload: serde_json::Value) -> serde_json::Value {
    json!({
        "id": "66666666-6666-4666-8666-666666666666",
        "notification_type": notification_type,
        "project_id": PROJECT_ID,
        "task": null,
        "payload": payload,
        "cursor": "MjAyNi0wMS0wMVQwMDowMDowMFo",
        "read_at": null,
        "created_at": "2026-01-01T00:00:00Z",
    })
}

#[tokio::test]
async fn list_sends_the_unread_filter_and_the_limit() {
    let harness = harness().await;
    Mock::given(method("GET"))
        .and(path("/v1/users/me/notifications"))
        .and(query_param("unread", "true"))
        .and(query_param("limit", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "unread_count": 1,
            "next_cursor": null,
            "notifications": [notification_json(
                "review_round_created",
                json!({
                    "repo": "koyori-app/task",
                    "pr_number": 618,
                    "round": 2,
                    "reviewer": "yupix",
                    "counts": { "high": 1, "medium": 0, "low": 0, "nit": 0 },
                }),
            )],
        })))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&["task", "notifications", "list", "--unread", "--limit", "10"])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

/// 既読化は本文を返さない（204）。空応答を失敗と取り違えない。
#[tokio::test]
async fn read_all_accepts_an_empty_response() {
    let harness = harness().await;
    Mock::given(method("PATCH"))
        .and(path("/v1/users/me/notifications/read-all"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&["task", "notifications", "read-all"])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn read_marks_one_notification() {
    let harness = harness().await;
    let id = "66666666-6666-4666-8666-666666666666";
    Mock::given(method("PATCH"))
        .and(path(format!("/v1/users/me/notifications/{id}/read")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": id,
            "notification_type": "assigned",
            "project_id": PROJECT_ID,
            "task": { "id": "22222222-2222-4222-8222-222222222222", "seq_id": 7, "title": "Golden task" },
            "payload": { "assigned_by": "yupix", "role": "primary" },
            "cursor": "MjAyNi0wMS0wMVQwMDowMDowMFo",
            "read_at": "2026-01-02T00:00:00Z",
            "created_at": "2026-01-01T00:00:00Z",
        })))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&["task", "notifications", "read", id])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

/// 綴りを外した種別は送る前に落とす。送ってしまうと、どの値が未知なのかを
/// サーバーの応答から読み取れない。
#[tokio::test]
async fn settings_rejects_an_unknown_event_type_before_sending() {
    let harness = harness().await;

    let err = harness
        .run(&[
            "task",
            "notifications",
            "settings",
            "--project",
            "APP",
            "--in-app",
            "assigned,mentiond",
        ])
        .await
        .unwrap_err();

    assert_eq!(err.exit_code, 2);
    assert!(err.message.contains("mentiond"), "{}", err.message);
    assert!(harness.sent_nothing().await, "送信前に弾く");
}

/// 片方だけ指定したとき、もう片方は現在値のまま送る（空にしない）。
#[tokio::test]
async fn settings_keeps_the_side_that_was_not_given() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("GET"))
        .and(path(settings_path()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "email_events": ["assigned"],
            "in_app_events": ["mentioned"],
        })))
        .expect(1)
        .mount(&harness.server)
        .await;
    Mock::given(method("PUT"))
        .and(path(settings_path()))
        .and(body_json(json!({
            "email_events": ["assigned"],
            "in_app_events": ["assigned", "comment_added"],
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "email_events": ["assigned"],
            "in_app_events": ["assigned", "comment_added"],
        })))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "notifications",
            "settings",
            "--project",
            "APP",
            "--in-app",
            "assigned,comment_added",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

/// UUID 指定はプロジェクト API（`read:project`）を呼ばない。設定 API の
/// `read:task` だけを持つ PAT が、手前のプロジェクト解決で 403 にならないように。
#[tokio::test]
async fn settings_with_a_uuid_skips_the_project_lookup() {
    let harness = harness().await;
    Mock::given(method("GET"))
        .and(path(settings_path()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "email_events": [],
            "in_app_events": ["assigned"],
        })))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&["task", "notifications", "settings", "--project", PROJECT_ID])
        .await
        .unwrap();
    assert_eq!(code, 0);

    let project_calls = harness
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|request| request.url.path().starts_with("/v1/tenants/"))
        .count();
    assert_eq!(project_calls, 0, "プロジェクト API を経由しない");
}

/// フラグ無しは読み取りだけ。設定を書き換えない。
#[tokio::test]
async fn settings_without_flags_only_reads() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("GET"))
        .and(path(settings_path()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "email_events": [],
            "in_app_events": ["assigned"],
        })))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&["task", "notifications", "settings", "--project", "APP"])
        .await
        .unwrap();
    assert_eq!(code, 0);

    let puts = harness
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|request| request.method == wiremock::http::Method::PUT)
        .count();
    assert_eq!(puts, 0, "読み取りだけで PUT は出ない");
}
