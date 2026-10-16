//! pull_request webhook → PR とタスクのリンク、`GET /tasks/{id}/forge`
//! （TASK-220、docs/features/tasks/9.github-tasks.md §5「PRイベント共通の前処理」/ §8）。

use crate::common::TestApp;
use crate::forge_commit_links_integration::{
    Fixture, REPO_NAME, REPO_OWNER, insert_project, insert_task, job_state, queued_jobs, setup,
    sign,
};
use apalis::prelude::Data;
use entity::{forge_pull_request_links, forge_pull_requests, scopes::Scope, task_activities};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, QueryFilter, QueryOrder, Statement,
};
use service::forge::commits::apply_push;
use service::forge::events::{ForgeCommit, ForgeRepo};
use service::forge::pull_requests::PULL_REQUEST_LINKED_EVENT;
use uuid::Uuid;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn repo() -> ForgeRepo {
    ForgeRepo {
        host: "github".into(),
        host_url: "https://github.com".into(),
        repo_owner: REPO_OWNER.into(),
        repo_name: REPO_NAME.into(),
    }
}

/// GitHub の pull_request ペイロード（関係するフィールドだけ）
pub(crate) fn pr_payload(
    installation_id: i64,
    action: &str,
    number: i32,
    title: &str,
    body: Option<&str>,
    updated_at: &str,
) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "number": number,
        "pull_request": {
            "number": number,
            "state": if action == "closed" { "closed" } else { "open" },
            "title": title,
            "body": body,
            "user": { "login": "yupix", "id": 20 },
            "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/{number}"),
            "updated_at": updated_at,
            "merged_at": null,
            "merged": false,
            "head": { "ref": "feat/oauth", "sha": "a3f92c1e7b81d4000000000000000000000000aa" },
            "base": { "ref": "main", "sha": "b17cd9000000000000000000000000000000000a" }
        },
        "repository": { "name": REPO_NAME, "owner": { "login": REPO_OWNER } },
        "installation": { "id": installation_id },
        "sender": { "login": "yupix", "id": 20 }
    })
}

fn merged(mut payload: serde_json::Value, merged_at: &str) -> serde_json::Value {
    payload["pull_request"]["merged"] = true.into();
    payload["pull_request"]["merged_at"] = merged_at.into();
    payload
}

async fn post_event(app: &TestApp, delivery_id: &str, body: &serde_json::Value) -> u16 {
    let body = serde_json::to_vec(body).expect("serialize body");
    app.client()
        .post(format!("{}/v1/github/webhook", app.base_url()))
        .header("content-type", "application/json")
        .header("X-GitHub-Event", "pull_request")
        .header("X-GitHub-Delivery", delivery_id)
        .header("X-Hub-Signature-256", sign(&body))
        .body(body)
        .send()
        .await
        .expect("post webhook")
        .status()
        .as_u16()
}

/// 配信を受け、積まれたジョブをワーカーと同じ関数で処理する。
pub(crate) async fn deliver(
    app: &TestApp,
    fx: &Fixture,
    payload: &serde_json::Value,
) -> Result<(), apalis::prelude::BoxDynError> {
    let delivery = Uuid::new_v4().to_string();
    assert_eq!(post_event(app, &delivery, payload).await, 200);
    let job = queued_jobs(app, fx.tp.project_id)
        .await
        .into_iter()
        .find(|job| job.delivery_id.as_deref() == Some(delivery.as_str()))
        .expect("queued pull_request job");
    job::github_webhook::process(job, Data::new(job_state(app))).await
}

/// タスクのリンク行（参照元, closes）。参照元の昇順
pub(crate) async fn links(app: &TestApp, task_id: Uuid) -> Vec<(String, bool)> {
    forge_pull_request_links::Entity::find()
        .filter(forge_pull_request_links::Column::TaskId.eq(task_id))
        .order_by_asc(forge_pull_request_links::Column::Source)
        .all(&app.state.db)
        .await
        .expect("query links")
        .into_iter()
        .map(|link| (link.source, link.closes))
        .collect()
}

async fn pr_row(app: &TestApp, project_id: Uuid, number: i32) -> forge_pull_requests::Model {
    forge_pull_requests::Entity::find()
        .filter(forge_pull_requests::Column::ProjectId.eq(project_id))
        .filter(forge_pull_requests::Column::Number.eq(number))
        .one(&app.state.db)
        .await
        .expect("query pull request")
        .expect("pull request row")
}

