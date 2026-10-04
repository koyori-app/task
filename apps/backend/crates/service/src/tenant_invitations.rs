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

const KEY_COOLDOWN: &str = "tenant_invite:rl:";

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
