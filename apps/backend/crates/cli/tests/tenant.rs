//! tenant_id の解決。未設定なら PAT の束縛先を `GET /v1/personal_tokens/me` から引く。
//!
//! `/me` を何回叩くかを口ごとに固定する。テナントを含まない口で叩くと、
//! 余計な往復が増えるうえ `/me` の失敗で関係のない操作まで止まる。

mod common;

use common::*;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const ME: &str = "/v1/personal_tokens/me";
const PERMISSION_DENIED: &str = "Permission denied: insufficient access for this resource";

fn projects_path() -> String {
    format!("/v1/tenants/{TENANT}/projects")
}

async fn mount_token_identity(harness: &Harness) {
    Mock::given(method("GET"))
        .and(path(ME))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_identity_json()))
        .mount(&harness.server)
        .await;
}

async fn mount_projects(harness: &Harness, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(projects_path()))
        .respond_with(response)
        .mount(&harness.server)
        .await;
}

fn project_list() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!([project_json()]))
}

/// テナント付きのリクエストが何本あっても `/me` は 1 回で、config には書かない。
#[tokio::test]
async fn resolves_an_unset_tenant_from_the_token_once_without_saving_it() {
    let harness = harness_with(None, &[]).await;
    mount_token_identity(&harness).await;
    mount_projects(&harness, project_list()).await;
    Mock::given(method("GET"))
        .and(path(project_path("statuses")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([status_json(
            "33333333-3333-4333-8333-333333333333",
            "Todo",
            true,
            false,
            false,
            0
        )])))
        .expect(1)
        .mount(&harness.server)
        .await;
    let before = harness.store().load().unwrap();

    let code = harness
        .run(&["task", "projects", "statuses", "--project", "APP"])
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(harness.calls_to(ME).await, 1);
    assert_eq!(harness.calls_to(&projects_path()).await, 1);
    assert_eq!(harness.store().load().unwrap(), before);
}

#[tokio::test]
async fn an_explicit_tenant_from_config_or_env_skips_the_token_endpoint() {
    for harness in [
        harness_with(Some(TENANT), &[]).await,
        harness_with(None, &[("TASK_TENANT", TENANT)]).await,
    ] {
        mount_projects(&harness, project_list()).await;

        assert_eq!(harness.run(&["task", "projects", "list"]).await.unwrap(), 0);
        assert_eq!(harness.calls_to(ME).await, 0);
    }
}

#[tokio::test]
async fn whoami_calls_the_token_endpoint_once_without_a_tenant() {
    let harness = harness_with(None, &[]).await;
    mount_token_identity(&harness).await;

    assert_eq!(harness.run(&["task", "auth", "whoami"]).await.unwrap(), 0);
    assert_eq!(harness.calls_to(ME).await, 1);
}

/// 通知の一覧・既読はテナントを含まないパスなので、tenant を引かない。
#[tokio::test]
async fn notifications_list_and_read_do_not_resolve_the_tenant() {
    let harness = harness_with(None, &[]).await;
    let id = "66666666-6666-4666-8666-666666666666";
    Mock::given(method("GET"))
        .and(path("/v1/users/me/notifications"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "unread_count": 0,
            "next_cursor": null,
            "notifications": [],
        })))
        .expect(1)
        .mount(&harness.server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/v1/users/me/notifications/{id}/read")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": id,
            "notification_type": "assigned",
            "project_id": PROJECT_ID,
            "task": null,
            "payload": { "assigned_by": "yupix", "role": "primary" },
            "cursor": "MjAyNi0wMS0wMVQwMDowMDowMFo",
            "read_at": "2026-01-02T00:00:00Z",
            "created_at": "2026-01-01T00:00:00Z",
        })))
        .expect(1)
        .mount(&harness.server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/v1/users/me/notifications/read-all"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&harness.server)
        .await;

    for args in [
        &["task", "notifications", "list"][..],
        &["task", "notifications", "read", id],
        &["task", "notifications", "read-all"],
    ] {
        assert_eq!(harness.run(args).await.unwrap(), 0, "{args:?}");
    }
    assert_eq!(harness.calls_to(ME).await, 0);
}

