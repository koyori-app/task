//! レビューと PR 行の結線（TASK-264、docs/features/tasks/9.github-tasks.md §6 / §8、
//! docs/features/review-findings.md §5）。PR の状態、未解決件数、`pr_merged` 通知。

use crate::common::{TestApp, ensure_tenant_member_for_project, json_body};
use crate::forge_commit_links_integration::{Fixture, insert_task, setup};
use crate::forge_manual_pull_request_links_integration::{forget_github_mock, github_mock, link};
use crate::forge_pull_request_links_integration::{deliver, get_forge, merged, pr_payload, pr_row};
use entity::{notifications, scopes::Scope, task_assignees, task_watchers};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

const HEAD_SHA: &str = "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e";

/// `token` の利用者としてラウンドを 1 本起票する。`severities` の数だけ指摘を付ける。
async fn submit_round(app: &TestApp, fx: &Fixture, token: &str, pr: i32, severities: &[&str]) {
    let findings: Vec<_> = severities
        .iter()
        .map(
            |severity| serde_json::json!({ "severity": severity, "title": "指摘", "body": "根拠" }),
        )
        .collect();
    let res = app
        .post_json_with_bearer(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews",
                fx.tp.tenant_id, fx.tp.project_id
            ),
            serde_json::json!({
                "pr_number": pr, "head_sha": HEAD_SHA, "summary": "総評", "findings": findings,
            }),
            token,
        )
        .await;
    let status = res.status().as_u16();
    assert_eq!(
        status,
        201,
        "ラウンドの起票は成功する: {}",
        res.text().await.unwrap_or_default()
    );
}

async fn reviewed_pull_requests(app: &TestApp, fx: &Fixture, token: &str) -> serde_json::Value {
    let res = app
        .get_with_bearer(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews/pull-requests",
                fx.tp.tenant_id, fx.tp.project_id
            ),
            token,
        )
        .await;
    assert_eq!(res.status().as_u16(), 200);
    json_body(res).await
}

async fn pr_merged_notifications(app: &TestApp, user_id: Uuid) -> Vec<notifications::Model> {
    notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(user_id))
        .filter(notifications::Column::NotificationType.eq("pr_merged"))
        .all(&app.state.db)
        .await
        .expect("query notifications")
}

