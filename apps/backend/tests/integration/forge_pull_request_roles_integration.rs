//! PR の役割（実装 / 言及）と、その手の上書き（TASK-525、
//! docs/dev/features/tasks/github-tasks/index.md の「タスクへの自動リンク」）。
//!
//! - 自動の判定: 題の末尾の `(KEY-N)`・クローズキーワード・手動リンクの「マージしたら完了」が実装、ほかは言及
//! - 手の上書きは PR イベントで戻らず、リンクを外すと一緒に消える
//! - 役割は表示だけに使い、マージ時の自動完了は変えない
//! - `PUT …/role` の 200 / 400 / 403 / 404
//! - `head_branch`、コミットの `body` と `pull_requests`、ブランチの `pull_requests`

use crate::common::TestApp;
use crate::forge_commit_links_integration::{
    Fixture, REPO_NAME, REPO_OWNER, api_commit, insert_project, insert_task, job_state,
    queued_jobs, setup, sign,
};
use crate::forge_manual_pull_request_links_integration::link;
use crate::forge_pull_request_links_integration::{
    deliver, get_forge, github_mock, merged, pr_payload, pr_row, repo,
};
use apalis::prelude::Data;
use entity::{forge_pull_request_roles, project_statuses, scopes::Scope, tasks};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use service::forge::commits::apply_push;
use service::forge::events::ForgeCommit;
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

fn forget_github_mock() {
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::remove_var("GITHUB_API_BASE_URL");
    }
}

async fn set_role(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    task_id: Uuid,
    pull_request_id: &str,
    role: &str,
    token: &str,
) -> (u16, serde_json::Value) {
    let res = app
        .put_json_with_bearer(
            &format!(
                "/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{task_id}/forge/pull-requests/{pull_request_id}/role"
            ),
            serde_json::json!({ "role": role }),
            token,
        )
        .await;
    let status = res.status().as_u16();
    (status, res.json().await.unwrap_or(serde_json::Value::Null))
}

async fn unlink(app: &TestApp, fx: &Fixture, task_id: Uuid, pull_request_id: &str, token: &str) {
    let status = app
        .client()
        .delete(format!(
            "{}/v1/tenants/{}/projects/{}/tasks/{task_id}/forge/pull-requests/{pull_request_id}",
            app.base_url(),
            fx.tp.tenant_id,
            fx.tp.project_id
        ))
        .bearer_auth(token)
        .send()
        .await
        .expect("delete request")
        .status()
        .as_u16();
    assert_eq!(status, 204, "unlink");
}

/// タスクの forge の応答から、番号の PR を引く
async fn pull_request(
    app: &TestApp,
    fx: &Fixture,
    task_id: Uuid,
    number: i32,
    token: &str,
) -> serde_json::Value {
    let (status, forge) = get_forge(app, fx.tp.tenant_id, fx.tp.project_id, task_id, token).await;
    assert_eq!(status, 200);
    forge["pull_requests"]
        .as_array()
        .expect("pull_requests")
        .iter()
        .find(|pr| pr["number"] == number)
        .unwrap_or_else(|| panic!("pull request #{number} is not linked: {forge}"))
        .clone()
}

async fn role_rows(app: &TestApp, task_id: Uuid) -> Vec<forge_pull_request_roles::Model> {
    forge_pull_request_roles::Entity::find()
        .filter(forge_pull_request_roles::Column::TaskId.eq(task_id))
        .all(&app.state.db)
        .await
        .expect("query roles")
}

async fn token(app: &TestApp, fx: &Fixture) -> String {
    app.insert_pat(
        fx.owner_id,
        fx.tp.tenant_id,
        vec![Scope::ReadTask, Scope::WriteTask],
        None,
    )
    .await
}