/// キーはテナント配下の一覧から引くので `/me` が要る。UUID はそのまま渡すので要らない。
#[tokio::test]
async fn notification_settings_resolves_the_tenant_only_for_a_project_key() {
    for (project, me_calls) in [("APP", 1), (PROJECT_ID, 0)] {
        let harness = harness_with(None, &[]).await;
        mount_token_identity(&harness).await;
        mount_projects(&harness, project_list()).await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/v1/users/me/notification-settings/{PROJECT_ID}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "email_events": [],
                "in_app_events": ["assigned"],
            })))
            .expect(1)
            .mount(&harness.server)
            .await;

        let code = harness
            .run(&["task", "notifications", "settings", "--project", project])
            .await
            .unwrap();

        assert_eq!(code, 0);
        assert_eq!(harness.calls_to(ME).await, me_calls, "--project {project}");
    }
}

#[tokio::test]
async fn a_forbidden_response_names_the_explicit_tenant_to_remove() {
    let cases: [(Harness, &str); 2] = [
        (
            harness_with(Some(TENANT), &[]).await,
            "tenant_id is set in config; if the token belongs to another tenant, run `task config unset tenant_id`",
        ),
        // env は config に勝つので、両方あれば env の解除を案内する
        (
            harness_with(Some(TENANT), &[("TASK_TENANT", TENANT)]).await,
            "tenant_id is set by TASK_TENANT; if the token belongs to another tenant, unset it",
        ),
    ];
    for (harness, hint) in cases {
        mount_projects(&harness, ResponseTemplate::new(403)).await;

        let err = harness
            .run(&["task", "projects", "list"])
            .await
            .unwrap_err();

        assert_eq!(err.exit_code, 4);
        assert_eq!(err.message, format!("{PERMISSION_DENIED}\n{hint}"));
    }
}

/// 自動で引いた tenant は束縛先そのもの。tenant を疑う案内は誤誘導になる。
#[tokio::test]
async fn a_forbidden_response_has_no_tenant_hint_when_the_tenant_came_from_the_token() {
    let harness = harness_with(None, &[]).await;
    mount_token_identity(&harness).await;
    mount_projects(&harness, ResponseTemplate::new(403)).await;

    let err = harness
        .run(&["task", "projects", "list"])
        .await
        .unwrap_err();

    assert_eq!(err.exit_code, 4);
    assert_eq!(err.message, PERMISSION_DENIED);
}

/// テナントを含まない口は、明示 tenant があっても tenant を送らないので案内しない。
#[tokio::test]
async fn a_forbidden_response_on_a_tenantless_path_has_no_tenant_hint() {
    let harness = harness_with(Some(TENANT), &[]).await;
    Mock::given(method("GET"))
        .and(path(ME))
        .respond_with(ResponseTemplate::new(403))
        .mount(&harness.server)
        .await;

    let err = harness.run(&["task", "auth", "whoami"]).await.unwrap_err();

    assert_eq!(err.message, PERMISSION_DENIED);
}

/// `/me` が失敗したら後続を送らず、config にも書かない。
#[tokio::test]
async fn a_failed_token_lookup_stops_before_any_tenant_request() {
    let cases = [
        (ResponseTemplate::new(401), 3),
        (ResponseTemplate::new(403), 4),
        (ResponseTemplate::new(500), 1),
        (
            ResponseTemplate::new(200).set_body_json(json!({ "id": "not-a-token" })),
            1,
        ),
    ];
    for (response, exit_code) in cases {
        let harness = harness_with(None, &[]).await;
        Mock::given(method("GET"))
            .and(path(ME))
            .respond_with(response)
            .mount(&harness.server)
            .await;
        mount_projects(&harness, project_list()).await;
        let before = harness.store().load().unwrap();

        let err = harness
            .run(&["task", "projects", "list"])
            .await
            .unwrap_err();

        assert_eq!(err.exit_code, exit_code, "{}", err.message);
        let requests = harness.server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "/me の後に何も送らない");
        assert_eq!(harness.store().load().unwrap(), before);
    }
}

/// 手元の設定だけを触るコマンドは、接続設定が無くても動く。
#[tokio::test]
async fn local_commands_work_without_any_connection_settings() {
    let (_home, context) = harness_without_config();

    for args in [
        &["task", "config", "list"][..],
        &["task", "config", "set", "tenant_id", TENANT],
        &["task", "config", "get", "tenant_id"],
        &["task", "config", "unset", "tenant_id"],
        &["task", "auth", "token", "token-1"],
        &["task", "auth", "logout"],
    ] {
        let code = task_cli::run(parse(args), &context).await.unwrap();
        assert_eq!(code, 0, "{args:?}");
    }
}
