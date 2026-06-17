//! メール認証トークンを Redis に保持する（TTL で有効期限）。
//!
//! `user->token` と `token->user` は Lua で原子的に更新し、再送時の旧トークン無効化と
//! 消費時の逆マッピング削除が同時リクエストでも崩れないようにする。
//!
//! ジョブごとの `issued_at`（世代）を Redis に保持し、Apalis リトライで古いジョブが
//! 新しいトークンを上書きしないようにする。

use std::sync::LazyLock;

use uuid::Uuid;

use super::email::normalize_email;
use super::redis::RedisConnection;

/// 世代チェック後、旧 token キー削除 → 新 token/user/gen キー SET を一括実行。
/// 返却: 1 = 反映した, 0 = より新しい世代、または同世代で別トークンのためスキップ。
static STORE_TOKEN_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
        local user_key = KEYS[1]
        local gen_key = KEYS[2]
        local token_key = KEYS[3]
        local new_token = ARGV[3]
        local issued_at = tonumber(ARGV[5])
        local ttl = tonumber(ARGV[4])

        local current_gen = redis.call('GET', gen_key)
        if current_gen then
            local current_gen_num = tonumber(current_gen)
            if current_gen_num > issued_at then
                return 0
            end
            if current_gen_num == issued_at then
                local current_token = redis.call('GET', user_key)
                if current_token ~= new_token then
                    return 0
                end
            end
        end

        local old_token = redis.call('GET', user_key)
        if old_token then
            redis.call('DEL', ARGV[1] .. old_token)
        end
        redis.call('SET', token_key, ARGV[2], 'EX', ttl)
        redis.call('SET', user_key, ARGV[3], 'EX', ttl)
        redis.call('SET', gen_key, tostring(issued_at), 'EX', ttl)
        return 1
        "#,
    )
});

/// 消費時: GETDEL 後、user->token が当該トークンのときだけ user キーを削除。
static CONSUME_TOKEN_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
        local user_id = redis.call('GETDEL', KEYS[1])
        if not user_id then
            return nil
        end
        local user_key = ARGV[1] .. user_id
        if redis.call('GET', user_key) == ARGV[2] then
            redis.call('DEL', user_key)
        end
        return user_id
        "#,
    )
});

/// 認証リンクの有効期限（秒）。約 15 分。
pub const TOKEN_TTL_SECS: u64 = 15 * 60;
/// 認証メール再送のクールダウン（秒）。
pub const RESEND_COOLDOWN_SECS: u64 = 60;

const KEY_TOKEN: &str = "email_verify:t:";
const KEY_USER: &str = "email_verify:u:";
const KEY_GEN: &str = "email_verify:gen:";
const KEY_RESEND: &str = "email_verify:resend:e:";

/// 認証トークンを Redis に保存する（Lua で原子的に user/token/世代を更新）。
///
/// Redis の現世代より `issued_at` が小さい場合は更新せず、Apalis リトライによる
/// 古いジョブの上書きを防ぐ。同じ `issued_at` かつ同じ `token` のときだけ冪等に再反映する
/// （同一ミリ秒で別トークンが発行された場合はスキップ）。
///
/// # Arguments
/// * `redis` - トークン・世代キーを保持する Redis 接続
/// * `user_id` - 認証対象ユーザー ID
/// * `token` - 保存する認証トークン文字列
/// * `issued_at` - ジョブの発行世代（Unix ミリ秒。大きいほど新しい）
///
/// # Returns
/// * `Ok(true)` - トークンと世代を反映した
/// * `Ok(false)` - より新しい世代、または同世代の別トークンが既にあるためスキップした
///
/// # Errors
/// * Redis 接続・Lua スクリプト実行に失敗した場合
pub async fn store_token(
    redis: &RedisConnection,
    user_id: Uuid,
    token: &str,
    issued_at: u64,
) -> Result<bool, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;

    let user_key = format!("{KEY_USER}{user_id}");
    let gen_key = format!("{KEY_GEN}{user_id}");
    let token_key = format!("{KEY_TOKEN}{token}");
    let ttl = TOKEN_TTL_SECS.to_string();

    let applied: i32 = STORE_TOKEN_SCRIPT
        .key(&user_key)
        .key(&gen_key)
        .key(&token_key)
        .arg(KEY_TOKEN)
        .arg(user_id.to_string())
        .arg(token)
        .arg(&ttl)
        .arg(issued_at.to_string())
        .invoke_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis store_token script: {e}"))?;

    Ok(applied == 1)
}

/// GETDEL でトークンを消費し、対応するユーザー ID を返す。無効・期限切れなら `None`。
pub async fn consume_token(
    redis: &RedisConnection,
    token: &str,
) -> Result<Option<Uuid>, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;

    let token_key = format!("{KEY_TOKEN}{token}");
    let raw: Option<String> = CONSUME_TOKEN_SCRIPT
        .key(&token_key)
        .arg(KEY_USER)
        .arg(token)
        .invoke_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis consume_token script: {e}"))?;

    let Some(s) = raw else {
        return Ok(None);
    };

    let uid =
        Uuid::parse_str(s.trim()).map_err(|e| anyhow::anyhow!("invalid user id in redis: {e}"))?;

    Ok(Some(uid))
}

/// メールアドレス単位で再送クールダウン枠を取得する（`SET NX`）。
///
/// # Arguments
/// * `redis` - クールダウンキーを保持する Redis 接続
/// * `email` - 対象メールアドレス（内部で [`super::email::normalize_email`] する）
///
/// # Returns
/// * `Ok(true)` - 枠を取得できた（再送 API が続行してよい）
/// * `Ok(false)` - [`RESEND_COOLDOWN_SECS`] 以内に再送済み（429 相当）
///
/// # Errors
/// * Redis 接続・コマンド実行に失敗した場合
pub async fn try_acquire_resend_slot(
    redis: &RedisConnection,
    email: &str,
) -> Result<bool, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire failed: {e}"))?;

    let key = format!("{KEY_RESEND}{}", normalize_email(email));

    let set_ok: Option<String> = redis::cmd("SET")
        .arg(&key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(RESEND_COOLDOWN_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET NX resend cooldown: {e}"))?;

    Ok(set_ok.is_some())
}
