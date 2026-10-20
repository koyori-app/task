//! PR のコミット集合の同期とブランチのリンク（TASK-291、docs/features/tasks/9.github-tasks.md
//! §5「PR のコミット集合の同期」/ `BranchCreated` / `BranchDeleted`、§8 の `DELETE .../forge/branches/{link_id}`）。

use std::collections::BTreeSet;

use crate::common::TestApp;
use crate::forge_commit_links_integration::{
    Fixture, REPO_NAME, REPO_OWNER, api_commit, insert_task, job_state, queued_jobs, setup, sign,
};
use crate::forge_pull_request_links_integration::{
    deliver, get_forge, github_mock, mount_github_defaults, pr_payload, pr_row, repo,
};
use apalis::prelude::Data;
use entity::{forge_commit_links, forge_commits, scopes::Scope, task_activities};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, PaginatorTrait, QueryFilter,
    Statement,
};
use service::forge::commits::COMMIT_LINKED_EVENT;
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 40 桁の SHA。`prefix` は 16 進の 1 文字で、一覧ごとに分ける
fn sha(prefix: char, n: usize) -> String {
    format!("{prefix}{n:039x}")
}

/// コミット一覧 API の `n` 件。メッセージに KEY-N は含まない
fn api_commits(prefix: char, range: std::ops::Range<usize>) -> Vec<serde_json::Value> {
    range
        .map(|n| api_commit(&sha(prefix, n), &format!("chore: {prefix}{n}")))
        .collect()
}

fn shas(commits: &[serde_json::Value]) -> BTreeSet<String> {
    commits
        .iter()
        .map(|c| c["sha"].as_str().unwrap().to_owned())
        .collect()
}

/// PR のコミット一覧を `per_page=100` のページに分けて返す。`per_page` を指定しない
/// 呼び出し（既定の 30 件）にはこのモックが当たらない
async fn mount_pr_commits(server: &MockServer, number: i32, commits: &[serde_json::Value]) {
    for (i, chunk) in commits.chunks(100).enumerate() {
        mount_page(
            server,
            number,
            i as u32 + 1,
            ResponseTemplate::new(200).set_body_json(chunk),
        )
        .await;
    }
}

async fn mount_page(server: &MockServer, number: i32, page: u32, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{number}/commits"
        )))
        .and(query_param("per_page", "100"))
        .and(query_param("page", page.to_string()))
        .respond_with(response)
        .mount(server)
        .await;
}

/// 一覧を差し替える（force-push 後の状態など）
async fn replace_pr_commits(server: &MockServer, number: i32, commits: &[serde_json::Value]) {
    server.reset().await;
    mount_github_defaults(server).await;
    mount_pr_commits(server, number, commits).await;
}

async fn pr_commit_shas(app: &TestApp, pull_request_id: Uuid) -> BTreeSet<String> {
    app.state
        .db
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT c.sha FROM forge_pull_request_commits pc
             JOIN forge_commits c ON c.id = pc.commit_id
             WHERE pc.pull_request_id = $1",
            [pull_request_id.into()],
        ))
        .await
        .expect("query pull request commits")
        .iter()
        .map(|row| row.try_get::<String>("", "sha").expect("sha"))
        .collect()
}

async fn project_commit_rows(app: &TestApp, project_id: Uuid) -> u64 {
    forge_commits::Entity::find()
        .filter(forge_commits::Column::ProjectId.eq(project_id))
        .count(&app.state.db)
        .await
        .expect("count commits")
}

async fn task_activity_count(app: &TestApp, task_id: Uuid, event_type: Option<&str>) -> u64 {
    let mut query =
        task_activities::Entity::find().filter(task_activities::Column::TaskId.eq(task_id));
    if let Some(event_type) = event_type {
        query = query.filter(task_activities::Column::EventType.eq(event_type));
    }
    query.count(&app.state.db).await.expect("count activities")
}

fn remove_github_base() {
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::remove_var("GITHUB_API_BASE_URL");
    }
}