/// 題の末尾の `(KEY-N)` とクローズキーワードは実装、本文や題の途中の言及とブランチ名だけは言及。
/// 末尾の括弧に並んだ別のタスクも、それぞれの実装になる。
#[serial_test::serial]
#[tokio::test]
async fn role_is_decided_from_the_title_tail_and_close_keywords() {
    let _mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let token = token(&app, &fx).await;
    let mut tasks = Vec::new();
    for seq_id in 1..=6 {
        tasks.push(insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, seq_id).await);
    }

    for (number, title, body, head) in [
        (
            11,
            format!("feat: 末尾 ({key}-1, {key}-6)"),
            None,
            "feat/a".to_owned(),
        ),
        (
            12,
            "feat: 本文".into(),
            Some(format!("{key}-2 を見て")),
            "feat/b".into(),
        ),
        (
            13,
            "feat: 閉じる".into(),
            Some(format!("Closes {key}-3")),
            "feat/c".into(),
        ),
        (
            14,
            "feat: ブランチ".into(),
            None,
            format!("feat/{key}-4-foo"),
        ),
        (15, format!("{key}-5 を直す"), None, "feat/e".into()),
    ] {
        let mut payload = pr_payload(
            fx.installation_id,
            "opened",
            number,
            &title,
            body.as_deref(),
            "2026-10-01T00:00:00Z",
        );
        payload["pull_request"]["head"]["ref"] = head.into();
        deliver(&app, &fx, &payload).await.expect("opened");
    }

    for (task, number, role) in [
        (tasks[0], 11, "implementation"),
        (tasks[5], 11, "implementation"),
        (tasks[1], 12, "mention"),
        (tasks[2], 13, "implementation"),
        (tasks[3], 14, "mention"),
        (tasks[4], 15, "mention"),
    ] {
        let pr = pull_request(&app, &fx, task, number, &token).await;
        assert_eq!(pr["role"], role, "#{number}");
    }
    // 判定は応答のたびに計算する。保存しない
    for task in &tasks {
        assert!(role_rows(&app, *task).await.is_empty());
    }
    // head のブランチ名も返る
    let pr = pull_request(&app, &fx, tasks[3], 14, &token).await;
    assert_eq!(pr["head_branch"], format!("feat/{key}-4-foo"));

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}

/// 手の上書きは自動の判定に勝ち、同じ題の PR イベントが後から届いても戻らない。
/// 役割は表示だけで、言及の PR を実装にしてもマージでタスクは完了しない。
#[serial_test::serial]
#[tokio::test]
async fn override_survives_pull_request_events_and_does_not_change_merge_effects() {
    let _mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let token = token(&app, &fx).await;
    // 完了系のステータスがあっても完了しないことを見る
    project_statuses::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(fx.tp.project_id),
        name: Set("Done".into()),
        color: Set("#22c55e".into()),
        position: Set(9),
        is_default: Set(false),
        is_done_state: Set(true),
        is_default_done: Set(true),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert done status");
    let implemented = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let mentioned = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;

    let title = format!("feat: x ({key}-1)");
    let body = format!("{key}-2 に関係する");
    let event = |action: &str, updated_at: &str| {
        pr_payload(
            fx.installation_id,
            action,
            21,
            &title,
            Some(&body),
            updated_at,
        )
    };
    deliver(&app, &fx, &event("opened", "2026-10-01T00:00:00Z"))
        .await
        .expect("opened");
    let pull_request_id = pr_row(&app, fx.tp.project_id, 21).await.id.to_string();
    assert_eq!(
        pull_request(&app, &fx, implemented, 21, &token).await["role"],
        "implementation"
    );
    assert_eq!(
        pull_request(&app, &fx, mentioned, 21, &token).await["role"],
        "mention"
    );

    // 自動で実装の PR を言及に、言及の PR を実装に入れ替える。応答は入れ替えた後の PR
    let (status, changed) = set_role(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        implemented,
        &pull_request_id,
        "mention",
        &token,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(changed["role"], "mention");
    assert_eq!(changed["number"], 21);
    let (status, changed) = set_role(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        mentioned,
        &pull_request_id,
        "implementation",
        &token,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(changed["role"], "implementation");
    // 上書きは表示だけ。自動完了の旗は変えない
    assert_eq!(changed["closes"], false);

    // 同じ題・本文の新しいイベント
    deliver(&app, &fx, &event("edited", "2026-10-01T00:10:00Z"))
        .await
        .expect("edited");
    assert_eq!(
        pull_request(&app, &fx, implemented, 21, &token).await["role"],
        "mention"
    );
    assert_eq!(
        pull_request(&app, &fx, mentioned, 21, &token).await["role"],
        "implementation"
    );

    // 同じ値でもう一度上書きしても 1 行のまま（後勝ちの UPSERT）
    let (status, _) = set_role(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        mentioned,
        &pull_request_id,
        "implementation",
        &token,
    )
    .await;
    assert_eq!(status, 200);
    let rows = role_rows(&app, mentioned).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].set_by, Some(fx.owner_id));

    // マージしても、クローズキーワードの無いタスクは完了しない
    let closed = merged(
        event("closed", "2026-10-01T00:20:00Z"),
        "2026-10-01T00:20:00Z",
    );
    deliver(&app, &fx, &closed).await.expect("merged");
    for task in [implemented, mentioned] {
        let row = tasks::Entity::find_by_id(task)
            .one(&app.state.db)
            .await
            .expect("query task")
            .expect("task row");
        assert_eq!(row.status_id, fx.status_id);
        assert!(row.completed_at.is_none());
    }
    assert_eq!(
        pull_request(&app, &fx, mentioned, 21, &token).await["role"],
        "implementation"
    );

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}

