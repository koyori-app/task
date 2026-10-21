//! マージ時の自動完了と確認済み操作（TASK-289、docs/features/tasks/9.github-tasks.md
//! §5「マージ時副作用の独立処理」/ §8 `merge-effects/acknowledge` / §11）。

use crate::common::TestApp;
use crate::forge_commit_links_integration::{
    Fixture, REPO_NAME, REPO_OWNER, insert_project, insert_status, insert_task, setup,
};
use crate::forge_manual_pull_request_links_integration::{forget_github_mock, github_mock};
use crate::forge_pull_request_links_integration::{
    deliver, links, merged, mount_github_defaults, pr_payload, pr_row,
};
use entity::{
    github_issue_links, project_members, project_statuses, scopes::Scope, task_activities, tasks,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait,
    QueryFilter, Statement,
};
use service::forge::events::ForgeRepo;
use service::forge::merge_effects::{
    MERGE_EFFECTS_ACKNOWLEDGED_EVENT, PULL_REQUEST_MERGED_EVENT, apply_merge_effects,
};
use service::forge::pull_requests::PullRequestSnapshot;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 完了系ステータスを 2 つ足す。`is_default_done` の方（"Done"）を返し、もう片方は "Closed"。
async fn insert_done_statuses(app: &TestApp, project_id: Uuid) -> (Uuid, Uuid) {
    let mut ids = Vec::new();
    // 既定でない方を先（position が小さい）に置き、position より is_default_done が優先されることを見る
    for (name, position, is_default_done) in [("Closed", 1, false), ("Done", 2, true)] {
        let status = project_statuses::ActiveModel {
            id: Set(Uuid::new_v4()),
            project_id: Set(project_id),
            name: Set(name.into()),
            color: Set("#22c55e".into()),
            position: Set(position),
            is_default: Set(false),
            is_done_state: Set(true),
            is_default_done: Set(is_default_done),
            created_at: Set(chrono::Utc::now().into()),
        }
        .insert(&app.state.db)
        .await
        .expect("insert done status");
        ids.push(status.id);
    }
    (ids[1], ids[0])
}

async fn task_row(app: &TestApp, task_id: Uuid) -> tasks::Model {
    tasks::Entity::find_by_id(task_id)
        .one(&app.state.db)
        .await
        .expect("query task")
        .expect("task row")
}

async fn set_status(app: &TestApp, task_id: Uuid, status_id: Uuid) {
    let mut active: tasks::ActiveModel = task_row(app, task_id).await.into();
    active.status_id = Set(status_id);
    active.update(&app.state.db).await.expect("update status");
}

async fn activities(app: &TestApp, task_id: Uuid, event_type: &str) -> Vec<task_activities::Model> {
    task_activities::Entity::find()
        .filter(task_activities::Column::TaskId.eq(task_id))
        .filter(task_activities::Column::EventType.eq(event_type))
        .all(&app.state.db)
        .await
        .expect("query activities")
}

async fn insert_manual_link(app: &TestApp, pull_request_id: Uuid, task_id: Uuid, created_at: &str) {
    app.state
        .db
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO forge_pull_request_links (pull_request_id, task_id, closes, source, created_at)
             VALUES ($1, $2, true, 'manual', $3::timestamptz)",
            [pull_request_id.into(), task_id.into(), created_at.into()],
        ))
        .await
        .expect("insert manual link");
}

/// マージ後の状態を伝える `Edited`（GitHub はマージ済み PR の編集でも `merged_at` を載せる）。
/// 取得 API のスナップショットだけで初めて `merged` を観測する場合と同じ扱いになる。
fn edited_after_merge(
    fx: &Fixture,
    number: i32,
    body: &str,
    merged_at: &str,
    updated_at: &str,
) -> serde_json::Value {
    let mut payload = merged(
        pr_payload(
            fx.installation_id,
            "edited",
            number,
            "edited after merge",
            Some(body),
            updated_at,
        ),
        merged_at,
    );
    payload["pull_request"]["state"] = "closed".into();
    payload
}

async fn acknowledge(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    pull_request_id: Uuid,
    token: &str,
) -> u16 {
    app.client()
        .post(format!(
            "{}/v1/tenants/{tenant_id}/projects/{project_id}/forge/pull-requests/{pull_request_id}/merge-effects/acknowledge",
            app.base_url()
        ))
        .bearer_auth(token)
        .send()
        .await
        .expect("post acknowledge")
        .status()
        .as_u16()
}

