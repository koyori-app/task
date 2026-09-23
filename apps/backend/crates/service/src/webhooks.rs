//! 外部向け Webhook（docs/features/tasks/10.webhooks.md）。
//!
//! イベントの発火点は [`emit`] で `webhook_deliveries` に 1 行ずつ積むだけ（outbox）。
//! 送信は `job::webhook_delivery` の掃き出しループが行う。

use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use sea_orm::entity::prelude::Json;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    prelude::Uuid,
};
use sha2::Sha256;

use crate::error::AppError;
use crate::settings::Settings;
use common::notifications::{TYPE_REVIEW_FINDING_CHANGED, TYPE_REVIEW_ROUND_CREATED};
use entity::{projects, users, webhook_deliveries, webhooks};

pub const EVENT_TASK_CREATED: &str = "task.created";
pub const EVENT_COMMENT_CREATED: &str = "comment.created";
pub const EVENT_REVIEW_ROUND_CREATED: &str = "review.round_created";
pub const EVENT_REVIEW_FINDING_CHANGED: &str = "review.finding_changed";

/// 購読できるイベント。仕様 §3 の残り（task.updated など）は未実装。
pub const EVENTS: &[&str] = &[
    EVENT_TASK_CREATED,
    EVENT_COMMENT_CREATED,
    EVENT_REVIEW_ROUND_CREATED,
    EVENT_REVIEW_FINDING_CHANGED,
];

pub const FORMAT_JSON: &str = "json";
pub const FORMAT_DISCORD: &str = "discord";
pub const FORMATS: &[&str] = &[FORMAT_JSON, FORMAT_DISCORD];

/// secret の最短長。短い secret は署名を推測されやすい
pub const MIN_SECRET_LEN: usize = 16;
/// 1 配信の試行回数の上限（仕様 §5）
pub const MAX_ATTEMPTS: i16 = 5;
/// 打ち止めになった配信がこれだけ続いたら Webhook を止める（仕様 §5）
pub const MAX_FAILURE_STREAK: i16 = 5;

/// 送信先 URL の検証（SSRF 対策）。https 必須（開発設定時のみ localhost の http を許可）、
/// private / link-local / メタデータ宛てと、それらへ解決される名前を拒否する。
/// 作成・更新時と送信直前の両方で呼ぶ（DNS の向き先は後から変えられる）。
pub fn validate_url(settings: &Settings, url: &str) -> Result<(), AppError> {
    if !settings.webhook_allow_loopback {
        let parsed = url::Url::parse(url)
            .map_err(|e| AppError::BadRequestDetail(format!("url が不正です: {e}")))?;
        let loopback = match parsed.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => std::net::IpAddr::V6(ip).to_canonical().is_loopback(),
            Some(url::Host::Domain("localhost")) => true,
            Some(url::Host::Domain(_)) => {
                // localhost 以外の名前でも loopback に解決されるものは拒否する。
                let resolve = || parsed.socket_addrs(|| None);
                let addresses = if tokio::runtime::Handle::try_current().is_ok() {
                    tokio::task::block_in_place(resolve)
                } else {
                    resolve()
                }
                .map_err(|e| {
                    AppError::BadRequestDetail(format!("url の名前解決に失敗しました: {e}"))
                })?;
                addresses
                    .iter()
                    .any(|addr| addr.ip().to_canonical().is_loopback())
            }
            None => false,
        };
        if loopback {
            return Err(AppError::BadRequestDetail(
                "loopback は送信先に使えません".into(),
            ));
        }
    }
    auth_core::url_guard::validate_instance_url(url)
        .map_err(|e| AppError::BadRequestDetail(format!("url を送信先に使えません: {e}")))
}

pub fn validate_events(events: &[String]) -> Result<(), AppError> {
    if events.is_empty() {
        return Err(AppError::BadRequestDetail(
            "events を 1 つ以上指定してください".into(),
        ));
    }
    if let Some(unknown) = events.iter().find(|e| !EVENTS.contains(&e.as_str())) {
        return Err(AppError::BadRequestDetail(format!(
            "events に未知の値があります（受け取った値: {unknown}。使える値: {}）",
            EVENTS.join(", ")
        )));
    }
    Ok(())
}