/// `Opened` だけ（`Push` 無し）で、30 件と 100 件の両方を越える一覧を全ページ読んで中間表に入れる。
/// タスクに結ぶのは KEY-N を含むコミットだけで、`commits[]` にもそれだけが出る。
/// force-push で一覧から外れたコミットは中間表から消え、実体・リンク・アクティビティは残る。
// serial: GITHUB_API_BASE_URL を差し替えるため、他の GitHub テストと並列に走らせない。
#[serial_test::serial]
#[tokio::test]
async fn opened_syncs_every_page_and_force_push_drops_only_the_membership() {
    let mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;

    let mut listed = api_commits('a', 0..130);
    listed[0] = api_commit(&sha('a', 0), &format!("feat: 最初 {}-1", fx.key));
    listed[120] = api_commit(&sha('a', 120), &format!("fix: 2 ページ目 {}-1", fx.key));
    mount_pr_commits(&mock_server, 40, &listed).await;

    let opened = pr_payload(
        fx.installation_id,
        "opened",
        40,
        "feat",
        None,
        "2026-10-01T00:10:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");

    let pr = pr_row(&app, fx.tp.project_id, 40).await;
    assert!(pr.commits_complete, "全ページを読み切った");
    assert_eq!(pr_commit_shas(&app, pr.id).await, shas(&listed));
    assert_eq!(
        project_commit_rows(&app, fx.tp.project_id).await,
        130,
        "タスクに結ばないコミットも実体を作る"
    );
    assert_eq!(
        task_activity_count(&app, task, Some(COMMIT_LINKED_EVENT)).await,
        2
    );

    let token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let (status, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &token).await;
    assert_eq!(status, 200);
    let linked: BTreeSet<String> = forge["commits"]
        .as_array()
        .expect("commits")
        .iter()
        .map(|c| c["sha"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        linked,
        BTreeSet::from([sha('a', 0), sha('a', 120)]),
        "中間表だけの行は返さない"
    );

    // force-push で後ろの 20 件（リンク済みの a120 を含む）が外れる。
    // 古い更新時刻の Synchronize で共通処理が飛ばされても、コミット集合は現在の一覧へ同期する
    let after_force_push = listed[..110].to_vec();
    replace_pr_commits(&mock_server, 40, &after_force_push).await;
    let stale_synchronize = pr_payload(
        fx.installation_id,
        "synchronize",
        40,
        "feat",
        None,
        "2026-10-01T00:05:00Z",
    );
    deliver(&app, &fx, &stale_synchronize)
        .await
        .expect("synchronize");

    let pr = pr_row(&app, fx.tp.project_id, 40).await;
    assert!(pr.commits_complete);
    assert_eq!(pr_commit_shas(&app, pr.id).await, shas(&after_force_push));
    assert_eq!(
        project_commit_rows(&app, fx.tp.project_id).await,
        130,
        "コミットの実体は消さない"
    );
    assert_eq!(
        forge_commit_links::Entity::find()
            .filter(forge_commit_links::Column::TaskId.eq(task))
            .count(&app.state.db)
            .await
            .expect("count links"),
        2,
        "タスクへのリンクは消さない"
    );
    assert_eq!(
        task_activity_count(&app, task, Some(COMMIT_LINKED_EVENT)).await,
        2,
        "履歴は残し、増やさない"
    );
    let (_, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &token).await;
    assert_eq!(forge["commits"].as_array().expect("commits").len(), 2);

    remove_github_base();
    app.cleanup_user(fx.owner_id).await;
}

/// 2 ページ目が 5xx のときは、読めた分だけ足して既存の行を消さず `commits_complete = false`。
/// 次の同期で読み切れたら、一覧に無い行の削除まで進む。
// serial: GITHUB_API_BASE_URL を差し替えるため、他の GitHub テストと並列に走らせない。
#[serial_test::serial]
#[tokio::test]
async fn failed_page_keeps_rows_until_the_next_complete_sync() {
    let mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;

    let before = api_commits('a', 0..130);
    mount_pr_commits(&mock_server, 41, &before).await;
    let opened = pr_payload(
        fx.installation_id,
        "opened",
        41,
        "feat",
        None,
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");

    // rebase で先頭 30 件が外れ、別の 30 件が後ろに付いた。2 ページ目は一時障害
    let mut after = before[30..].to_vec();
    after.extend(api_commits('b', 0..30));
    mock_server.reset().await;
    mount_github_defaults(&mock_server).await;
    mount_page(
        &mock_server,
        41,
        1,
        ResponseTemplate::new(200).set_body_json(&after[..100]),
    )
    .await;
    mount_page(&mock_server, 41, 2, ResponseTemplate::new(502)).await;
    let synchronize = pr_payload(
        fx.installation_id,
        "synchronize",
        41,
        "feat",
        None,
        "2026-10-01T00:10:00Z",
    );
    assert!(
        deliver(&app, &fx, &synchronize).await.is_err(),
        "読み切れなければジョブを失敗させて再試行させる"
    );
    let pr = pr_row(&app, fx.tp.project_id, 41).await;
    assert!(!pr.commits_complete);
    assert_eq!(
        pr_commit_shas(&app, pr.id).await,
        shas(&before),
        "1 ページ目に無い既存の行を消さない"
    );

    replace_pr_commits(&mock_server, 41, &after).await;
    let next = pr_payload(
        fx.installation_id,
        "synchronize",
        41,
        "feat",
        None,
        "2026-10-01T00:20:00Z",
    );
    deliver(&app, &fx, &next).await.expect("next synchronize");
    let pr = pr_row(&app, fx.tp.project_id, 41).await;
    assert!(pr.commits_complete);
    assert_eq!(pr_commit_shas(&app, pr.id).await, shas(&after));

    remove_github_base();
    app.cleanup_user(fx.owner_id).await;
}

/// ホストの上限（250 件）に達した一覧は読み切っても集合を確定できないので、削除しない。
// serial: GITHUB_API_BASE_URL を差し替えるため、他の GitHub テストと並列に走らせない。
#[serial_test::serial]
#[tokio::test]
async fn host_limit_keeps_rows_and_stays_incomplete() {
    let mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;

    let before = api_commits('a', 0..130);
    mount_pr_commits(&mock_server, 42, &before).await;
    let opened = pr_payload(
        fx.installation_id,
        "opened",
        42,
        "feat",
        None,
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");

    // 上限ちょうどで頭打ちになった一覧（100 / 100 / 50）
    let capped = api_commits('c', 0..250);
    replace_pr_commits(&mock_server, 42, &capped).await;
    let synchronize = pr_payload(
        fx.installation_id,
        "synchronize",
        42,
        "feat",
        None,
        "2026-10-01T00:10:00Z",
    );
    deliver(&app, &fx, &synchronize).await.expect("synchronize");

    let pr = pr_row(&app, fx.tp.project_id, 42).await;
    assert!(!pr.commits_complete);
    let expected: BTreeSet<String> = shas(&before).union(&shas(&capped)).cloned().collect();
    assert_eq!(pr_commit_shas(&app, pr.id).await, expected);

    remove_github_base();
    app.cleanup_user(fx.owner_id).await;
}

/// 同じコミットが 2 つの PR に属するとき、両方に結ばれ、片方の同期が他方の行を消さない。
// serial: GITHUB_API_BASE_URL を差し替えるため、他の GitHub テストと並列に走らせない。
#[serial_test::serial]
#[tokio::test]
async fn shared_commit_stays_with_the_other_pull_request() {
    let mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;

    let shared = api_commit(&sha('d', 0), "chore: shared");
    let first = vec![shared.clone(), api_commit(&sha('d', 1), "chore: first")];
    let second = vec![shared, api_commit(&sha('d', 2), "chore: second")];
    mount_pr_commits(&mock_server, 50, &first).await;
    mount_pr_commits(&mock_server, 51, &second).await;
    for number in [50, 51] {
        let opened = pr_payload(
            fx.installation_id,
            "opened",
            number,
            "feat",
            None,
            "2026-10-01T00:00:00Z",
        );
        deliver(&app, &fx, &opened).await.expect("opened");
    }
    let first_pr = pr_row(&app, fx.tp.project_id, 50).await.id;
    let second_pr = pr_row(&app, fx.tp.project_id, 51).await.id;
    assert_eq!(pr_commit_shas(&app, first_pr).await, shas(&first));
    assert_eq!(pr_commit_shas(&app, second_pr).await, shas(&second));
    assert_eq!(
        project_commit_rows(&app, fx.tp.project_id).await,
        3,
        "同じ SHA は 1 行"
    );

    // 2 つ目の PR から共有コミットが外れる
    let rebased = second[1..].to_vec();
    replace_pr_commits(&mock_server, 51, &rebased).await;
    let synchronize = pr_payload(
        fx.installation_id,
        "synchronize",
        51,
        "feat",
        None,
        "2026-10-01T00:10:00Z",
    );
    deliver(&app, &fx, &synchronize).await.expect("synchronize");
    assert_eq!(pr_commit_shas(&app, second_pr).await, shas(&rebased));
    assert_eq!(
        pr_commit_shas(&app, first_pr).await,
        shas(&first),
        "他の PR の行は消さない"
    );

    remove_github_base();
    app.cleanup_user(fx.owner_id).await;
}

/// GitHub の create / delete ペイロード
fn ref_payload(installation_id: i64, ref_type: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "ref": name,
        "ref_type": ref_type,
        "pusher_type": "user",
        "repository": {
            "name": REPO_NAME,
            "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}"),
            "owner": { "login": REPO_OWNER }
        },
        "installation": { "id": installation_id },
        "sender": { "login": "yupix", "id": 20 }
    })
}