/// マージ時点の参照のうち `closes` が true のタスクだけを、プロジェクトの既定の完了ステータスへ
/// 一度だけ遷移させる。
///
/// - 既に完了のタスク・`closes` の無い参照・マージ後に足した手動リンクは触らない
/// - マージ前の手動リンクは対象にする。完了ステータスの無いプロジェクトのタスクは飛ばす
/// - 同じ `Closed { merged: true }` の再送（同じ更新時刻なので PR 取得 API を経る）でも遷移は 1 回
// serial: GITHUB_API_BASE_URL を差し替えるため、他の GitHub テストと並列に走らせない。
#[serial_test::serial]
#[tokio::test]
async fn merged_pull_request_completes_closing_tasks_once() {
    let mock_server = MockServer::start().await;
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    // installation token と、`Opened` が引く PR のコミット一覧
    mount_github_defaults(&mock_server).await;

    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let (done, closed_status) = insert_done_statuses(&app, fx.tp.project_id).await;
    let closing = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let mentioned = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;
    let already_done = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 3).await;
    set_status(&app, already_done, closed_status).await;
    let manual_before = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 4).await;
    let manual_after = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 5).await;
    // タイトルでは closes 無し、本文では closes 有り（bool_or で完了対象）
    let mixed = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 6).await;
    // 完了系ステータスの無いプロジェクト
    let second_key = format!("S{}", &key[1..]);
    let second_project = insert_project(&app, fx.tp.tenant_id, &second_key).await;
    let second_status = insert_status(&app, second_project).await;
    let no_done = insert_task(&app, second_project, second_status, fx.owner_id, 1).await;

    let title = format!("feat {key}-6");
    let body =
        format!("Closes {key}-1\n{key}-2\nCloses {key}-3\nCloses {key}-6\nCloses {second_key}-1");
    let opened = pr_payload(
        fx.installation_id,
        "opened",
        40,
        &title,
        Some(&body),
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");
    let pull_request_id = pr_row(&app, fx.tp.project_id, 40).await.id;
    insert_manual_link(&app, pull_request_id, manual_before, "2026-10-01T00:05:00Z").await;
    insert_manual_link(&app, pull_request_id, manual_after, "2026-10-01T00:20:00Z").await;
    assert_eq!(task_row(&app, closing).await.status_id, fx.status_id);

    let closed = merged(
        pr_payload(
            fx.installation_id,
            "closed",
            40,
            &title,
            Some(&body),
            "2026-10-01T00:10:00Z",
        ),
        "2026-10-01T00:10:00Z",
    );
    deliver(&app, &fx, &closed).await.expect("closed merged");

    for task in [closing, manual_before, mixed] {
        let row = task_row(&app, task).await;
        assert_eq!(row.status_id, done, "既定の完了ステータスへ遷移する");
        assert!(row.completed_at.is_some());
    }
    assert_eq!(
        task_row(&app, mentioned).await.status_id,
        fx.status_id,
        "closes の無い参照は触らない"
    );
    assert_eq!(
        task_row(&app, already_done).await.status_id,
        closed_status,
        "既に完了なら遷移しない"
    );
    assert_eq!(
        task_row(&app, manual_after).await.status_id,
        fx.status_id,
        "マージ後の手動リンクは過去のマージの対象にしない"
    );
    assert_eq!(
        task_row(&app, no_done).await.status_id,
        second_status,
        "完了ステータスの無いプロジェクトのタスクは飛ばす"
    );
    for task in [mentioned, already_done, manual_after, no_done] {
        assert!(activities(&app, task, "status_changed").await.is_empty());
        assert!(
            activities(&app, task, PULL_REQUEST_MERGED_EVENT)
                .await
                .is_empty()
        );
    }

    let changed = activities(&app, closing, "status_changed").await;
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].user_id, None, "システムによる変更");
    assert_eq!(
        changed[0].payload,
        serde_json::json!({ "from": "Todo", "to": "Done" })
    );
    let merged_activity = activities(&app, closing, PULL_REQUEST_MERGED_EVENT).await;
    assert_eq!(merged_activity.len(), 1);
    assert_eq!(
        merged_activity[0].dedupe_key.as_deref(),
        Some(format!("{PULL_REQUEST_MERGED_EVENT}:{pull_request_id}:{closing}").as_str())
    );
    assert_eq!(
        merged_activity[0].payload,
        serde_json::json!({
            "host": "github",
            "number": 40,
            "title": title,
            "author_handle": "yupix",
            "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/40"),
            "state": "merged",
        })
    );
    let row = pr_row(&app, fx.tp.project_id, 40).await;
    let processed_at = row
        .merge_effects_processed_at
        .expect("merge effects processed");
    assert_eq!(row.merge_effects_acknowledged_by, None);

    // 同じイベントの再送。更新時刻が同じなので PR 取得 API の現在の内容で共通処理を行う
    Mock::given(method("GET"))
        .and(path(format!("/repos/{REPO_OWNER}/{REPO_NAME}/pulls/40")))
        .respond_with(ResponseTemplate::new(200).set_body_json(closed["pull_request"].clone()))
        .mount(&mock_server)
        .await;
    // 再送前に完了を手で戻しても、副作用は処理済みなので再び完了させない
    set_status(&app, closing, fx.status_id).await;
    deliver(&app, &fx, &closed).await.expect("redelivered");
    assert_eq!(task_row(&app, closing).await.status_id, fx.status_id);
    assert_eq!(activities(&app, mixed, "status_changed").await.len(), 1);
    assert_eq!(
        activities(&app, mixed, PULL_REQUEST_MERGED_EVENT)
            .await
            .len(),
        1
    );
    assert_eq!(
        pr_row(&app, fx.tp.project_id, 40)
            .await
            .merge_effects_processed_at,
        Some(processed_at),
        "処理時刻は一度だけ設定する"
    );

    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::remove_var("GITHUB_API_BASE_URL");
    }
    app.cleanup_user(fx.owner_id).await;
}

