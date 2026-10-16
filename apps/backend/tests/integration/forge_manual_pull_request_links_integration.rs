//! PR の手動リンク・解除（TASK-288、docs/features/tasks/9.github-tasks.md §8）。

use crate::common::TestApp;
use crate::forge_commit_links_integration::{
    REPO_NAME, REPO_OWNER, insert_project, insert_status, insert_task, setup,
};
use crate::forge_pull_request_links_integration::{
    deliver, links, mount_github_defaults, pr_payload,
};
use entity::{forge_pull_requests, scopes::Scope, task_activities};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use service::forge::pull_requests::{PULL_REQUEST_LINKED_EVENT, PULL_REQUEST_UNLINKED_EVENT};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// GitHub API のモック。インストールトークンと空のコミット一覧（`Opened` が引く）だけ返し、
/// PR 取得は各テストで足す（無ければ 404）
async fn github_mock() -> MockServer {
    let mock_server = MockServer::start().await;
    // SAFETY: 統合テストは --test-threads=1 で直列に走る
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    mount_github_defaults(&mock_server).await;
    mock_server
}

fn forget_github_mock() {
    // SAFETY: 統合テストは --test-threads=1 で直列に走る
    unsafe {
        std::env::remove_var("GITHUB_API_BASE_URL");
    }
}

fn pulls_url(tenant_id: Uuid, project_id: Uuid, task_id: Uuid) -> String {
    format!("/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{task_id}/forge/pull-requests")
}

async fn link(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    task_id: Uuid,
    body: serde_json::Value,
    token: &str,
) -> (u16, serde_json::Value) {
    let res = app
        .post_json_with_bearer(&pulls_url(tenant_id, project_id, task_id), body, token)
        .await;
    let status = res.status().as_u16();
    (status, res.json().await.unwrap_or(serde_json::Value::Null))
}

async fn unlink(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    task_id: Uuid,
    pull_request_id: &str,
    token: &str,
) -> u16 {
    app.client()
        .delete(format!(
            "{}{}/{pull_request_id}",
            app.base_url(),
            pulls_url(tenant_id, project_id, task_id)
        ))
        .bearer_auth(token)
        .send()
        .await
        .expect("delete request")
        .status()
        .as_u16()
}

async fn activities(app: &TestApp, task_id: Uuid, event_type: &str) -> Vec<task_activities::Model> {
    task_activities::Entity::find()
        .filter(task_activities::Column::TaskId.eq(task_id))
        .filter(task_activities::Column::EventType.eq(event_type))
        .all(&app.state.db)
        .await
        .expect("query activities")
}

async fn forge_repository(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    task_id: Uuid,
    token: &str,
) -> serde_json::Value {
    let res = app
        .get_with_bearer(
            &format!("/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{task_id}/forge"),
            token,
        )
        .await;
    assert_eq!(res.status().as_u16(), 200);
    res.json::<serde_json::Value>().await.expect("json")["repository"].clone()
}

/// PR 取得 API の応答（関係するフィールドだけ）
fn api_pull_request(number: i32, title: &str, body: &str, updated_at: &str) -> serde_json::Value {
    serde_json::json!({
        "number": number,
        "state": "open",
        "title": title,
        "body": body,
        "user": { "login": "octocat" },
        "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/{number}"),
        "updated_at": updated_at,
        "merged_at": null,
        "head": { "ref": "feat/oauth", "sha": "a3f92c1e7b81d4000000000000000000000000aa" }
    })
}

