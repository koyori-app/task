//! テナント招待メールの送信ジョブ（Apalis + PostgreSQL）。
//!
//! ペイロードは招待の id と世代だけ。リンクのトークンは処理時に id と世代から導く
//! （`service::tenant_invitations::invitation_token`）ので、apalis.jobs に平文で残らず、
//! 同じ世代の再試行は同じリンクを送る（配信済みのリンクを壊さない）。
//! 招待の発行・再送とジョブの投入は 1 トランザクションで確定する（[`issue_and_enqueue`]）。

use std::sync::Arc;
use std::time::Duration;

use apalis::prelude::{BackoffConfig, BoxDynError, Data, IntervalStrategy, StrategyBuilder};
use apalis_postgres::{Config, JsonCodec, PgPool, PgTask, PostgresStorage};
use sea_orm::{ActiveEnum, EntityTrait};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use common::settings::Settings;
use entity::tenant_members::TenantRole;
use entity::{tenants, users};
use service::tenant_invitations::{self, InvitationMail};

use crate::JobState;

pub const QUEUE_NAME: &str = "tenant_invitation_email";
pub const MAX_RETRIES: usize = 8;

#[derive(Clone, Serialize, Deserialize)]
pub struct TenantInvitationEmailJob {
    pub invitation_id: Uuid,
    /// 積んだ時点の招待の世代。処理時に招待の世代と違えば（後から再送・再招待された）送らない
    pub generation: i32,
}

impl TenantInvitationEmailJob {
    pub fn new(invitation_id: Uuid, generation: i32) -> Self {
        Self {
            invitation_id,
            generation,
        }
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

/// 招待の行を書いたトランザクションのまま送信ジョブを積み、まとめて確定する。
///
/// 別々に確定すると、ジョブの投入に失敗したときに世代だけが進み、前のリンクが通らないのに
/// 新しいメールも届かない状態が残る。`storage.push` はプールから別接続を取るので、
/// 同じトランザクションに入れるには直接流す（`github_webhook::enqueue_delivery` と同じ）。
async fn enqueue_in(
    tx: &mut sqlx::PgConnection,
    invitation_id: Uuid,
    generation: i32,
) -> Result<(), anyhow::Error> {
    let task = PgTask::new(serde_json::to_vec(&TenantInvitationEmailJob::new(
        invitation_id,
        generation,
    ))?);
    apalis_postgres::sink::push_tasks(tx, Config::new(QUEUE_NAME), vec![task]).await?;
    Ok(())
}

/// 招待を発行する。同じ宛先の保留中の招待があれば、ロール・招待者・期限を更新して世代を上げる
/// （前のリンクは通らなくなる）。送信ジョブも同じトランザクションで積む。招待の id を返す。
pub async fn issue_and_enqueue(
    pool: &PgPool,
    tenant_id: Uuid,
    email: &str,
    role: &TenantRole,
    invited_by: Uuid,
) -> Result<Uuid, anyhow::Error> {
    let mut tx = pool.begin().await?;
    let (id, generation): (String, i32) = sqlx::query_as(
        "INSERT INTO tenant_invitations
             (id, tenant_id, email, role, generation, invited_by, expires_at, created_at)
         VALUES ($1::uuid, $2::uuid, $3, $4, 0, $5::uuid, now() + make_interval(days => $6), now())
         ON CONFLICT (tenant_id, email) DO UPDATE SET
             role = EXCLUDED.role,
             invited_by = EXCLUDED.invited_by,
             expires_at = EXCLUDED.expires_at,
             created_at = EXCLUDED.created_at,
             generation = tenant_invitations.generation + 1
         RETURNING id::text, generation",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id.to_string())
    .bind(email)
    .bind(role.to_value())
    .bind(invited_by.to_string())
    .bind(tenant_invitations_ttl_days())
    .fetch_one(&mut *tx)
    .await?;
    let id = Uuid::parse_str(&id)?;
    enqueue_in(&mut tx, id, generation).await?;
    tx.commit().await?;
    Ok(id)
}

/// 招待を送り直す。期限を今から延ばして世代を上げ（前のリンクは通らなくなる）、送信ジョブを
/// 同じトランザクションで積む。招待者は送り直した人に替える（元の招待者が外されていても、
/// 送り直した招待が使えるように）。招待がそのテナントに無ければ `false`。
pub async fn resend_and_enqueue(
    pool: &PgPool,
    tenant_id: Uuid,
    invitation_id: Uuid,
    resent_by: Uuid,
) -> Result<bool, anyhow::Error> {
    let mut tx = pool.begin().await?;
    let generation: Option<(i32,)> = sqlx::query_as(
        "UPDATE tenant_invitations
         SET generation = generation + 1,
             expires_at = now() + make_interval(days => $3),
             invited_by = $4::uuid
         WHERE id = $1::uuid AND tenant_id = $2::uuid
         RETURNING generation",
    )
    .bind(invitation_id.to_string())
    .bind(tenant_id.to_string())
    .bind(tenant_invitations_ttl_days())
    .bind(resent_by.to_string())
    .fetch_optional(&mut *tx)
    .await?;
    let Some((generation,)) = generation else {
        return Ok(false);
    };
    enqueue_in(&mut tx, invitation_id, generation).await?;
    tx.commit().await?;
    Ok(true)
}

fn tenant_invitations_ttl_days() -> i32 {
    tenant_invitations::TTL_DAYS as i32
}

pub async fn process(
    job: TenantInvitationEmailJob,
    state: Data<JobState>,
) -> Result<(), BoxDynError> {
    let invitation = entity::tenant_invitations::Entity::find_by_id(job.invitation_id)
        .one(&state.db)
        .await?;
    let Some(invitation) = invitation.filter(|invitation| {
        invitation.generation == job.generation && !tenant_invitations::is_expired(invitation)
    }) else {
        // 承諾・取り消し・期限切れの後か、後から再送・再招待された（新しい世代のジョブが送る）
        info!(
            invitation_id = %job.invitation_id,
            generation = job.generation,
            "skip tenant invitation email: not the current pending invitation"
        );
        return Ok(());
    };
    // 招待者が外されていれば、承諾もプレビューも 404 になるリンクなので外へ出さない
    if !tenant_invitations::inviter_still_admin(&state.db, &invitation).await? {
        info!(
            invitation_id = %job.invitation_id,
            "skip tenant invitation email: inviter is no longer an admin"
        );
        return Ok(());
    }
    // 同じ世代なら何度作っても同じ値。再試行で配信済みのリンクを壊さない
    let token = tenant_invitations::invitation_token(
        invitation.id,
        invitation.generation,
        &state.settings.personal_token_secret,
    )?;
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
        let job = TenantInvitationEmailJob::new(Uuid::new_v4(), 3);
        let value = serde_json::to_value(&job).expect("serialize job");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("payload is a JSON object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["generation", "invitation_id"]);
    }
}