/// 同じ PR のマージ時副作用を 2 つの処理が同時に走らせても、遷移と処理時刻の設定は 1 回。
/// （並行するジョブ 2 つの代わりに、副作用の処理を同時に 2 本走らせる）
#[tokio::test]
async fn concurrent_merge_effects_run_once() {
    // `Opened` は PR のコミット一覧を引く
    let _mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    insert_done_statuses(&app, fx.tp.project_id).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let body = format!("Closes {key}-1");
    let opened = pr_payload(
        fx.installation_id,
        "opened",
        41,
        "feat",
        Some(&body),
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");

    let at = "2026-10-01T00:10:00Z".parse().expect("timestamp");
    let event = PullRequestSnapshot {
        title: "feat".into(),
        body,
        author_handle: "yupix".into(),
        state: "closed".into(),
        merged_at: Some(at),
        head_sha: "a3f92c1e7b81d4000000000000000000000000aa".into(),
        head_branch: "feat/oauth".into(),
        html_url: format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/41"),
        updated_at: at,
    };
    let repo = ForgeRepo {
        host: "github".into(),
        host_url: "https://github.com".into(),
        repo_owner: REPO_OWNER.into(),
        repo_name: REPO_NAME.into(),
    };
    let (first, second) = tokio::join!(
        apply_merge_effects(&app.state.db, fx.tp.project_id, &repo, 41, &event),
        apply_merge_effects(&app.state.db, fx.tp.project_id, &repo, 41, &event),
    );
    let processed = [first.expect("first"), second.expect("second")];
    assert_eq!(
        processed.iter().filter(|done| done.is_some()).count(),
        1,
        "片方だけが処理する"
    );
    assert_eq!(activities(&app, task, "status_changed").await.len(), 1);
    assert_eq!(
        activities(&app, task, PULL_REQUEST_MERGED_EVENT)
            .await
            .len(),
        1
    );

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}

/// マージ後の編集が先着する。マージ時本文は `Closes K-1`、マージ後本文は `Closes K-2`。
///
/// 新しい `Edited` を先に処理すると表示（PR 行・リンク）は編集後になり、副作用は未処理のまま。
/// 後着した古い `Closed` は表示を巻き戻さず、イベント本文の K-1 だけを完了する。
#[tokio::test]
async fn late_merge_event_completes_tasks_from_its_own_body() {
    // `Opened` は PR のコミット一覧を引く
    let _mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let (done, _) = insert_done_statuses(&app, fx.tp.project_id).await;
    let at_merge = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let after_merge = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;

    let opened = pr_payload(
        fx.installation_id,
        "opened",
        42,
        "feat",
        Some(&format!("Closes {key}-1")),
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");
    let edited = edited_after_merge(
        &fx,
        42,
        &format!("Closes {key}-2"),
        "2026-10-01T00:10:00Z",
        "2026-10-01T00:20:00Z",
    );
    deliver(&app, &fx, &edited)
        .await
        .expect("edited after merge");
    let row = pr_row(&app, fx.tp.project_id, 42).await;
    assert_eq!(row.state, "merged", "スナップショットのマージは表示に出す");
    assert_eq!(
        row.merge_effects_processed_at, None,
        "Closed を受けるまで副作用はしない"
    );
    assert_eq!(task_row(&app, after_merge).await.status_id, fx.status_id);

    let closed = merged(
        pr_payload(
            fx.installation_id,
            "closed",
            42,
            "feat",
            Some(&format!("Closes {key}-1")),
            "2026-10-01T00:10:00Z",
        ),
        "2026-10-01T00:10:00Z",
    );
    deliver(&app, &fx, &closed).await.expect("late closed");

    assert_eq!(task_row(&app, at_merge).await.status_id, done);
    assert_eq!(
        task_row(&app, after_merge).await.status_id,
        fx.status_id,
        "マージ後の本文の参照は完了させない"
    );
    let row = pr_row(&app, fx.tp.project_id, 42).await;
    assert_eq!(row.title, "edited after merge", "表示は編集後のまま");
    assert!(row.merge_effects_processed_at.is_some());
    assert!(
        links(&app, at_merge).await.is_empty(),
        "リンクを巻き戻さない"
    );
    assert_eq!(
        links(&app, after_merge).await,
        vec![("body".to_string(), true)]
    );

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}

/// 確認済み操作。タスクは完了させず、処理時刻と操作者を残してリンク先に履歴を積む。
///
/// - 200 / 409（確認済み・処理済み・merged でない）/ 404（別プロジェクトのパス）/ 403（write:task 無し）
/// - 確認済みの PR に後から `Closed` が届いても完了しない。対照として未確認の PR は完了する
/// - リンク先が 0 件の PR でも成功する
#[tokio::test]
async fn acknowledge_merge_effects_without_completing_tasks() {
    // `Opened` は PR のコミット一覧を引く
    let _mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let (done, _) = insert_done_statuses(&app, fx.tp.project_id).await;
    let acknowledged_task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let control_task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;
    insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 3).await;
    let second_project = insert_project(&app, fx.tp.tenant_id, &format!("S{}", &key[1..])).await;

    // 50: 確認済みにする PR、51: 対照、52: 未マージ、53: リンク先なし
    for (number, body) in [
        (50, format!("Closes {key}-1")),
        (51, format!("Closes {key}-2")),
        (53, "no refs".to_string()),
    ] {
        let edited = edited_after_merge(
            &fx,
            number,
            &body,
            "2026-10-01T00:10:00Z",
            "2026-10-01T00:20:00Z",
        );
        deliver(&app, &fx, &edited).await.expect("merged snapshot");
    }
    let opened = pr_payload(
        fx.installation_id,
        "opened",
        52,
        "open",
        Some(&format!("Closes {key}-3")),
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");
    let pr50 = pr_row(&app, fx.tp.project_id, 50).await.id;
    let pr51 = pr_row(&app, fx.tp.project_id, 51).await.id;
    let pr52 = pr_row(&app, fx.tp.project_id, 52).await.id;
    let pr53 = pr_row(&app, fx.tp.project_id, 53).await.id;

    let writer = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;
    let reader = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let (tenant, project) = (fx.tp.tenant_id, fx.tp.project_id);

    assert_eq!(
        acknowledge(&app, tenant, project, pr50, &reader).await,
        403,
        "タスク編集権限が無い"
    );
    assert_eq!(
        acknowledge(&app, tenant, second_project, pr50, &writer).await,
        404,
        "パスのプロジェクトの PR ではない"
    );
    assert_eq!(
        acknowledge(&app, tenant, project, Uuid::new_v4(), &writer).await,
        404
    );
    assert_eq!(
        pr_row(&app, project, 50).await.merge_effects_processed_at,
        None
    );

    assert_eq!(acknowledge(&app, tenant, project, pr50, &writer).await, 200);
    let row = pr_row(&app, project, 50).await;
    assert!(row.merge_effects_processed_at.is_some());
    assert_eq!(row.merge_effects_acknowledged_by, Some(fx.owner_id));
    assert_eq!(
        task_row(&app, acknowledged_task).await.status_id,
        fx.status_id,
        "タスクは完了させない"
    );
    let acknowledged = activities(&app, acknowledged_task, MERGE_EFFECTS_ACKNOWLEDGED_EVENT).await;
    assert_eq!(acknowledged.len(), 1);
    assert_eq!(acknowledged[0].user_id, Some(fx.owner_id));
    assert_eq!(
        acknowledged[0].dedupe_key.as_deref(),
        Some(format!("{MERGE_EFFECTS_ACKNOWLEDGED_EVENT}:{pr50}:{acknowledged_task}").as_str())
    );
    assert_eq!(
        acknowledged[0].payload,
        serde_json::json!({
            "host": "github",
            "pull_request_id": pr50,
            "number": 50,
            "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/pull/50"),
        })
    );

    assert_eq!(
        acknowledge(&app, tenant, project, pr50, &writer).await,
        409,
        "確認済み"
    );
    assert_eq!(
        activities(&app, acknowledged_task, MERGE_EFFECTS_ACKNOWLEDGED_EVENT)
            .await
            .len(),
        1
    );
    assert_eq!(
        acknowledge(&app, tenant, project, pr52, &writer).await,
        409,
        "merged でない"
    );
    assert_eq!(acknowledge(&app, tenant, project, pr53, &writer).await, 200);
    assert_eq!(
        pr_row(&app, project, 53)
            .await
            .merge_effects_acknowledged_by,
        Some(fx.owner_id),
        "リンク先が無くても操作者は PR 行に残る"
    );

    // 元の Closed が後から届く。確認済みの PR は完了させず、未確認の PR は完了させる
    for number in [50, 51] {
        let closed = merged(
            pr_payload(
                fx.installation_id,
                "closed",
                number,
                "feat",
                Some(&format!("Closes {key}-{}", number - 49)),
                "2026-10-01T00:10:00Z",
            ),
            "2026-10-01T00:10:00Z",
        );
        deliver(&app, &fx, &closed).await.expect("late closed");
    }
    assert_eq!(
        task_row(&app, acknowledged_task).await.status_id,
        fx.status_id
    );
    assert!(
        activities(&app, acknowledged_task, PULL_REQUEST_MERGED_EVENT)
            .await
            .is_empty()
    );
    assert_eq!(task_row(&app, control_task).await.status_id, done);
    assert_eq!(
        acknowledge(&app, tenant, project, pr51, &writer).await,
        409,
        "処理済み"
    );
    assert_eq!(
        pr_row(&app, project, 51)
            .await
            .merge_effects_acknowledged_by,
        None
    );

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}

async fn add_project_member(app: &TestApp, project_id: Uuid, user_id: Uuid) {
    crate::common::ensure_tenant_member_for_project(&app.state.db, project_id, user_id).await;
    project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        user_id: Set(user_id),
        role: Set(project_members::ProjectRole::Member),
    }
    .insert(&app.state.db)
    .await
    .expect("add project member");
}