async fn pr_activities(app: &TestApp, task_id: Uuid) -> Vec<task_activities::Model> {
    task_activities::Entity::find()
        .filter(task_activities::Column::TaskId.eq(task_id))
        .filter(task_activities::Column::EventType.eq(PULL_REQUEST_LINKED_EVENT))
        .all(&app.state.db)
        .await
        .expect("query activities")
}

async fn get_forge(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    task_id: Uuid,
    token: &str,
) -> (u16, serde_json::Value) {
    let res = app
        .get_with_bearer(
            &format!("/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{task_id}/forge"),
            token,
        )
        .await;
    let status = res.status().as_u16();
    (status, res.json().await.unwrap_or(serde_json::Value::Null))
}

async fn soft_delete_task(app: &TestApp, task_id: Uuid) {
    app.state
        .db
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "UPDATE tasks SET deleted_at = now() WHERE id = $1",
            [task_id.into()],
        ))
        .await
        .expect("delete task");
}

/// タイトルと本文の `KEY-N` が、同じテナントの別プロジェクトも含めてタスクへ結ばれる。
/// 解決できない参照（不明・削除済み・別テナント）は捨て、同じ PR の有効な参照は結ぶ。
/// 同じタスクがタイトルと本文の両方にあっても表示は 1 件、履歴は 1 行。
#[tokio::test]
async fn pull_request_links_title_and_body_references() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let both = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let body_only = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;
    let deleted = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 3).await;
    soft_delete_task(&app, deleted).await;
    let second_key = format!("S{}", &key[1..]);
    let second_project = insert_project(&app, fx.tp.tenant_id, &second_key).await;
    let second_status =
        crate::forge_commit_links_integration::insert_status(&app, second_project).await;
    let second = insert_task(&app, second_project, second_status, fx.owner_id, 1).await;
    // 別テナント（キーが違う）のタスク
    let outsider = app.insert_user(false, false).await;
    let other_tp = app.insert_tenant_project(outsider.id).await;
    let other_status =
        crate::forge_commit_links_integration::insert_status(&app, other_tp.project_id).await;
    let other_task = insert_task(&app, other_tp.project_id, other_status, outsider.id, 1).await;
    let other_key = entity::projects::Entity::find_by_id(other_tp.project_id)
        .one(&app.state.db)
        .await
        .expect("query project")
        .expect("project")
        .key;

    let body =
        format!("Closes {key}-1\n{key}-2 {second_key}-1 {key}-999 {key}-3 {other_key}-1 {key}-1");
    let payload = pr_payload(
        fx.installation_id,
        "opened",
        87,
        &format!("feat: OAuth {key}-1"),
        Some(&body),
        "2026-10-01T00:00:00Z",
    );
    // 同じ配信の再送はジョブを積まない
    let delivery = Uuid::new_v4().to_string();
    assert_eq!(post_event(&app, &delivery, &payload).await, 200);
    assert_eq!(post_event(&app, &delivery, &payload).await, 200);
    let jobs = queued_jobs(&app, fx.tp.project_id).await;
    assert_eq!(jobs.len(), 1, "同じ配信の再送でジョブを積まない");
    assert!(
        jobs[0].payload.is_null(),
        "正規化したイベントのジョブにホストのペイロードを残さない"
    );
    job::github_webhook::process(jobs[0].clone(), Data::new(job_state(&app)))
        .await
        .expect("process opened");

    assert_eq!(
        links(&app, both).await,
        vec![("body".to_string(), true), ("title".to_string(), false)],
        "参照元ごとに 1 行"
    );
    assert_eq!(
        links(&app, body_only).await,
        vec![("body".to_string(), false)]
    );
    assert_eq!(links(&app, second).await, vec![("body".to_string(), false)]);
    assert!(links(&app, deleted).await.is_empty(), "削除済みは結ばない");
    assert!(
        links(&app, other_task).await.is_empty(),
        "別テナントは結ばない"
    );

    let row = pr_row(&app, fx.tp.project_id, 87).await;
    assert_eq!(row.title, format!("feat: OAuth {key}-1"));
    assert_eq!(row.state, "open");
    assert_eq!(row.author_handle, "yupix");
    assert_eq!(
        row.head_sha.as_deref(),
        Some("a3f92c1e7b81d4000000000000000000000000aa")
    );

    let activities = pr_activities(&app, both).await;
    assert_eq!(
        activities.len(),
        1,
        "タイトルと本文の両方にあっても履歴は 1 行"
    );
    assert_eq!(
        activities[0].dedupe_key.as_deref(),
        Some(format!("{PULL_REQUEST_LINKED_EVENT}:{}:{both}", row.id).as_str())
    );
    assert_eq!(
        activities[0].payload,
        serde_json::json!({
            "host": "github",
            "number": 87,
            "title": format!("feat: OAuth {key}-1"),
            "author_handle": "yupix",
            "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/87"),
            "state": "open",
            "source": "title",
        })
    );
    assert_eq!(pr_activities(&app, second).await.len(), 1);

    let token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let (status, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, both, &token).await;
    assert_eq!(status, 200);
    let prs = forge["pull_requests"].as_array().expect("pull_requests");
    assert_eq!(prs.len(), 1, "同じ PR は 1 件にまとめる");
    assert_eq!(prs[0]["number"], 87);
    assert_eq!(prs[0]["sources"], serde_json::json!(["body", "title"]));
    assert_eq!(prs[0]["closes"], true);
    assert_eq!(prs[0]["state"], "open");
    assert_eq!(prs[0]["unresolved"], serde_json::Value::Null);
    assert_eq!(forge["commits"], serde_json::json!([]));
    assert_eq!(forge["branches"], serde_json::json!([]));

    // 別プロジェクトのタスクには、そのプロジェクトの口から見える
    let (status, forge) = get_forge(&app, fx.tp.tenant_id, second_project, second, &token).await;
    assert_eq!(status, 200);
    assert_eq!(forge["pull_requests"][0]["number"], 87);

    app.cleanup_user(outsider.id).await;
    app.cleanup_user(fx.owner_id).await;
}