/// CLI の `review submit` が作った `unknown` の PR 行に webhook が合流し、マージで `merged` になる。
/// `pr_merged` はレビュワー・担当者・ウォッチャーに 1 人 1 通だけ届き、再送や後続イベントで増えない。
#[tokio::test]
async fn merge_notifies_reviewers_and_task_people_once() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let reviewer = app.insert_user(false, false).await;
    let assignee = app.insert_user(false, false).await;
    let watcher = app.insert_user(false, false).await;
    for user in [reviewer.id, assignee.id, watcher.id] {
        ensure_tenant_member_for_project(&app.state.db, fx.tp.project_id, user).await;
    }
    task_assignees::ActiveModel {
        id: Set(Uuid::new_v4()),
        task_id: Set(task),
        user_id: Set(assignee.id),
        role: Set("assignee".into()),
        assigned_at: Set(chrono::Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert assignee");
    // レビュワーはウォッチャーも兼ねる（1 通にまとまること）
    for user in [watcher.id, reviewer.id, assignee.id] {
        task_watchers::ActiveModel {
            task_id: Set(task),
            user_id: Set(user),
            created_at: Set(chrono::Utc::now().into()),
        }
        .insert(&app.state.db)
        .await
        .expect("insert watcher");
    }

    // webhook より先に CLI からレビューが出る
    let reviewer_token = app
        .insert_pat(reviewer.id, fx.tp.tenant_id, vec![Scope::WriteReview], None)
        .await;
    submit_round(&app, &fx, &reviewer_token, 87, &["high"]).await;
    submit_round(&app, &fx, &reviewer_token, 87, &[]).await;
    let created = pr_row(&app, fx.tp.project_id, 87).await;
    assert_eq!(created.state, "unknown");
    assert_eq!(created.host_updated_at, None);
    let listed = reviewed_pull_requests(&app, &fx, &reviewer_token).await;
    assert_eq!(listed[0]["state"], "unknown");
    assert_eq!(listed[0]["pull_request_id"], created.id.to_string());

    let opened = pr_payload(
        fx.installation_id,
        "opened",
        87,
        "feat: OAuth",
        Some(&format!("Closes {key}-1")),
        "2026-10-01T00:00:00Z",
    );
    deliver(&app, &fx, &opened).await.expect("process opened");
    let joined = pr_row(&app, fx.tp.project_id, 87).await;
    assert_eq!(joined.id, created.id, "webhook は同じ PR 行に合流する");
    assert_eq!(joined.state, "open");
    assert!(pr_merged_notifications(&app, reviewer.id).await.is_empty());

    let closed = merged(
        pr_payload(
            fx.installation_id,
            "closed",
            87,
            "feat: OAuth",
            Some(&format!("Closes {key}-1")),
            "2026-10-02T00:00:00Z",
        ),
        "2026-10-02T00:00:00Z",
    );
    deliver(&app, &fx, &closed).await.expect("process closed");
    // マージ後に届いた閉じたイベントや編集では送り直さない
    let mut closed_again = closed.clone();
    closed_again["pull_request"]["updated_at"] = "2026-10-02T12:00:00Z".into();
    deliver(&app, &fx, &closed_again)
        .await
        .expect("process closed again");
    let edited = pr_payload(
        fx.installation_id,
        "edited",
        87,
        "feat: OAuth (edited)",
        Some(&format!("Closes {key}-1")),
        "2026-10-03T00:00:00Z",
    );
    deliver(&app, &fx, &edited).await.expect("process edited");

    assert_eq!(pr_row(&app, fx.tp.project_id, 87).await.state, "merged");
    let listed = reviewed_pull_requests(&app, &fx, &reviewer_token).await;
    assert_eq!(listed[0]["state"], "merged");
    assert_eq!(listed[0]["merged_at"], "2026-10-02T00:00:00Z");
    assert!(
        listed[0]["merge_effects_processed_at"].is_string(),
        "Closed {{ merged: true }} でマージ時副作用を処理済みになる"
    );

    for user in [reviewer.id, assignee.id, watcher.id] {
        let received = pr_merged_notifications(&app, user).await;
        assert_eq!(received.len(), 1, "関係者には 1 回だけ届く");
        assert_eq!(received[0].task_id, Some(task));
        assert_eq!(
            received[0].payload,
            serde_json::json!({
                "number": 87,
                "title": "feat: OAuth",
                "html_url": "https://github.com/acme/backend/pull/87",
                "repo_owner": "acme",
                "repo_name": "backend",
            })
        );
    }
    assert!(
        pr_merged_notifications(&app, fx.owner_id).await.is_empty(),
        "関係のない人には届かない"
    );

    // テナントごとレビューを消してから（レビュワーの行はレビューに参照されている）
    for user in [fx.owner_id, reviewer.id, assignee.id, watcher.id] {
        app.cleanup_user(user).await;
    }
}

/// タスクに結ばれていない PR でも、レビュワーにはタスク無しの通知が届く。
#[tokio::test]
async fn merge_of_unlinked_pull_request_notifies_reviewers() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let reviewer = app.insert_user(false, false).await;
    ensure_tenant_member_for_project(&app.state.db, fx.tp.project_id, reviewer.id).await;
    let token = app
        .insert_pat(reviewer.id, fx.tp.tenant_id, vec![Scope::WriteReview], None)
        .await;
    submit_round(&app, &fx, &token, 90, &["low"]).await;

    let closed = merged(
        pr_payload(
            fx.installation_id,
            "closed",
            90,
            "chore: 掃除",
            None,
            "2026-10-02T00:00:00Z",
        ),
        "2026-10-02T00:00:00Z",
    );
    deliver(&app, &fx, &closed).await.expect("process closed");

    let received = pr_merged_notifications(&app, reviewer.id).await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].task_id, None);
    assert_eq!(received[0].project_id, Some(fx.tp.project_id));

    app.cleanup_user(fx.owner_id).await;
    app.cleanup_user(reviewer.id).await;
}