/// 確認済みにするとリンク先の全タスクの自動完了が止まり、そのタスクに履歴が積まれるので、
/// パスのプロジェクトに加えてリンク先の全プロジェクトの編集権限が要る（§8 権限）。
///
/// - A にだけ入れる利用者は、B のタスクにもリンクした PR を確認済みにできない（403、何も変わらない）
/// - 対照: 同じ利用者でも A のタスクだけにリンクした PR は 200、A と B の両方に入れるオーナーは 200
#[tokio::test]
async fn acknowledge_requires_access_to_every_linked_task_project() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let a_task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let b_key = format!("S{}", &key[1..]);
    let b_project = insert_project(&app, fx.tp.tenant_id, &b_key).await;
    let b_status = insert_status(&app, b_project).await;
    let b_task = insert_task(&app, b_project, b_status, fx.owner_id, 1).await;
    let member = app.insert_user(false, false).await;
    add_project_member(&app, fx.tp.project_id, member.id).await;
    // B はオーナーだけを明示メンバーにして閉じる
    add_project_member(&app, b_project, fx.owner_id).await;

    // 70: A と B のタスクにリンク、71: A のタスクだけにリンク
    for (number, body) in [
        (70, format!("Closes {key}-1\nCloses {b_key}-1")),
        (71, format!("Closes {key}-1")),
    ] {
        let edited = edited_after_merge(
            &fx,
            number,
            &body,
            "2026-10-01T00:10:00Z",
            "2026-10-01T00:20:00Z",
        );
        deliver(&app, &fx, &edited).await.expect("merged snapshot");
    }
    let pr70 = pr_row(&app, fx.tp.project_id, 70).await.id;
    let pr71 = pr_row(&app, fx.tp.project_id, 71).await.id;
    let member_token = app
        .insert_pat(member.id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;
    let owner_token = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;
    let (tenant, project) = (fx.tp.tenant_id, fx.tp.project_id);

    assert_eq!(
        acknowledge(&app, tenant, project, pr70, &member_token).await,
        403,
        "B のタスクの自動完了を止められない"
    );
    assert_eq!(
        pr_row(&app, project, 70).await.merge_effects_processed_at,
        None
    );
    for task in [a_task, b_task] {
        assert!(
            activities(&app, task, MERGE_EFFECTS_ACKNOWLEDGED_EVENT)
                .await
                .is_empty()
        );
    }

    assert_eq!(
        acknowledge(&app, tenant, project, pr71, &member_token).await,
        200,
        "入れるプロジェクトのタスクだけなら確認済みにできる"
    );
    assert_eq!(
        acknowledge(&app, tenant, project, pr70, &owner_token).await,
        200
    );
    let acknowledged = activities(&app, b_task, MERGE_EFFECTS_ACKNOWLEDGED_EVENT).await;
    assert_eq!(acknowledged.len(), 1);
    assert_eq!(acknowledged[0].user_id, Some(fx.owner_id));

    app.cleanup_user(member.id).await;
    app.cleanup_user(fx.owner_id).await;
}