pub fn validate_format(format: &str) -> Result<(), AppError> {
    if FORMATS.contains(&format) {
        Ok(())
    } else {
        Err(AppError::BadRequestDetail(format!(
            "format は json か discord です（受け取った値: {format}）"
        )))
    }
}

pub fn validate_secret(secret: &str) -> Result<(), AppError> {
    if secret.chars().count() >= MIN_SECRET_LEN {
        Ok(())
    } else {
        Err(AppError::BadRequestDetail(format!(
            "secret は {MIN_SECRET_LEN} 文字以上にしてください"
        )))
    }
}

/// secret の暗号化鍵。`totp_encryption_key` は起動に必須の設定なので、
/// GitHub App を設定していない環境でも Webhook を使える。
fn secret_key(settings: &Settings) -> &str {
    &settings.totp_encryption_key
}

pub fn encrypt_secret(settings: &Settings, secret: &str) -> Result<String, AppError> {
    auth_core::crypto::encrypt_token(secret_key(settings), secret).map_err(AppError::Internal)
}

pub fn decrypt_secret(settings: &Settings, secret_enc: &str) -> Result<String, AppError> {
    auth_core::crypto::decrypt_token(secret_key(settings), secret_enc).map_err(AppError::Internal)
}

/// `X-Task-Signature` の値（`sha256=<hex(HMAC-SHA256(secret, body))>`）。
pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// 試行 `attempt` 回を終えたあと、次の試行までの待ち（仕様 §5: 即時 / 30s / 5m / 30m / 2h）。
pub fn backoff(attempt: i16) -> Duration {
    match attempt {
        i16::MIN..=0 => Duration::ZERO,
        1 => Duration::from_secs(30),
        2 => Duration::from_secs(5 * 60),
        3 => Duration::from_secs(30 * 60),
        _ => Duration::from_secs(2 * 60 * 60),
    }
}