/// タイトルだけ・本文だけ・本文が空（null）の PR がそれぞれ出る。
/// `Opened` を受けていない PR も、最初の `Synchronize` / `Closed` で行とリンクができる。
#[tokio::test]
async fn each_action_creates_the_pull_request_and_its_links() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let at = "2026-10-01T00:00:00Z";

    let title_only = pr_payload(
        fx.installation_id,
        "opened",
        1,
        &format!("{key}-1"),
        None,
        at,
    );
    deliver(&app, &fx, &title_only).await.expect("title only");
    let body_only = pr_payload(
        fx.installation_id,
        "opened",
        2,
        "fix",
        Some(&format!("{key}-1")),
        at,
    );
    deliver(&app, &fx, &body_only).await.expect("body only");
    let synchronize = pr_payload(
        fx.installation_id,
        "synchronize",
        3,
        &format!("{key}-1"),
        Some(""),
        at,
    );
    deliver(&app, &fx, &synchronize)
        .await
        .expect("synchronize first");
    let closed = pr_payload(
        fx.installation_id,
        "closed",
        4,
        &format!("{key}-1"),
        None,
        at,
    );
    deliver(&app, &fx, &closed).await.expect("closed first");
    let merged_first = merged(
        pr_payload(
            fx.installation_id,
            "closed",
            5,
            &format!("{key}-1"),
            None,
            at,
        ),
        at,
    );
    deliver(&app, &fx, &merged_first)
        .await
        .expect("merged first");

    let token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let (status, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &token).await;
    assert_eq!(status, 200);
    let mut prs: Vec<(i64, serde_json::Value, String)> = forge["pull_requests"]
        .as_array()
        .expect("pull_requests")
        .iter()
        .map(|pr| {
            (
                pr["number"].as_i64().unwrap(),
                pr["sources"].clone(),
                pr["state"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    prs.sort_by_key(|(number, ..)| *number);
    assert_eq!(
        prs,
        vec![
            (1, serde_json::json!(["title"]), "open".to_string()),
            (2, serde_json::json!(["body"]), "open".to_string()),
            (3, serde_json::json!(["title"]), "open".to_string()),
            (4, serde_json::json!(["title"]), "closed".to_string()),
            (5, serde_json::json!(["title"]), "merged".to_string()),
        ]
    );
    // マージ時の副作用（TASK-289）はまだ行わない
    let row = pr_row(&app, fx.tp.project_id, 5).await;
    assert!(row.merged_at.is_some());
    assert_eq!(row.merge_effects_processed_at, None);
    assert_eq!(pr_activities(&app, task).await.len(), 5, "PR ごとに 1 行");

    app.cleanup_user(fx.owner_id).await;
}

/// `Edited` で本文から消した参照の `body` 行が消える。タイトルに残っていれば表示も残る。
/// 手動リンク（`manual`）は自動の再構築で消さない。履歴は残す。
#[tokio::test]
async fn edited_pull_request_rebuilds_title_and_body_links() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let kept = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let removed = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;
    let manual = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 3).await;

    let opened = pr_payload(
        fx.installation_id,
        "opened",
        10,
        &format!("feat {key}-1"),
        Some(&format!("{key}-1 {key}-2 {key}-3")),
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");
    let pull_request_id = pr_row(&app, fx.tp.project_id, 10).await.id;
    // 手動リンク（TASK-288 の API の代わりに直接入れる）
    app.state
        .db
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO forge_pull_request_links (pull_request_id, task_id, closes, source, created_at)
             VALUES ($1, $2, false, 'manual', now())",
            [pull_request_id.into(), manual.into()],
        ))
        .await
        .expect("insert manual link");

    let edited = pr_payload(
        fx.installation_id,
        "edited",
        10,
        &format!("feat {key}-1"),
        Some("本文から参照を消した"),
        "2026-10-01T00:05:00Z",
    );
    deliver(&app, &fx, &edited).await.expect("edited");

    assert_eq!(links(&app, kept).await, vec![("title".to_string(), false)]);
    assert!(links(&app, removed).await.is_empty());
    assert_eq!(
        links(&app, manual).await,
        vec![("manual".to_string(), false)]
    );
    assert_eq!(pr_activities(&app, removed).await.len(), 1, "履歴は残す");

    let token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let (_, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, kept, &token).await;
    assert_eq!(
        forge["pull_requests"][0]["sources"],
        serde_json::json!(["title"])
    );
    let (_, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, removed, &token).await;
    assert_eq!(forge["pull_requests"], serde_json::json!([]));

    app.cleanup_user(fx.owner_id).await;
}