async fn insert_issue_link(app: &TestApp, project_id: Uuid, task_id: Uuid, number: i32) {
    app.state
        .db
        .execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO github_issue_links (id, project_id, integration_id, task_id, github_number,
                 synced_hash, github_updated_at, pending_push, updated_at, github_state)
             SELECT $1, $2, i.id, $3, $4, 'synced', now(), false, now(), 'open'
             FROM github_integrations i WHERE i.project_id = $2",
            [
                Uuid::new_v4().into(),
                project_id.into(),
                task_id.into(),
                number.into(),
            ],
        ))
        .await
        .expect("insert issue link");
}

async fn pending_push(app: &TestApp, task_id: Uuid) -> bool {
    github_issue_links::Entity::find()
        .filter(github_issue_links::Column::TaskId.eq(task_id))
        .one(&app.state.db)
        .await
        .expect("query issue link")
        .expect("issue link")
        .pending_push
}

/// 積まれている書き戻しジョブの件数。ペイロードは JSON なので task_id で絞る
async fn queued_push_jobs(app: &TestApp, task_id: Uuid) -> i64 {
    app.state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT COUNT(*) AS count FROM apalis.jobs \
             WHERE job_type = $1 AND convert_from(job, 'UTF8') LIKE $2",
            [
                job::github_issue_sync::QUEUE_NAME.into(),
                format!("%{task_id}%").into(),
            ],
        ))
        .await
        .expect("count jobs")
        .expect("count row")
        .try_get("", "count")
        .expect("count")
}