/// webhook を受け、積まれたジョブをワーカーと同じ関数で処理する
async fn deliver_ref_event(app: &TestApp, fx: &Fixture, event: &str, payload: &serde_json::Value) {
    let delivery = Uuid::new_v4().to_string();
    let body = serde_json::to_vec(payload).expect("serialize body");
    let status = app
        .client()
        .post(format!("{}/v1/github/webhook", app.base_url()))
        .header("content-type", "application/json")
        .header("X-GitHub-Event", event)
        .header("X-GitHub-Delivery", &delivery)
        .header("X-Hub-Signature-256", sign(&body))
        .body(body)
        .send()
        .await
        .expect("post webhook")
        .status();
    assert_eq!(status.as_u16(), 200);
    let job = queued_jobs(app, fx.tp.project_id)
        .await
        .into_iter()
        .find(|job| job.delivery_id.as_deref() == Some(delivery.as_str()))
        .expect("queued job");
    job::github_webhook::process(job, Data::new(job_state(app)))
        .await
        .expect("process ref event");
}

/// ブランチ名の KEY-N でリンクが付き、ブランチ削除で消える。タグは無視し、アクティビティは積まない。
#[tokio::test]
async fn branch_create_links_and_delete_unlinks() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 12).await;
    let branch = format!("feat/{}-12-foo", fx.key);

    deliver_ref_event(
        &app,
        &fx,
        "create",
        &ref_payload(fx.installation_id, "tag", &format!("{}-12", fx.key)),
    )
    .await;
    deliver_ref_event(
        &app,
        &fx,
        "create",
        &ref_payload(fx.installation_id, "branch", &branch),
    )
    .await;
    // 同じブランチの作成が再び届いても 1 行
    deliver_ref_event(
        &app,
        &fx,
        "create",
        &ref_payload(fx.installation_id, "branch", &branch),
    )
    .await;

    let token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let (status, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &token).await;
    assert_eq!(status, 200);
    let branches = forge["branches"].as_array().expect("branches");
    assert_eq!(branches.len(), 1, "タグは結ばない・重複しない");
    assert_eq!(branches[0]["branch"], branch.as_str());
    assert_eq!(
        branches[0]["html_url"],
        format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/tree/{branch}")
    );
    assert_eq!(branches[0]["repo_owner"], REPO_OWNER);
    assert_eq!(task_activity_count(&app, task, None).await, 0);

    deliver_ref_event(
        &app,
        &fx,
        "delete",
        &ref_payload(fx.installation_id, "branch", &branch),
    )
    .await;
    let (_, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &token).await;
    assert_eq!(forge["branches"], serde_json::json!([]));

    app.cleanup_user(fx.owner_id).await;
}

