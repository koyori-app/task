//! GitHub Webhook イベント処理ジョブ。

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use apalis::prelude::{
    BackoffConfig, BoxDynError, Data, IntervalStrategy, StrategyBuilder, TaskSink,
};
use apalis_postgres::{Config, JsonCodec, PgPool, PgTask, PostgresStorage};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use common::settings::Settings;
use service::forge::events::ForgeEvent;

use crate::JobState;

pub const QUEUE_NAME: &str = "github_webhook";
pub const MAX_RETRIES: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubWebhookJob {
    pub integration_id: Uuid,
    pub project_id: Uuid,
    pub event: String,
    pub delivery_id: Option<String>,
    /// 正規化していないイベントのペイロード。`forge_event` があるときは持たない（Null）
    pub payload: serde_json::Value,
    /// 受信口で正規化したイベント。これを足す前に積まれたジョブには無い
    #[serde(default)]
    pub forge_event: Option<ForgeEvent>,
}

pub type GithubWebhookStorage = PostgresStorage<
    GithubWebhookJob,
    apalis_postgres::CompactType,
    JsonCodec<apalis_postgres::CompactType>,
    apalis_postgres::PgNotify,
>;

pub fn build_storage(pool: &PgPool, _settings: &Settings) -> GithubWebhookStorage {
    let config = Config::new(QUEUE_NAME).with_poll_interval(
        StrategyBuilder::new()
            .apply(
                IntervalStrategy::new(Duration::from_secs(2))
                    .with_backoff(BackoffConfig::default()),
            )
            .build(),
    );
    PostgresStorage::new_with_notify(pool, &config)
}

pub async fn setup(
    pool: &PgPool,
    settings: &Settings,
) -> Result<Arc<GithubWebhookStorage>, anyhow::Error> {
    PostgresStorage::setup(pool).await?;
    Ok(Arc::new(build_storage(pool, settings)))
}

pub async fn enqueue(
    storage: &GithubWebhookStorage,
    job: GithubWebhookJob,
) -> Result<(), anyhow::Error> {
    let mut storage = storage.clone();
    storage
        .push(job)
        .await
        .map_err(|e| anyhow::anyhow!("push github webhook job: {e}"))?;
    Ok(())
}