/// 配送順の逆転と同じ更新時刻。
///
/// - マージ後に古い `Edited` が届いても `merged` のまま、タイトルもリンクも巻き戻らない（PR 取得 API は呼ばない）
/// - 同じ更新時刻のときだけ PR 取得 API で取り直し、ペイロードではなくその内容を適用する
/// - 取り直せなければ何も変えずにジョブを失敗させ、再試行で適用する
// serial: GITHUB_API_BASE_URL を差し替えるため、他の GitHub テストと並列に走らせない。
#[serial_test::serial]
#[tokio::test]
async fn pull_request_generations_do_not_roll_back() {
    let mock_server = MockServer::start().await;
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    Mock::given(method("POST"))
        .and(path_regex(r"^/app/installations/\d+/access_tokens$"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "token": "ghs_test_installation_token",
            "expires_at": "2030-01-01T00:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let first = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let second = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;
    let pulls_path = format!("/repos/{REPO_OWNER}/{REPO_NAME}/pulls/20");

    let opened = pr_payload(
        fx.installation_id,
        "opened",
        20,
        "opened",
        Some(&format!("{key}-1")),
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");
    let closed = merged(
        pr_payload(
            fx.installation_id,
            "closed",
            20,
            "merged title",
            Some(&format!("{key}-1")),
            "2026-10-01T00:10:00Z",
        ),
        "2026-10-01T00:10:00Z",
    );
    deliver(&app, &fx, &closed).await.expect("closed merged");

    // マージより前の編集が後から届く
    let stale = pr_payload(
        fx.installation_id,
        "edited",
        20,
        "stale edit",
        Some(&format!("{key}-1 {key}-2")),
        "2026-10-01T00:05:00Z",
    );
    deliver(&app, &fx, &stale).await.expect("stale edited");
    let row = pr_row(&app, fx.tp.project_id, 20).await;
    assert_eq!(row.state, "merged");
    assert_eq!(row.title, "merged title");
    assert!(
        links(&app, second).await.is_empty(),
        "古い本文のリンクを足さない"
    );
    assert!(
        mock_server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .all(|req| req.url.path() != pulls_path),
        "更新時刻が違えば PR 取得 API は呼ばない"
    );

    // マージと同じ秒の編集。ペイロード（payload title / KEY-2）ではなく API の現在の内容を使う
    let current = serde_json::json!({
        "number": 20,
        "state": "closed",
        "title": "api title",
        "body": format!("{key}-1"),
        "user": { "login": "yupix" },
        "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/20"),
        "updated_at": "2026-10-01T00:10:00Z",
        "merged_at": "2026-10-01T00:10:00Z",
        "head": { "ref": "feat/oauth", "sha": "c0ffee0000000000000000000000000000000000" }
    });
    // 1 回目は一時障害
    Mock::given(method("GET"))
        .and(path(pulls_path.clone()))
        .respond_with(ResponseTemplate::new(502))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path(pulls_path.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(current))
        .mount(&mock_server)
        .await;

    let same_second = pr_payload(
        fx.installation_id,
        "edited",
        20,
        "payload title",
        Some(&format!("{key}-2")),
        "2026-10-01T00:10:00Z",
    );
    let delivery = Uuid::new_v4().to_string();
    assert_eq!(post_event(&app, &delivery, &same_second).await, 200);
    let job = queued_jobs(&app, fx.tp.project_id)
        .await
        .into_iter()
        .find(|job| job.delivery_id.as_deref() == Some(delivery.as_str()))
        .expect("queued job");
    assert!(
        job::github_webhook::process(job.clone(), Data::new(job_state(&app)))
            .await
            .is_err(),
        "取り直せなければ推測で確定せず再試行させる"
    );
    assert_eq!(
        pr_row(&app, fx.tp.project_id, 20).await.title,
        "merged title"
    );

    job::github_webhook::process(job, Data::new(job_state(&app)))
        .await
        .expect("retry same second");
    let row = pr_row(&app, fx.tp.project_id, 20).await;
    assert_eq!(row.title, "api title");
    assert_eq!(row.state, "merged");
    assert_eq!(
        row.head_sha.as_deref(),
        Some("c0ffee0000000000000000000000000000000000")
    );
    assert_eq!(links(&app, first).await, vec![("body".to_string(), false)]);
    assert!(
        links(&app, second).await.is_empty(),
        "ペイロードにしか無い参照は入らない"
    );
    assert_eq!(
        mock_server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|req| req.url.path() == pulls_path)
            .count(),
        2,
        "同じ更新時刻のときだけ PR 取得 API を呼ぶ（失敗 1 回 + 再試行 1 回）"
    );

    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::remove_var("GITHUB_API_BASE_URL");
    }
    app.cleanup_user(fx.owner_id).await;
}