/// リンクを外すと上書きも消える。自動リンクが戻ったら、役割は自動の判定になる。
#[serial_test::serial]
#[tokio::test]
async fn unlink_drops_the_override() {
    let _mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let token = token(&app, &fx).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let title = format!("feat: x ({key}-1)");
    let event = |action: &str, updated_at: &str| {
        pr_payload(fx.installation_id, action, 31, &title, None, updated_at)
    };
    deliver(&app, &fx, &event("opened", "2026-10-01T00:00:00Z"))
        .await
        .expect("opened");
    let pull_request_id = pr_row(&app, fx.tp.project_id, 31).await.id.to_string();

    let (status, _) = set_role(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        task,
        &pull_request_id,
        "mention",
        &token,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(role_rows(&app, task).await.len(), 1);

    unlink(&app, &fx, task, &pull_request_id, &token).await;
    assert!(role_rows(&app, task).await.is_empty(), "上書きも消える");
    // リンクが無い PR は上書きできない
    let (status, _) = set_role(
        &app,
        fx.tp.tenant_id,
        fx.tp.project_id,
        task,
        &pull_request_id,
        "mention",
        &token,
    )
    .await;
    assert_eq!(status, 404);
    assert!(role_rows(&app, task).await.is_empty());

    deliver(&app, &fx, &event("edited", "2026-10-01T00:10:00Z"))
        .await
        .expect("edited");
    assert_eq!(
        pull_request(&app, &fx, task, 31, &token).await["role"],
        "implementation",
        "前の上書き（言及）は残っていない"
    );

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}

/// リンクの無い PR・他プロジェクトのタスクは 404、`write:task` が無ければ 403、知らない値は 400。
/// 対照として、同じ PR への正しい要求は 200。拒否した要求は行を作らない。
#[serial_test::serial]
#[tokio::test]
async fn set_role_authorization_and_validation() {
    let _mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let token = token(&app, &fx).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let unlinked = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;
    deliver(
        &app,
        &fx,
        &pr_payload(
            fx.installation_id,
            "opened",
            41,
            "feat: x",
            Some(&format!("{key}-1")),
            "2026-10-01T00:00:00Z",
        ),
    )
    .await
    .expect("opened");
    let pull_request_id = pr_row(&app, fx.tp.project_id, 41).await.id.to_string();
    let put = |project_id: Uuid,
               task_id: Uuid,
               pull_request_id: String,
               role: &'static str,
               token: String| {
        let app = &app;
        let tenant_id = fx.tp.tenant_id;
        async move {
            set_role(
                app,
                tenant_id,
                project_id,
                task_id,
                &pull_request_id,
                role,
                &token,
            )
            .await
        }
    };

    let (status, _) = put(
        fx.tp.project_id,
        task,
        Uuid::new_v4().to_string(),
        "implementation",
        token.clone(),
    )
    .await;
    assert_eq!(status, 404, "存在しない PR");
    let (status, _) = put(
        fx.tp.project_id,
        unlinked,
        pull_request_id.clone(),
        "implementation",
        token.clone(),
    )
    .await;
    assert_eq!(status, 404, "このタスクにリンクされていない PR");

    let second_key = format!("S{}", &key[1..]);
    let second_project = insert_project(&app, fx.tp.tenant_id, &second_key).await;
    let (status, _) = put(
        second_project,
        task,
        pull_request_id.clone(),
        "implementation",
        token.clone(),
    )
    .await;
    assert_eq!(status, 404, "他プロジェクトのタスク");

    let read_only = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let (status, _) = put(
        fx.tp.project_id,
        task,
        pull_request_id.clone(),
        "implementation",
        read_only,
    )
    .await;
    assert_eq!(status, 403, "write:task が無い");

    let outsider = app.insert_user(false, false).await;
    let outsider_token = app
        .insert_pat(outsider.id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;
    let (status, _) = put(
        fx.tp.project_id,
        task,
        pull_request_id.clone(),
        "implementation",
        outsider_token,
    )
    .await;
    assert_eq!(status, 403, "テナントのメンバー外");

    let (status, error) = put(
        fx.tp.project_id,
        task,
        pull_request_id.clone(),
        "x",
        token.clone(),
    )
    .await;
    assert_eq!(status, 400, "知らない役割: {error}");
    assert!(
        role_rows(&app, task).await.is_empty(),
        "拒否した要求は行を作らない"
    );
    assert_eq!(
        pull_request(&app, &fx, task, 41, &token).await["role"],
        "mention"
    );

    let (status, changed) = put(
        fx.tp.project_id,
        task,
        pull_request_id.clone(),
        "implementation",
        token.clone(),
    )
    .await;
    assert_eq!(status, 200, "対照: 正しい要求");
    assert_eq!(changed["role"], "implementation");

    app.cleanup_user(outsider.id).await;
    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}

/// 手動リンクは「マージしたら完了にする」のチェックで役割が決まる。
#[serial_test::serial]
#[tokio::test]
async fn manual_link_role_follows_closes() {
    let _mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let token = token(&app, &fx).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;

    // PR 取得 API はモックに無い（404）ので、PR は `state = unknown` で作られる
    for (number, closes, role) in [(51, true, "implementation"), (52, false, "mention")] {
        let (status, linked) = link(
            &app,
            fx.tp.tenant_id,
            fx.tp.project_id,
            task,
            serde_json::json!({ "number": number, "closes": closes }),
            &token,
        )
        .await;
        assert_eq!(status, 201);
        assert_eq!(linked["role"], role, "#{number}");
        assert_eq!(linked["head_branch"], serde_json::Value::Null);
        assert_eq!(
            pull_request(&app, &fx, task, number, &token).await["role"],
            role
        );
    }

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}

/// コミットは本文と、それを含む PR を返す。ブランチは、それを head にしている PR を返す。
#[serial_test::serial]
#[tokio::test]
async fn commits_and_branches_carry_their_pull_requests() {
    let mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let token = token(&app, &fx).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let branch = format!("feat/{key}-1-foo");

    // タスクに結ばれたコミット（push 由来）。PR のコミット一覧にも入っている
    let linked_sha = format!("a{:039x}", 7);
    let message = format!("fix: トークン期限切れ {key}-1\n\n期限の比較が逆だった。\n2 行目\n");
    apply_push(
        &app.state.db,
        fx.tp.project_id,
        &repo(),
        &[ForgeCommit {
            sha: linked_sha.clone(),
            message: message.clone(),
            author_handle: "yupix".into(),
            author_name: "Yupix".into(),
            committed_at: chrono::Utc::now(),
            html_url: format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/commit/{linked_sha}"),
        }],
    )
    .await
    .expect("apply push");
    // 本文の無いコミット。どの PR にも入っていない
    let lone_sha = format!("b{:039x}", 1);
    apply_push(
        &app.state.db,
        fx.tp.project_id,
        &repo(),
        &[ForgeCommit {
            sha: lone_sha.clone(),
            message: format!("chore: {key}-1"),
            author_handle: "yupix".into(),
            author_name: "Yupix".into(),
            committed_at: chrono::Utc::now() - chrono::Duration::hours(1),
            html_url: format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/commit/{lone_sha}"),
        }],
    )
    .await
    .expect("apply push");

    // GitHub のページの既定（30 件）を越える件数。境界ちょうどだとページングの欠落を隠す
    let mut commits: Vec<serde_json::Value> = (0..34)
        .map(|n| api_commit(&format!("c{n:039x}"), &format!("chore: c{n}")))
        .collect();
    commits.push(api_commit(&linked_sha, &message));
    for number in [61, 62] {
        Mock::given(method("GET"))
            .and(path(format!(
                "/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{number}/commits"
            )))
            .and(query_param("per_page", "100"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&commits))
            .mount(&mock_server)
            .await;
    }
    for (number, head) in [
        (62, branch.clone()),
        (61, branch.clone()),
        (63, "main2".into()),
        (64, branch.clone()),
    ] {
        let mut payload = pr_payload(
            fx.installation_id,
            "opened",
            number,
            "feat: x",
            None,
            "2026-10-01T00:00:00Z",
        );
        payload["pull_request"]["head"]["ref"] = head.into();
        if number == 64 {
            // フォークの同名ブランチからの PR
            payload["pull_request"]["head"]["repo"] =
                serde_json::json!({ "name": REPO_NAME, "owner": { "login": "someone" } });
        }
        deliver(&app, &fx, &payload).await.expect("opened");
    }

    // ブランチの作成
    let delivery = Uuid::new_v4().to_string();
    let body = serde_json::to_vec(&serde_json::json!({
        "ref": branch,
        "ref_type": "branch",
        "pusher_type": "user",
        "repository": {
            "name": REPO_NAME,
            "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}"),
            "owner": { "login": REPO_OWNER }
        },
        "installation": { "id": fx.installation_id },
        "sender": { "login": "yupix", "id": 20 }
    }))
    .expect("serialize body");
    let status = app
        .client()
        .post(format!("{}/v1/github/webhook", app.base_url()))
        .header("content-type", "application/json")
        .header("X-GitHub-Event", "create")
        .header("X-GitHub-Delivery", &delivery)
        .header("X-Hub-Signature-256", sign(&body))
        .body(body)
        .send()
        .await
        .expect("post webhook")
        .status();
    assert_eq!(status.as_u16(), 200);
    let job = queued_jobs(&app, fx.tp.project_id)
        .await
        .into_iter()
        .find(|job| job.delivery_id.as_deref() == Some(delivery.as_str()))
        .expect("queued job");
    job::github_webhook::process(job, Data::new(job_state(&app)))
        .await
        .expect("process create event");

    let (status, forge) = get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &token).await;
    assert_eq!(status, 200);

    let commits = forge["commits"].as_array().expect("commits");
    assert_eq!(commits.len(), 2, "{forge}");
    let linked = commits
        .iter()
        .find(|c| c["sha"] == linked_sha.as_str())
        .expect("linked commit");
    assert_eq!(linked["message"], format!("fix: トークン期限切れ {key}-1"));
    assert_eq!(linked["body"], "期限の比較が逆だった。\n2 行目");
    let numbers = |value: &serde_json::Value| -> Vec<i64> {
        value["pull_requests"]
            .as_array()
            .expect("pull_requests")
            .iter()
            .map(|pr| pr["number"].as_i64().expect("number"))
            .collect()
    };
    assert_eq!(numbers(linked), vec![61, 62], "番号の昇順");
    assert_eq!(linked["pull_requests"][0]["state"], "open");
    assert_eq!(
        linked["pull_requests"][0]["html_url"],
        format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/61")
    );
    let lone = commits
        .iter()
        .find(|c| c["sha"] == lone_sha.as_str())
        .expect("lone commit");
    assert_eq!(lone["body"], "");
    assert_eq!(numbers(lone), Vec::<i64>::new());

    let branches = forge["branches"].as_array().expect("branches");
    assert_eq!(branches.len(), 1, "{forge}");
    assert_eq!(branches[0]["branch"], branch.as_str());
    // 別のブランチを head にする #63 も、フォークの同名ブランチからの #64 も入らない
    assert_eq!(numbers(&branches[0]), vec![61, 62]);

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}
