//! テナントへのメール招待（apps/backend/docs/tenant-project-authz.md の「招待」）。
//!
//! 平文のトークンはメールにだけ載せる。DB にはハッシュ（PAT と同じ HMAC）だけを置き、
//! 発行は送信ジョブの処理時に行う（apalis.jobs のペイロードに平文で残さないため）。
//! 発行し直すと前のリンクは使えなくなる。

use chrono::{Duration, Utc};
use sea_orm::prelude::{DateTimeWithTimeZone, Expr, Uuid};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::auth::{create_personal_token_hash, generate_email_verification_token};
use crate::notification_email::escape;
use crate::settings::Settings;
use crate::smtp::SmtpClient;
use common::cache::redis::RedisConnection;
use entity::tenant_invitations;
use entity::tenant_members::TenantRole;

/// 招待の有効期限（日）。再送で発行し直すと、そこからまた数える。
pub const TTL_DAYS: i64 = 7;
/// 同じ宛先への送信の間隔（秒）。再送ボタンの連打で受け手にメールが積もらないようにする。
pub const SEND_COOLDOWN_SECS: u64 = 60;

const KEY_COOLDOWN: &str = "tenant_invite:rl:";

pub fn expires_at_from_now() -> DateTimeWithTimeZone {
    (Utc::now() + Duration::days(TTL_DAYS)).into()
}

pub fn is_expired(invitation: &tenant_invitations::Model) -> bool {
    invitation.expires_at <= Utc::now()
}

fn hash_token(token: &str, secret: &str) -> Result<String, anyhow::Error> {
    create_personal_token_hash(token, secret).map_err(|e| anyhow::anyhow!("hash invitation: {e}"))
}

/// 新しいトークンを発行し、ハッシュだけを保存して平文を返す。
/// 招待が無い（承諾・取り消し済み）か期限切れなら `None`（送らない）。
pub async fn issue_token<C: ConnectionTrait>(
    db: &C,
    invitation_id: Uuid,
    secret: &str,
) -> Result<Option<(tenant_invitations::Model, String)>, anyhow::Error> {
    let token = generate_email_verification_token();
    let updated = tenant_invitations::Entity::update_many()
        .col_expr(
            tenant_invitations::Column::TokenHash,
            Expr::value(hash_token(&token, secret)?),
        )
        .filter(tenant_invitations::Column::Id.eq(invitation_id))
        .filter(tenant_invitations::Column::ExpiresAt.gt(Utc::now()))
        .exec_with_returning(db)
        .await?;
    Ok(updated
        .into_iter()
        .next()
        .map(|invitation| (invitation, token)))
}

/// トークンに対応する招待。期限切れも返す（呼び出し側で 410 と 404 を分けるため）。
pub async fn find_by_token<C: ConnectionTrait>(
    db: &C,
    token: &str,
    secret: &str,
) -> Result<Option<tenant_invitations::Model>, anyhow::Error> {
    Ok(tenant_invitations::Entity::find()
        .filter(tenant_invitations::Column::TokenHash.eq(hash_token(token, secret)?))
        .one(db)
        .await?)
}

pub fn send_slot_key(tenant_id: Uuid, email: &str) -> String {
    format!("{KEY_COOLDOWN}{tenant_id}:{email}")
}

/// 宛先ごとの送信の枠を取る。取れなければ `false`（間隔を空けずに送ろうとした）。
pub async fn try_acquire_send_slot(
    redis: &RedisConnection,
    tenant_id: Uuid,
    email: &str,
) -> Result<bool, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;
    let set_ok: Option<String> = redis::cmd("SET")
        .arg(send_slot_key(tenant_id, email))
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(SEND_COOLDOWN_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET NX invitation cooldown: {e}"))?;
    Ok(set_ok.is_some())
}

pub fn build_accept_url(settings: &Settings, token: &str) -> String {
    format!(
        "{}/invitations/accept?token={}",
        settings.email_verification_app_url.trim_end_matches('/'),
        urlencoding::encode(token)
    )
}

pub struct InvitationMail<'a> {
    pub to: &'a str,
    pub tenant_name: &'a str,
    pub inviter: &'a str,
    pub role: &'a TenantRole,
    pub token: &'a str,
}

pub async fn send_invitation_email(
    smtp: &SmtpClient,
    settings: &Settings,
    mail: InvitationMail<'_>,
) -> Result<(), anyhow::Error> {
    let url = build_accept_url(settings, mail.token);
    let role = format!("{:?}", mail.role);
    let subject = format!("{} への招待", mail.tenant_name);
    let text = format!(
        "{inviter} さんがあなたを {tenant} に {role} として招待しました。\n\
         以下のリンクから参加してください（有効期限は {TTL_DAYS} 日です）。\n\n{url}\n\n\
         アカウントをお持ちでない場合は、このメールアドレス（{to}）で登録してから同じリンクを開いてください。\n\
         心当たりのない場合は、このメールを無視してください。",
        inviter = mail.inviter,
        tenant = mail.tenant_name,
        to = mail.to,
    );
    let html = format!(
        "<p>{inviter} さんがあなたを <strong>{tenant}</strong> に {role} として招待しました。</p>\
         <p>以下のリンクから参加してください（有効期限は {TTL_DAYS} 日です）。</p>\
         <p><a href=\"{url}\">{url}</a></p>\
         <p>アカウントをお持ちでない場合は、このメールアドレス（{to}）で登録してから同じリンクを開いてください。</p>",
        inviter = escape(mail.inviter),
        tenant = escape(mail.tenant_name),
        url = escape(&url),
        to = escape(mail.to),
    );
    smtp.send_email(mail.to, &subject, &text, Some(&html))
        .await
        .map_err(|e| anyhow::anyhow!("send tenant invitation email: {e}"))
}
