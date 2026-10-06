//! テナントへのメール招待（apps/backend/docs/tenant-project-authz.md の「招待」）。
//!
//! トークンは保存しない。招待 id と世代（`generation`）からサーバーの鍵で導く
//! （`{招待 id}.{HMAC}`）。同じ世代なら何度作っても同じ値なので、送信ジョブの再試行が
//! 配信済みのリンクを壊さない。再送・再招待で世代を上げたときだけ前のリンクが通らなくなる。
//! 平文のトークンは DB にも apalis.jobs にも残らない。

use std::sync::LazyLock;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use sea_orm::prelude::{DateTimeWithTimeZone, Uuid};
use sea_orm::{ConnectionTrait, EntityTrait};
use sha2::Sha256;

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
/// テナントごと・招待者ごとの送信の上限（1 時間 / 1 日）。テナントは誰でも作れるので、
/// 宛先を変えながら送り続けてサービスの送信ドメインの評判を落とせないようにする。
pub const HOURLY_SEND_LIMIT: u32 = 50;
pub const DAILY_SEND_LIMIT: u32 = 200;

const KEY_COOLDOWN: &str = "tenant_invite:rl:";
const KEY_QUOTA: &str = "tenant_invite:quota:";

/// 窓ごとの数を 4 つ（テナント / 招待者 × 時 / 日）まとめて見て、どれも上限に届いていなければ
/// すべて 1 つ進める。1 つでも届いていれば何も進めずに 0 を返す。
/// KEYS: テナント時・テナント日・招待者時・招待者日。ARGV: 時の上限・日の上限・時の TTL・日の TTL
static CONSUME_QUOTA_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
        local limits = { tonumber(ARGV[1]), tonumber(ARGV[2]), tonumber(ARGV[1]), tonumber(ARGV[2]) }
        local ttls = { ARGV[3], ARGV[4], ARGV[3], ARGV[4] }
        for i = 1, 4 do
            if tonumber(redis.call('GET', KEYS[i]) or '0') >= limits[i] then
                return 0
            end
        end
        for i = 1, 4 do
            if redis.call('INCR', KEYS[i]) == 1 then
                redis.call('EXPIRE', KEYS[i], ttls[i])
            end
        end
        return 1
        "#,
    )
});

/// 値が自分の取った枠のときだけ消す。枠が期限切れになった後に別のリクエストが取り直した枠を、
/// 遅れて失敗した前のリクエストが消さないようにする。
static RELEASE_SLOT_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
        if redis.call('GET', KEYS[1]) == ARGV[1] then
            return redis.call('DEL', KEYS[1])
        end
        return 0
        "#,
    )
});

pub fn expires_at_from_now() -> DateTimeWithTimeZone {
    (Utc::now() + Duration::days(TTL_DAYS)).into()
}

pub fn is_expired(invitation: &tenant_invitations::Model) -> bool {
    invitation.expires_at <= Utc::now()
}

fn token_mac(id: Uuid, generation: i32, secret: &str) -> Result<Hmac<Sha256>, anyhow::Error> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("invitation hmac init: {e}"))?;
    // 他の用途の HMAC（PAT のハッシュなど）と同じ鍵を使うので、用途の前置きで分ける
    mac.update(format!("tenant-invitation:{id}:{generation}").as_bytes());
    Ok(mac)
}

/// 招待リンクのトークン（`{招待 id}.{HMAC}`）。
pub fn invitation_token(id: Uuid, generation: i32, secret: &str) -> Result<String, anyhow::Error> {
    let tag = token_mac(id, generation, secret)?.finalize().into_bytes();
    Ok(format!("{id}.{}", URL_SAFE_NO_PAD.encode(tag)))
}

/// トークンがこの招待の今の世代のものか（定数時間で比べる）。形が崩れていれば false。
pub fn token_matches(
    invitation: &tenant_invitations::Model,
    token: &str,
    secret: &str,
) -> Result<bool, anyhow::Error> {
    let Some((id, tag)) = token.split_once('.') else {
        return Ok(false);
    };
    if id != invitation.id.to_string() {
        return Ok(false);
    }
    let Ok(tag) = URL_SAFE_NO_PAD.decode(tag) else {
        return Ok(false);
    };
    Ok(token_mac(invitation.id, invitation.generation, secret)?
        .verify_slice(&tag)
        .is_ok())
}

/// トークンに対応する招待。期限切れも返す（呼び出し側で 410 と 404 を分けるため）。
/// 世代が古い（再送・再招待の前のリンク）・取り消し・承諾済みなら `None`。
pub async fn find_by_token<C: ConnectionTrait>(
    db: &C,
    token: &str,
    secret: &str,
) -> Result<Option<tenant_invitations::Model>, anyhow::Error> {
    let Some(id) = token
        .split_once('.')
        .and_then(|(id, _)| Uuid::parse_str(id).ok())
    else {
        return Ok(None);
    };
    let Some(invitation) = tenant_invitations::Entity::find_by_id(id).one(db).await? else {
        return Ok(None);
    };
    Ok(token_matches(&invitation, token, secret)?.then_some(invitation))
}