/// 受信記録とジョブ投入を 1 トランザクションで確定する
/// （docs/features/tasks/9.github-tasks.md §5「冪等性設計」）。
///
/// 同じ配信の受信記録がすでにあれば何も積まず `false` を返す（ホストの再送）。
/// ジョブ投入に失敗したら受信記録ごとロールバックするので、再送は初回として処理される。
/// 呼ぶのは署名検証を通した後に限る（先に記録すると配信 ID を先取りされる）。
pub async fn enqueue_delivery(
    pool: &PgPool,
    host: &str,
    host_url: &str,
    delivery_id: &str,
    jobs: Vec<GithubWebhookJob>,
) -> Result<bool, anyhow::Error> {
    let mut tx = pool.begin().await?;
    let recorded = sqlx::query(
        "INSERT INTO forge_webhook_deliveries (id, host, host_url, delivery_id, created_at)
         VALUES (gen_random_uuid(), $1, $2, $3, now())
         ON CONFLICT (host, host_url, delivery_id) DO NOTHING",
    )
    .bind(host)
    .bind(host_url)
    .bind(delivery_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !recorded {
        return Ok(false);
    }

    let tasks = jobs
        .iter()
        .map(|job| serde_json::to_vec(job).map(PgTask::new))
        .collect::<Result<Vec<_>, _>>()?;
    if !tasks.is_empty() {
        // storage.push はプールから別接続を取るので、同じトランザクションに入れるには直接流す
        apalis_postgres::sink::push_tasks(&mut *tx, Config::new(QUEUE_NAME), tasks).await?;
    }
    tx.commit().await?;
    Ok(true)
}

/// `issues` はタスクへ反映し、正規化済みの `push` はコミットをタスクへリンクする。
/// それ以外は受信をログに残すのみ。
pub async fn process(job: GithubWebhookJob, state: Data<JobState>) -> Result<(), BoxDynError> {
    if let Some(ForgeEvent::Push {
        repo,
        commits,
        after,
        commits_truncated,
        ..
    }) = &job.forge_event
    {
        // ホストの上限で切り詰められた push は、欠けた分を取り直してから処理する。
        // 取り直せなければ成功にせず再試行させる（成功にすると、この配信は受信済みとして
        // 捨てられ、欠けたコミットは二度とタスクに結ばれない）。
        let commits = if *commits_truncated {
            let github = state
                .settings
                .require_github_app()
                .map_err(|e| anyhow::anyhow!("github app settings are required: {e}"))?;
            Cow::Owned(
                service::github::commits::backfill_push_commits(
                    &state.db,
                    &state.http_client,
                    github,
                    job.project_id,
                    repo,
                    after,
                    commits,
                )
                .await?,
            )
        } else {
            Cow::Borrowed(commits.as_slice())
        };
        service::forge::commits::apply_push(&state.db, job.project_id, repo, &commits).await?;
        tracing::info!(
            integration_id = %job.integration_id,
            project_id = %job.project_id,
            delivery_id = ?job.delivery_id,
            commits = commits.len(),
            truncated = *commits_truncated,
            "forge push event processed"
        );
        return Ok(());
    }

    if job.event == "issues" {
        let applied =
            service::github::sync::apply_issue_event(&state.db, job.project_id, &job.payload)
                .await?;
        tracing::info!(
            integration_id = %job.integration_id,
            project_id = %job.project_id,
            delivery_id = ?job.delivery_id,
            applied,
            "github issues event processed"
        );
        return Ok(());
    }

    tracing::info!(
        integration_id = %job.integration_id,
        project_id = %job.project_id,
        event = %job.event,
        delivery_id = ?job.delivery_id,
        "github webhook received (wave 0 ack)"
    );
    Ok(())
}

pub fn worker_concurrency(settings: &Settings) -> usize {
    settings.github_webhook_worker_concurrency
}

#[cfg(test)]
mod tests {
    use service::forge::events::{ForgeCommit, ForgeRepo};

    use super::*;

    fn sorted_keys(value: &serde_json::Value) -> Vec<&str> {
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        keys
    }

    /// ジョブのペイロードは apalis.jobs に平文で永続化される。
    /// トークンや作者のメールアドレス等を含めてはならない。フィールド追加でこのテストが
    /// 落ちた場合は、機微情報でないことを確認したうえで期待キー集合を更新すること。
    #[test]
    fn payload_contains_no_sensitive_fields() {
        let job = GithubWebhookJob {
            integration_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            event: "push".into(),
            delivery_id: Some("delivery".into()),
            payload: serde_json::Value::Null,
            forge_event: Some(ForgeEvent::Push {
                repo: ForgeRepo {
                    host: "github".into(),
                    host_url: "https://github.com".into(),
                    repo_owner: "acme".into(),
                    repo_name: "backend".into(),
                },
                ref_name: "refs/heads/main".into(),
                forced: false,
                after: "a3f92c1".into(),
                commits_truncated: false,
                commits: vec![ForgeCommit {
                    sha: "a3f92c1".into(),
                    message: "fix: TASK-1".into(),
                    author_handle: "yupix".into(),
                    author_name: "Yupix".into(),
                    committed_at: chrono::Utc::now(),
                    html_url: "https://github.com/acme/backend/commit/a3f92c1".into(),
                }],
            }),
        };
        let value = serde_json::to_value(&job).expect("serialize job");
        assert_eq!(
            sorted_keys(&value),
            vec![
                "delivery_id",
                "event",
                "forge_event",
                "integration_id",
                "payload",
                "project_id"
            ]
        );
        let event = &value["forge_event"];
        assert_eq!(
            sorted_keys(event),
            vec![
                "after",
                "commits",
                "commits_truncated",
                "forced",
                "kind",
                "ref_name",
                "repo"
            ]
        );
        assert_eq!(
            sorted_keys(&event["repo"]),
            vec!["host", "host_url", "repo_name", "repo_owner"]
        );
        assert_eq!(
            sorted_keys(&event["commits"][0]),
            vec![
                "author_handle",
                "author_name",
                "committed_at",
                "html_url",
                "message",
                "sha"
            ]
        );
    }

    /// デプロイ前に積まれていたジョブ（forge_event が無い）も読める
    #[test]
    fn job_queued_before_forge_event_is_supported() {
        let job: GithubWebhookJob = serde_json::from_value(serde_json::json!({
            "integration_id": Uuid::new_v4(),
            "project_id": Uuid::new_v4(),
            "event": "issues",
            "delivery_id": null,
            "payload": { "action": "opened" },
        }))
        .expect("deserialize legacy job");
        assert!(job.forge_event.is_none());
        assert_eq!(job.payload["action"], "opened");
    }
}