/// GitHub Issue から取り込んだタスクをマージで完了にしたら、通常のタスク更新と同じく
/// 書き戻しの印（`pending_push`）を同じトランザクションで立て、コミット後に書き戻しを積む。
/// GitHub は `Closes KEY-N` を Issue 番号として読まないので、書き戻さないと Issue が開いたまま残る。
/// 対照: 参照だけで完了させないタスクには印を立てない
#[tokio::test]
async fn merge_completion_pushes_back_to_the_source_issue() {
    // `Opened` は PR のコミット一覧を引く
    let _mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let (done, _) = insert_done_statuses(&app, fx.tp.project_id).await;
    let closing = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let mentioned = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 2).await;
    insert_issue_link(&app, fx.tp.project_id, closing, 11).await;
    insert_issue_link(&app, fx.tp.project_id, mentioned, 12).await;

    let body = format!("Closes {key}-1\n{key}-2");
    let opened = pr_payload(
        fx.installation_id,
        "opened",
        80,
        "feat",
        Some(&body),
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("opened");
    let closed = merged(
        pr_payload(
            fx.installation_id,
            "closed",
            80,
            "feat",
            Some(&body),
            "2026-10-01T00:10:00Z",
        ),
        "2026-10-01T00:10:00Z",
    );
    deliver(&app, &fx, &closed).await.expect("closed merged");

    assert_eq!(task_row(&app, closing).await.status_id, done);
    assert!(
        pending_push(&app, closing).await,
        "完了を取り込み元 Issue へ書き戻す印"
    );
    assert_eq!(queued_push_jobs(&app, closing).await, 1);
    assert_eq!(task_row(&app, mentioned).await.status_id, fx.status_id);
    assert!(!pending_push(&app, mentioned).await);
    assert_eq!(queued_push_jobs(&app, mentioned).await, 0);

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}