/// イベントを購読している Webhook ごとに配信を 1 行積む。
///
/// **呼び出し側のトランザクション内で呼ぶ**（outbox）。イベントが巻き戻ったのに
/// 配信だけ残る・その逆を起こさない。`fields` はイベント固有の中身で、共通の
/// `event` / `timestamp` / `project_id` / `project` / `actor` をここで足す。
pub async fn emit<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    actor_id: Uuid,
    event: &str,
    fields: Json,
) -> Result<(), AppError> {
    let hooks: Vec<webhooks::Model> = webhooks::Entity::find()
        .filter(webhooks::Column::ProjectId.eq(project_id))
        .filter(webhooks::Column::IsActive.eq(true))
        .all(db)
        .await?
        .into_iter()
        .filter(|hook| hook.events.iter().any(|e| e == event))
        .collect();
    if hooks.is_empty() {
        return Ok(());
    }

    let project = projects::Entity::find_by_id(project_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("webhook project {project_id} not found"))?;
    let actor = users::Entity::find_by_id(actor_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("webhook actor {actor_id} not found"))?;

    let now = chrono::Utc::now();
    let mut payload = serde_json::json!({
        "event": event,
        "timestamp": now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "project_id": project_id,
        "project": { "id": project.id, "key": project.key, "name": project.name },
        "actor": { "id": actor.id, "username": actor.username },
    });
    if let (Some(out), Json::Object(extra)) = (payload.as_object_mut(), fields) {
        out.extend(extra);
    }

    for hook in hooks {
        webhook_deliveries::ActiveModel {
            id: Set(Uuid::new_v4()),
            webhook_id: Set(hook.id),
            event: Set(event.to_string()),
            payload: Set(payload.clone()),
            status_code: Set(None),
            attempt: Set(0),
            next_attempt_at: Set(Some(now.into())),
            last_error: Set(None),
            delivered_at: Set(None),
            created_at: Set(now.into()),
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

fn clip(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

fn str_at<'a>(payload: &'a Json, pointer: &str) -> &'a str {
    payload
        .pointer(pointer)
        .and_then(Json::as_str)
        .unwrap_or("")
}

/// Discord の Incoming Webhook に送る本文（`content` + `embeds`）。
///
/// レビュー系の要約は通知メールの件名・本文 1 行目と同じ文言にする。
pub fn discord_message(event: &str, payload: &Json) -> Json {
    let (summary, detail) = match event {
        EVENT_REVIEW_ROUND_CREATED | EVENT_REVIEW_FINDING_CHANGED => {
            // in-app の payload は actor を名前の文字列で持つ（Webhook では id つきの object）
            let mut flat = payload.clone();
            if let Some(name) = payload.pointer("/actor/username").cloned() {
                flat["actor"] = name;
            }
            let notification_type = if event == EVENT_REVIEW_ROUND_CREATED {
                TYPE_REVIEW_ROUND_CREATED
            } else {
                TYPE_REVIEW_FINDING_CHANGED
            };
            crate::notification_email::subject_and_line(notification_type, &flat, None)
        }
        EVENT_TASK_CREATED => {
            let seq = payload.pointer("/task/seq_id").cloned().unwrap_or_default();
            let title = str_at(payload, "/task/title");
            (
                format!("[Koyori] タスク #{seq} {title} が作成されました"),
                format!(
                    "ステータス: {} / 優先度: {}",
                    str_at(payload, "/task/status"),
                    str_at(payload, "/task/priority")
                ),
            )
        }
        EVENT_COMMENT_CREATED => {
            let seq = payload.pointer("/task/seq_id").cloned().unwrap_or_default();
            (
                format!(
                    "[Koyori] #{seq} {} に {} がコメントしました",
                    str_at(payload, "/task/title"),
                    str_at(payload, "/comment/author")
                ),
                str_at(payload, "/comment/body").to_string(),
            )
        }
        other => (format!("[Koyori] {other}"), String::new()),
    };

    // Discord は空の value を持つ field を受け付けない
    let mut fields = Vec::new();
    for (name, pointer) in [
        ("プロジェクト", "/project/name"),
        ("実行者", "/actor/username"),
    ] {
        let value = str_at(payload, pointer);
        if !value.is_empty() {
            fields.push(
                serde_json::json!({ "name": name, "value": clip(value, 1024), "inline": true }),
            );
        }
    }
    // content は 2000 文字、embed の description は 4096 文字が Discord の上限
    serde_json::json!({
        "content": clip(&summary, 2000),
        "embeds": [{
            "title": event,
            "description": clip(&detail, 4096),
            "fields": fields,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_follows_spec() {
        let secs: Vec<u64> = (0..=5).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(secs, vec![0, 30, 300, 1800, 7200, 7200]);
    }

    /// RFC 4231 ではなく広く引かれる既知ベクタ（Wikipedia の HMAC 例）。
    #[test]
    fn sign_matches_known_vector() {
        assert_eq!(
            sign("key", b"The quick brown fox jumps over the lazy dog"),
            "sha256=f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn discord_message_for_review_reuses_mail_subject() {
        let payload = serde_json::json!({
            "event": "review.round_created",
            "project": {"id": Uuid::nil(), "key": "KOY", "name": "Koyori"},
            "actor": {"id": Uuid::nil(), "username": "alice"},
            "pr_number": 618,
            "round": 2,
            "reviewer": "alice",
            "counts": {"high": 1, "medium": 0, "low": 0, "nit": 0},
        });
        let message = discord_message(EVENT_REVIEW_ROUND_CREATED, &payload);
        assert_eq!(
            message["content"],
            "[Koyori] PR #618 のレビュー R2（high 1 / medium 0 / low 0 / nit 0）"
        );
        let embed = &message["embeds"][0];
        assert_eq!(embed["title"], "review.round_created");
        assert!(
            embed["description"]
                .as_str()
                .unwrap()
                .starts_with("alice が")
        );
        assert_eq!(embed["fields"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn discord_message_clips_long_comment_and_drops_empty_fields() {
        let payload = serde_json::json!({
            "task": {"seq_id": 3, "title": "t"},
            "comment": {"author": "bob", "body": "あ".repeat(5000)},
        });
        let message = discord_message(EVENT_COMMENT_CREATED, &payload);
        assert_eq!(
            message["content"],
            "[Koyori] #3 t に bob がコメントしました"
        );
        let description = message["embeds"][0]["description"].as_str().unwrap();
        assert_eq!(description.chars().count(), 4096);
        assert!(
            message["embeds"][0]["fields"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn events_must_be_known_and_non_empty() {
        assert!(validate_events(&[]).is_err());
        assert!(validate_events(&["task.updated".into()]).is_err());
        assert!(validate_events(&["task.created".into(), "review.round_created".into()]).is_ok());
    }
}
