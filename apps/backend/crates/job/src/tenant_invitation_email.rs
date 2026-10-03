//! テナント招待メールの送信ジョブ（Apalis + PostgreSQL）。
//!
//! ペイロードは招待の id だけ。トークンは処理時に発行してハッシュだけを DB に置く
//! （`service::tenant_invitations`）。apalis.jobs に平文で残さないため。
//! 再試行のたびに発行し直すので、届いたメールのうち最後の 1 通のリンクだけが有効になる。

use std::sync::Arc;
use std::time::Duration;

use apalis::prelude::{
    BackoffConfig, BoxDynError, Data, IntervalStrategy, StrategyBuilder, TaskSink,
};
use apalis_postgres::{Config, JsonCodec, PgPool, PostgresStorage};
use sea_orm::EntityTrait;
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use common::settings::Settings;
use entity::{tenants, users};
use service::tenant_invitations::{self, InvitationMail};

use crate::JobState;

pub const QUEUE_NAME: &str = "tenant_invitation_email";
pub const MAX_RETRIES: usize = 8;

#[derive(Clone, Serialize, Deserialize)]
pub struct TenantInvitationEmailJob {
    pub invitation_id: Uuid,
}

impl TenantInvitationEmailJob {
    pub fn new(invitation_id: Uuid) -> Self {
        Self { invitation_id }
    }
}

pub type TenantInvitationEmailStorage = PostgresStorage<
    TenantInvitationEmailJob,
    apalis_postgres::CompactType,
    JsonCodec<apalis_postgres::CompactType>,
    apalis_postgres::PgNotify,
>;

pub fn build_storage(pool: &PgPool, _settings: &Settings) -> TenantInvitationEmailStorage {
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
) -> Result<Arc<TenantInvitationEmailStorage>, anyhow::Error> {
    PostgresStorage::setup(pool).await?;
    Ok(Arc::new(build_storage(pool, settings)))
}

pub async fn enqueue(
    storage: &TenantInvitationEmailStorage,
    job: TenantInvitationEmailJob,
) -> Result<(), anyhow::Error> {
    let mut storage = storage.clone();
    storage
        .push(job)
        .await
        .map_err(|e| anyhow::anyhow!("push tenant invitation email job: {e}"))
}

pub async fn process(
    job: TenantInvitationEmailJob,
    state: Data<JobState>,
) -> Result<(), BoxDynError> {
    let Some((invitation, token)) = tenant_invitations::issue_token(
        &state.db,
        job.invitation_id,
        &state.settings.personal_token_secret,
    )
    .await?
    else {
        // 承諾・取り消し・期限切れの後に回ってきた。送るものは無い
        info!(invitation_id = %job.invitation_id, "skip tenant invitation email: no pending invitation");
        return Ok(());
    };
    // FK（CASCADE）があるので、招待が残っている限りテナントと招待者も居る
    let tenant = tenants::Entity::find_by_id(invitation.tenant_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("invitation {} has no tenant row", invitation.id))?;
    let inviter = users::Entity::find_by_id(invitation.invited_by)
        .one(&state.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("invitation {} has no inviter row", invitation.id))?;
    tenant_invitations::send_invitation_email(
        &state.smtp_client,
        &state.settings,
        InvitationMail {
            to: &invitation.email,
            tenant_name: &tenant.name,
            inviter: &inviter.username,
            role: &invitation.role,
            token: &token,
        },
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ジョブペイロードは Postgres の apalis.jobs に平文で永続化される。
    /// トークンを載せないことの回帰ガードとして、シリアライズ後のキー集合を固定する。
    #[test]
    fn payload_contains_no_token() {
        let job = TenantInvitationEmailJob::new(Uuid::new_v4());
        let value = serde_json::to_value(&job).expect("serialize job");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("payload is a JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["invitation_id"]);
    }
}
