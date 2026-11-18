//! ジョブを積む道がどれも、最大試行回数を `job::DEFAULT_MAX_ATTEMPTS` に決めて行を書くことを確かめる。
//!
//! apalis-postgres 1.0.0-rc.9 以降は、積む側が決めなければ既定の 25 で積まれる（rc.8 までは 5）。
//! リトライの上限が 5 倍になっても動作は壊れず、気づきにくい。

use job::{
    AlreadyRegisteredEmailJob, GithubIssueSyncJob, GithubWebhookJob, PasswordResetEmailJob,
    ReviewSummaryJob, already_registered_email, github_issue_sync, github_webhook,
    password_reset_email, review_summary, tenant_invitation_email, verification_email,
};
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use serde_json::json;
use uuid::Uuid;

use crate::common::TestApp;

async fn max_attempts_of(app: &TestApp, queue: &str) -> Vec<i32> {
    app.state
        .db
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT max_attempts FROM apalis.jobs WHERE job_type = $1",
            [queue.into()],
        ))
        .await
        .expect("select jobs")
        .iter()
        .map(|row| row.try_get("", "max_attempts").expect("max_attempts"))
        .collect()
}

/// DB はテスト同士で共有するので、行数は下限だけを見て、キューの行がすべて決めた値であることを縛る。
async fn assert_capped(app: &TestApp, queue: &str, at_least: usize) {
    let expected = i32::try_from(job::DEFAULT_MAX_ATTEMPTS).expect("fits in i32");
    let found = max_attempts_of(app, queue).await;
    assert!(found.len() >= at_least, "{queue}: {found:?}");
    assert!(found.iter().all(|&n| n == expected), "{queue}: {found:?}");
}

#[tokio::test]
async fn verification_email_is_enqueued_with_capped_attempts() {
    let app = TestApp::new().await;
    verification_email::enqueue(
        &app.state.verification_email_storage,
        verification_email::VerificationEmailJob::new(Uuid::new_v4(), "a@example.com".into()),
    )
    .await
    .expect("enqueue");
    assert_capped(&app, verification_email::QUEUE_NAME, 1).await;
}

#[tokio::test]
async fn password_reset_email_is_enqueued_with_capped_attempts() {
    let app = TestApp::new().await;
    password_reset_email::enqueue(
        &app.state.password_reset_email_storage,
        PasswordResetEmailJob::new(Uuid::new_v4(), "a@example.com".into()),
    )
    .await
    .expect("enqueue");
    assert_capped(&app, password_reset_email::QUEUE_NAME, 1).await;
}

#[tokio::test]
async fn already_registered_email_is_enqueued_with_capped_attempts() {
    let app = TestApp::new().await;
    already_registered_email::enqueue(
        &app.state.already_registered_email_storage,
        AlreadyRegisteredEmailJob::new("a@example.com".into()),
    )
    .await
    .expect("enqueue");
    assert_capped(&app, already_registered_email::QUEUE_NAME, 1).await;
}

#[tokio::test]
async fn github_issue_sync_is_enqueued_with_capped_attempts() {
    let app = TestApp::new().await;
    github_issue_sync::enqueue(
        &app.state.github_issue_sync_storage,
        GithubIssueSyncJob::Push {
            task_id: Uuid::new_v4(),
        },
    )
    .await
    .expect("enqueue");
    assert_capped(&app, github_issue_sync::QUEUE_NAME, 1).await;
}

#[tokio::test]
async fn review_summary_is_enqueued_with_capped_attempts() {
    let app = TestApp::new().await;
    review_summary::enqueue(
        &app.state.review_summary_storage,
        ReviewSummaryJob {
            project_id: Uuid::new_v4(),
            pr_number: 1,
            repo_owner: "acme".into(),
            repo_name: "backend".into(),
        },
    )
    .await
    .expect("enqueue");
    assert_capped(&app, review_summary::QUEUE_NAME, 1).await;
}

fn webhook_job() -> GithubWebhookJob {
    GithubWebhookJob {
        integration_id: Uuid::new_v4(),
        project_id: Uuid::new_v4(),
        event: "ping".into(),
        delivery_id: None,
        payload: serde_json::Value::Null,
        forge_event: None,
        backfill_page: None,
    }
}

#[tokio::test]
async fn github_webhook_is_enqueued_with_capped_attempts() {
    let app = TestApp::new().await;
    github_webhook::enqueue(&app.state.github_webhook_storage, webhook_job())
        .await
        .expect("enqueue");
    assert_capped(&app, github_webhook::QUEUE_NAME, 1).await;
}

/// 受信記録と同じトランザクションで積む道は、storage を通らず行を直接書く。
#[tokio::test]
async fn github_webhook_delivery_is_enqueued_with_capped_attempts() {
    let app = TestApp::new().await;
    let recorded = github_webhook::enqueue_delivery(
        &app.state.pg_pool,
        "github.com",
        "https://github.com",
        &Uuid::new_v4().to_string(),
        vec![webhook_job(), webhook_job()],
    )
    .await
    .expect("enqueue delivery");
    assert!(recorded);
    assert_capped(&app, github_webhook::QUEUE_NAME, 2).await;
}

/// 招待の行と同じトランザクションで積む道も、storage を通らず行を直接書く。
#[tokio::test]
async fn tenant_invitation_email_is_enqueued_with_capped_attempts() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;

    let res = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/invitations", tp.tenant_id),
            json!({ "email": format!("invitee-{}@example.com", Uuid::new_v4()), "role": "Member" }),
        )
        .await;
    assert_eq!(res.status(), reqwest::StatusCode::CREATED);
    assert_capped(&app, tenant_invitation_email::QUEUE_NAME, 1).await;
}