pub fn send_slot_key(tenant_id: Uuid, email: &str) -> String {
    format!("{KEY_COOLDOWN}{tenant_id}:{email}")
}

/// 宛先ごとの送信の枠を取り、返すときに使う持ち主の印を返す。
/// 取れなければ `None`（間隔を空けずに送ろうとした）。
pub async fn try_acquire_send_slot(
    redis: &RedisConnection,
    tenant_id: Uuid,
    email: &str,
) -> Result<Option<String>, anyhow::Error> {
    let owner = Uuid::new_v4().to_string();
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;
    let set_ok: Option<String> = redis::cmd("SET")
        .arg(send_slot_key(tenant_id, email))
        .arg(&owner)
        .arg("NX")
        .arg("EX")
        .arg(SEND_COOLDOWN_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET NX invitation cooldown: {e}"))?;
    Ok(set_ok.map(|_| owner))
}

/// 取った送信の枠を返す。招待の更新やジョブの投入に失敗して、メールを送らずに終わったときに使う
/// （返さないと、直後のやり直しが 429 になる）。`owner` は [`try_acquire_send_slot`] が返した印で、
/// 枠がもう別のリクエストのものになっていれば何もしない。
pub async fn release_send_slot(
    redis: &RedisConnection,
    tenant_id: Uuid,
    email: &str,
    owner: &str,
) -> Result<(), anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;
    RELEASE_SLOT_SCRIPT
        .key(send_slot_key(tenant_id, email))
        .arg(owner)
        .invoke_async::<i64>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis release invitation cooldown: {e}"))?;
    Ok(())
}

/// テナントごと・招待者ごとの送信の上限を 1 通ぶん使う。上限に届いていれば `false`。
/// 窓は時刻で区切る固定窓（毎時・毎日 UTC で切り替わる）。
pub async fn try_consume_send_quota(
    redis: &RedisConnection,
    tenant_id: Uuid,
    inviter_id: Uuid,
) -> Result<bool, anyhow::Error> {
    const HOUR: i64 = 60 * 60;
    const DAY: i64 = 24 * HOUR;
    let now = Utc::now().timestamp();
    let (hour, day) = (now / HOUR, now / DAY);
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;
    let consumed: i64 = CONSUME_QUOTA_SCRIPT
        .key(format!("{KEY_QUOTA}t:{tenant_id}:h:{hour}"))
        .key(format!("{KEY_QUOTA}t:{tenant_id}:d:{day}"))
        .key(format!("{KEY_QUOTA}u:{inviter_id}:h:{hour}"))
        .key(format!("{KEY_QUOTA}u:{inviter_id}:d:{day}"))
        .arg(HOURLY_SEND_LIMIT)
        .arg(DAILY_SEND_LIMIT)
        .arg(HOUR)
        .arg(DAY)
        .invoke_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis consume invitation quota: {e}"))?;
    Ok(consumed == 1)
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
    // 件名は固定にする。テナント名は誰でも好きに付けられるので、件名に出すと
    // 偽の請求などの文面をサービスの送信元から送れてしまう（本文ではエスケープして出す）
    let subject = "テナントへの招待が届きました";
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
    smtp.send_email(mail.to, subject, &text, Some(&html))
        .await
        .map_err(|e| anyhow::anyhow!("send tenant invitation email: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invitation(generation: i32) -> tenant_invitations::Model {
        tenant_invitations::Model {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "a@example.com".into(),
            role: TenantRole::Member,
            generation,
            invited_by: Uuid::new_v4(),
            expires_at: expires_at_from_now(),
            created_at: Utc::now().into(),
        }
    }

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn same_generation_yields_the_same_token() {
        let inv = invitation(3);
        let a = invitation_token(inv.id, inv.generation, SECRET).unwrap();
        let b = invitation_token(inv.id, inv.generation, SECRET).unwrap();
        assert_eq!(a, b, "再試行で同じリンクを送る");
        assert!(token_matches(&inv, &a, SECRET).unwrap());
    }

    #[test]
    fn other_generation_id_or_secret_does_not_match() {
        let inv = invitation(3);
        let old = invitation_token(inv.id, 2, SECRET).unwrap();
        assert!(
            !token_matches(&inv, &old, SECRET).unwrap(),
            "前の世代のリンクは通らない"
        );
        let other = invitation_token(Uuid::new_v4(), 3, SECRET).unwrap();
        assert!(!token_matches(&inv, &other, SECRET).unwrap());
        let forged = invitation_token(inv.id, 3, "another-secret-another-secret-xx").unwrap();
        assert!(!token_matches(&inv, &forged, SECRET).unwrap());
        for broken in [
            "",
            "no-dot",
            &format!("{}.", inv.id),
            &format!("{}.!!!", inv.id),
        ] {
            assert!(!token_matches(&inv, broken, SECRET).unwrap(), "{broken}");
        }
    }
}