/// 手動リンクはホストから PR を取って行を埋め、`manual` 行として結ぶ。履歴は初回だけで、操作者が主語。
/// 外すと全参照元の行が消えて PR 行は残り、本文の `KEY-N` で戻っても `linked` の履歴は増えない。
/// `manual` 行は webhook の再構築で消えない。
#[tokio::test]
async fn manual_link_and_unlink() {
    let mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let other = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO_OWNER}/{REPO_NAME}/pulls/87")))
        .respond_with(ResponseTemplate::new(200).set_body_json(api_pull_request(
            87,
            "feat: OAuth",
            &format!("{key}-2"),
            "2026-10-01T00:00:00Z",
        )))
        .mount(&mock_server)
        .await;
    let token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;

    let (status, pr) = link(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        task,
        serde_json::json!({ "number": 87, "closes": true }),
        &token,
    )
    .await;
    assert_eq!(status, 201, "{pr}");
    assert_eq!(pr["number"], 87);
    assert_eq!(pr["title"], "feat: OAuth");
    assert_eq!(pr["author_handle"], "octocat");
    assert_eq!(pr["state"], "open");
    assert_eq!(pr["repo_owner"], REPO_OWNER);
    assert_eq!(pr["sources"], serde_json::json!(["manual"]));
    assert_eq!(pr["closes"], true);
    let pull_request_id = pr["id"].as_str().expect("id").to_owned();
    assert_eq!(links(&app, task).await, vec![("manual".to_string(), true)]);
    // PR の本文は webhook と同じ共通処理で解決される
    assert_eq!(links(&app, other).await, vec![("body".to_string(), false)]);

    let linked = activities(&app, task, PULL_REQUEST_LINKED_EVENT).await;
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0].user_id, Some(fx.owner_id), "主語は操作者");
    assert_eq!(linked[0].payload["source"], "manual");
    assert_eq!(
        linked[0].dedupe_key.as_deref(),
        Some(format!("{PULL_REQUEST_LINKED_EVENT}:{pull_request_id}:{task}").as_str())
    );

    // 付け直しは closes を更新するだけで、履歴は増えない
    let (status, pr) = link(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        task,
        serde_json::json!({ "number": 87 }),
        &token,
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(pr["closes"], false);
    assert_eq!(
        activities(&app, task, PULL_REQUEST_LINKED_EVENT)
            .await
            .len(),
        1
    );

    // 本文由来の行があるタスクでも、外すと全参照元の行が消える
    let (status, pr) = link(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        other,
        serde_json::json!({ "number": 87 }),
        &token,
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(pr["sources"], serde_json::json!(["body", "manual"]));
    assert_eq!(
        unlink(
            &app,
            fx.tp.tenant_id,
            fx.tp.project_id,
            other,
            &pull_request_id,
            &token
        )
        .await,
        204
    );
    assert!(links(&app, other).await.is_empty());
    assert!(
        forge_pull_requests::Entity::find_by_id(Uuid::parse_str(&pull_request_id).unwrap())
            .one(&app.state.db)
            .await
            .expect("query pull request")
            .is_some(),
        "PR 行は残す"
    );
    let unlinked = activities(&app, other, PULL_REQUEST_UNLINKED_EVENT).await;
    assert_eq!(unlinked.len(), 1);
    assert_eq!(unlinked[0].user_id, Some(fx.owner_id));
    assert_eq!(unlinked[0].dedupe_key, None);
    assert_eq!(
        unlinked[0].payload,
        serde_json::json!({
            "host": "github",
            "number": 87,
            "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/87"),
            "pull_request_id": pull_request_id,
        })
    );
    assert_eq!(
        unlink(
            &app,
            fx.tp.tenant_id,
            fx.tp.project_id,
            other,
            &pull_request_id,
            &token
        )
        .await,
        404,
        "リンクが無ければ 404"
    );

    // 本文の KEY-N で戻っても linked の履歴は増えない。task の manual 行は再構築で消えない
    let edited = pr_payload(
        fx.installation_id,
        "edited",
        87,
        "feat: OAuth",
        Some(&format!("{key}-2")),
        "2026-10-01T00:10:00Z",
    );
    deliver(&app, &fx, &edited).await.expect("edited");
    assert_eq!(links(&app, other).await, vec![("body".to_string(), false)]);
    assert_eq!(
        activities(&app, other, PULL_REQUEST_LINKED_EVENT)
            .await
            .len(),
        1
    );
    assert_eq!(links(&app, task).await, vec![("manual".to_string(), false)]);

    // 手の操作なので、もう一度外せば履歴も 2 行になる
    assert_eq!(
        unlink(
            &app,
            fx.tp.tenant_id,
            fx.tp.project_id,
            other,
            &pull_request_id,
            &token
        )
        .await,
        204
    );
    assert_eq!(
        activities(&app, other, PULL_REQUEST_UNLINKED_EVENT)
            .await
            .len(),
        2
    );

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}