async fn delete_branch_link(
    app: &TestApp,
    fx: &Fixture,
    task_id: Uuid,
    link_id: &str,
    token: &str,
) -> u16 {
    app.client()
        .delete(format!(
            "{}/v1/tenants/{}/projects/{}/tasks/{task_id}/forge/branches/{link_id}",
            app.base_url(),
            fx.tp.tenant_id,
            fx.tp.project_id
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("delete branch link")
        .status()
        .as_u16()
}

/// ブランチのリンクを外す API。他タスクの link は 404、`write:task` が無ければ 403。
#[tokio::test]
async fn delete_branch_link_requires_the_task_and_write_scope() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let other = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;
    let branch = format!("feat/{}-1", fx.key);
    service::forge::branches::apply_branch_created(
        &app.state.db,
        fx.tp.project_id,
        &repo(),
        &branch,
        "https://github.com/acme/backend/tree/feat",
    )
    .await
    .expect("link branch");

    let read = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let write = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;
    let (_, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &read).await;
    let link_id = forge["branches"][0]["id"]
        .as_str()
        .expect("link id")
        .to_owned();

    assert_eq!(
        delete_branch_link(&app, &fx, other, &link_id, &write).await,
        404,
        "他タスクの link は外せない"
    );
    assert_eq!(
        delete_branch_link(&app, &fx, task, &link_id, &read).await,
        403
    );
    assert_eq!(
        delete_branch_link(&app, &fx, task, &link_id, &write).await,
        204
    );
    let (_, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &read).await;
    assert_eq!(forge["branches"], serde_json::json!([]));
    assert_eq!(
        delete_branch_link(&app, &fx, task, &link_id, &write).await,
        404
    );
    assert_eq!(task_activity_count(&app, task, None).await, 0);

    app.cleanup_user(fx.owner_id).await;
}