/// `unresolved` は High / Medium の未解決件数。`read:review` の無い PAT とレビューの無い PR は null。
#[tokio::test]
async fn unresolved_counts_blocking_findings_only_with_review_scope() {
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let key = &fx.key;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    for number in [87, 88, 89] {
        let opened = pr_payload(
            fx.installation_id,
            "opened",
            number,
            &format!("{key}-1"),
            None,
            "2026-10-01T00:00:00Z",
        );
        deliver(&app, &fx, &opened).await.expect("process opened");
    }
    let writer = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::WriteReview], None)
        .await;
    // 87: High + Medium + Low → 2 件。89: 指摘なしのラウンド → 0 件。88: レビュー無し → null
    submit_round(&app, &fx, &writer, 87, &["high", "low"]).await;
    submit_round(&app, &fx, &writer, 87, &["medium"]).await;
    submit_round(&app, &fx, &writer, 89, &[]).await;

    let unresolved = |forge: &serde_json::Value, number: i32| {
        forge["pull_requests"]
            .as_array()
            .expect("pull_requests")
            .iter()
            .find(|pr| pr["number"] == number)
            .expect("pull request")["unresolved"]
            .clone()
    };

    let task_only = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let (status, forge) =
        get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &task_only).await;
    assert_eq!(status, 200);
    assert_eq!(unresolved(&forge, 87), serde_json::Value::Null);

    for scopes in [
        vec![Scope::ReadTask, Scope::ReadReview],
        vec![Scope::ReadTask, Scope::WriteReview],
    ] {
        let token = app
            .insert_pat(fx.owner_id, fx.tp.tenant_id, scopes, None)
            .await;
        let (status, forge) =
            get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &token).await;
        assert_eq!(status, 200);
        assert_eq!(unresolved(&forge, 87), 2);
        assert_eq!(unresolved(&forge, 88), serde_json::Value::Null);
        assert_eq!(unresolved(&forge, 89), 0);
    }

    app.cleanup_user(fx.owner_id).await;
}

/// 手動リンクの 201 も `GET /forge` と同じ集計の `unresolved` を返す（`read:review` が無ければ null）。
#[tokio::test]
async fn manual_link_response_carries_unresolved() {
    let _mock_server = github_mock().await;
    let app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    let task = insert_task(&app, fx.tp.project_id, fx.status_id, fx.owner_id, 1).await;
    let writer = app
        .insert_pat(fx.owner_id, fx.tp.tenant_id, vec![Scope::WriteReview], None)
        .await;
    submit_round(&app, &fx, &writer, 87, &["high", "medium", "low"]).await;

    for (scopes, expected) in [
        (
            vec![Scope::ReadTask, Scope::WriteTask],
            serde_json::Value::Null,
        ),
        (
            vec![Scope::ReadTask, Scope::WriteTask, Scope::ReadReview],
            serde_json::json!(2),
        ),
    ] {
        let token = app
            .insert_pat(fx.owner_id, fx.tp.tenant_id, scopes, None)
            .await;
        let (status, pr) = link(
            &app,
            fx.tp.tenant_id,
            fx.tp.project_id,
            task,
            serde_json::json!({ "number": 87 }),
            &token,
        )
        .await;
        assert_eq!(status, 201, "{pr}");
        assert_eq!(pr["unresolved"], expected, "手動リンクの応答");
        let (status, forge) =
            get_forge(&app, fx.tp.tenant_id, fx.tp.project_id, task, &token).await;
        assert_eq!(status, 200);
        assert_eq!(
            forge["pull_requests"][0]["unresolved"], expected,
            "GET の PR 行と同じ値"
        );
    }

    forget_github_mock();
    app.cleanup_user(fx.owner_id).await;
}