/// タスクの閲覧権限が無い呼び出し元には返さない。対照としてオーナーには返る。
#[tokio::test]
async fn task_forge_requires_access_to_the_task() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let payload = pr_payload(
        fx.installation_id,
        "opened",
        30,
        &format!("feat {}-1", fx.key),
        None,
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &payload).await.expect("opened");
    // 既存のコミットのリンクも同じ口で返す（メッセージは先頭行だけ）
    let commit = ForgeCommit {
        sha: "e7b81d4000000000000000000000000000000bb".into(),
        message: format!("fix: トークン期限切れ {}-1\n\n本文", fx.key),
        author_handle: "yupix".into(),
        author_name: "Yupix".into(),
        committed_at: chrono::Utc::now(),
        html_url: "https://github.com/acme/backend/commit/e7b81d4".into(),
    };
    apply_push(&app.state.db, fx.tp.project_id, &repo(), &[commit])
        .await
        .expect("apply push");

    let owner_token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let (status, forge) =
        get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &owner_token).await;
    assert_eq!(status, 200);
    assert_eq!(forge["pull_requests"][0]["number"], 30);
    assert_eq!(
        forge["commits"][0]["message"],
        format!("fix: トークン期限切れ {}-1", fx.key)
    );
    assert_eq!(
        forge["commits"][0]["sha"],
        "e7b81d4000000000000000000000000000000bb"
    );

    // 別テナントの利用者
    let outsider = app.insert_user(false, false).await;
    let outsider_tp = app.insert_tenant_project(outsider.id).await;
    let outsider_token = app
        .insert_pat(
            outsider.id,
            outsider_tp.tenant_id,
            vec![Scope::ReadTask],
            None,
        )
        .await;
    let (status, _) = get_forge(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        task,
        &outsider_token,
    )
    .await;
    assert_eq!(status, 403);

    // 自分のプロジェクトの口から、他プロジェクトのタスクは引けない
    let (status, _) = get_forge(
        &app,
        outsider_tp.tenant_id,
        outsider_tp.project_id,
        task,
        &outsider_token,
    )
    .await;
    assert_eq!(status, 404);

    // read:task を持たない PAT
    let no_scope = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadProject], None)
        .await;
    let (status, _) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &no_scope).await;
    assert_eq!(status, 403);

    app.cleanup_user(outsider.id).await;
    app.cleanup_user(fx.owner_id).await;
}