/// PR 取得 API が引けなくても（404・障害）`state = unknown` で作ってリンクする。
#[tokio::test]
async fn manual_link_without_host_data_is_unknown() {
    let mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO_OWNER}/{REPO_NAME}/pulls/91")))
        .respond_with(ResponseTemplate::new(502))
        .mount(&mock_server)
        .await;
    let token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;

    // 90 は 404（モックに無い）、91 は 502
    for number in [90, 91] {
        let (status, pr) = link(
            &app,
            fx.tp.tenant_id,
            fx.tp.project_id,
            task,
            serde_json::json!({ "number": number }),
            &token,
        )
        .await;
        assert_eq!(status, 201, "{pr}");
        assert_eq!(pr["state"], "unknown");
        assert_eq!(pr["title"], "");
        assert_eq!(
            pr["html_url"],
            format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/{number}")
        );
        assert_eq!(pr["sources"], serde_json::json!(["manual"]));
    }
    let linked = activities(&app, task, PULL_REQUEST_LINKED_EVENT).await;
    assert_eq!(linked.len(), 2);
    assert!(linked.iter().all(|a| a.payload["state"] == "unknown"));

    // 後から届いた webhook で中身が埋まり、manual 行は残る
    let opened = pr_payload(
        fx.installation_id,
        "opened",
        90,
        "feat: later",
        None,
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");
    let row = forge_pull_requests::Entity::find()
        .filter(forge_pull_requests::Column::ProjectId.eq(fx.tp.project_id))
        .filter(forge_pull_requests::Column::Number.eq(90))
        .one(&app.state.db)
        .await
        .expect("query pull request")
        .expect("pull request row");
    assert_eq!(row.state, "open");
    assert_eq!(row.title, "feat: later");
    assert_eq!(links(&app, task).await.len(), 2);

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}

/// 連携なしは 409、メンバー外・write:task なしは 403、他プロジェクトのタスクは 404。
/// 対照としてオーナーは 201 / 204。PR 取得 API は認可を通るまで呼ばない。
#[tokio::test]
async fn manual_link_authorization() {
    let mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let body = serde_json::json!({ "number": 87 });

    let outsider = app.insert_user(false, false).await;
    let outsider_token = app
        .insert_pat(outsider.id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;
    let (status, _) = link(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        task,
        body.clone(),
        &outsider_token,
    )
    .await;
    assert_eq!(status, 403, "テナントのメンバー外");

    let read_only = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let (status, _) = link(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        task,
        body.clone(),
        &read_only,
    )
    .await;
    assert_eq!(status, 403, "write:task が無い");

    let token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;
    // 同じテナントの、連携していない別プロジェクト
    let second_key = format!("S{}", &fx.key[1..]);
    let second_project = insert_project(&app, fx.tp.tenant_id, &second_key).await;
    let (status, _) = link(
        &app,
        fx.tp.tenant_id,
        second_project,
        task,
        body.clone(),
        &token,
    )
    .await;
    assert_eq!(status, 404, "他プロジェクトのタスク");
    let second_status = insert_status(&app, second_project).await;
    let second_task = insert_task(&app, second_project, second_status, fx.owner_id, 1).await;
    let (status, error) = link(
        &app,
        fx.tp.tenant_id,
        second_project,
        second_task,
        body.clone(),
        &token,
    )
    .await;
    assert_eq!(status, 409, "連携が無い");
    assert_eq!(error["message"], "github-integration-missing");

    // 連携先はタスクを読めるだけの PAT にも返る（ボタンの可否に使う）。連携が無ければ null
    assert_eq!(
        forge_repository(&app, fx.tp.tenant_id, fx.tp.project_id, task, &read_only).await,
        serde_json::json!({
            "host": "github",
            "host_url": "https://github.com",
            "repo_owner": REPO_OWNER,
            "repo_name": REPO_NAME,
            "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}"),
        })
    );
    assert_eq!(
        forge_repository(
            &app,
            fx.tp.tenant_id,
            second_project,
            second_task,
            &read_only
        )
        .await,
        serde_json::Value::Null
    );
    let (status, _) = link(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        task,
        serde_json::json!({ "number": 0 }),
        &token,
    )
    .await;
    assert_eq!(status, 400, "PR 番号は 1 以上");
    assert!(
        mock_server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "拒否した要求で GitHub を呼ばない"
    );

    let (status, pr) = link(&app, fx.tp.tenant_id, fx.tp.project_id, task, body, &token).await;
    assert_eq!(status, 201);
    let pull_request_id = pr["id"].as_str().expect("id").to_owned();

    assert_eq!(
        unlink(
            &app,
            fx.tp.tenant_id,
            fx.tp.project_id,
            task,
            &pull_request_id,
            &outsider_token
        )
        .await,
        403
    );
    assert_eq!(
        unlink(
            &app,
            fx.tp.tenant_id,
            fx.tp.project_id,
            task,
            &pull_request_id,
            &read_only
        )
        .await,
        403
    );
    assert_eq!(
        unlink(
            &app,
            fx.tp.tenant_id,
            second_project,
            task,
            &pull_request_id,
            &token
        )
        .await,
        404
    );
    assert_eq!(
        unlink(
            &app,
            fx.tp.tenant_id,
            fx.tp.project_id,
            task,
            &pull_request_id,
            &token
        )
        .await,
        204
    );

    forget_github_mock();
    app.cleanup_user(outsider.id).await;
    app.cleanup_user(fx.owner_id).await;
}
