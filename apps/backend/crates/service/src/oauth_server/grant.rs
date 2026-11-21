//! 認可コード、許可（grant）、アクセストークンとリフレッシュトークン。
//!
//! 許可 1 件は `personal_tokens` の 1 行（`oauth_client_id` が NULL でない行）で、scope・テナントの
//! 束縛・失効は PAT と同じ行と同じ経路で扱う。アクセストークンとリフレッシュトークンは
//! `oauth_tokens` の行で、使うたびに親の許可を引く。
//!
//! ロックの順は「テナントの行 → 利用者の行 → `personal_tokens` の行 → `personal_token_tenants` /
//! `oauth_tokens` の行」。`oauth_tokens` をロックしてから `personal_tokens` をロックする処理は作らない
//! （[`crate::auth::unbind_tenant_from_tokens`] と逆順に待ち合わない）。

use std::sync::LazyLock;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use common::cache::redis::RedisConnection;
use common::settings::Settings;
use sea_orm::sea_query::{Expr, LockType};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use entity::scopes::{Scope, ScopeList};
use entity::{oauth_tokens, personal_tokens, users};

use super::client::client_is_allowed;
use super::{
    ACCESS_TOKEN_PREFIX, ACCESS_TOKEN_TTL_SECS, CODE_TTL_SECS, REFRESH_TOKEN_PREFIX,
    REFRESH_TOKEN_TTL_DAYS,
};
use crate::auth::{self, AuthError};

const KEY_CODE: &str = "oauth_server:code:";
const KEY_CODE_USED: &str = "oauth_server:code_used:";
const KEY_CODE_REPLAYED: &str = "oauth_server:code_replayed:";
/// 誰にも渡さない、許可の行の `token_hash` を埋めるための接頭辞。
const GRANT_PLACEHOLDER_PREFIX: &str = "mcpgr_";

/// 認可コードに紐づけて Redis に置く値。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingAuthorization {
    /// 許可の id。承認の時点で決める（コードの二重使用を、交換の順によらず取り消せるようにする）
    pub grant_id: Uuid,
    pub user_id: Uuid,
    pub client_id: String,
    pub client_name: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub scopes: Vec<Scope>,
    pub tenant_ids: Vec<Uuid>,
    pub allow_refresh: bool,
    pub issued_at_ms: i64,
}

/// 認可コードの状態。
#[derive(Debug, PartialEq, Eq)]
pub enum CodeState {
    /// 未使用だった。この呼び出しで消費した
    Fresh(Box<PendingAuthorization>),
    /// 使用済み。値は、そのコードから作る（作った）許可の id
    Used(Uuid),
    /// 無い・期限切れ
    Unknown,
}

/// 発行したトークンの組。平文はこの値でだけ返る。
#[derive(Debug)]
pub struct IssuedTokens {
    pub access_token: String,
    /// クライアントが `refresh_token` の grant を宣言していないときは無い
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    pub scopes: Vec<Scope>,
}

#[derive(Debug, thiserror::Error)]
pub enum GrantError {
    /// コード・リフレッシュトークン・許可が使えない（クライアントには `invalid_grant`）。
    #[error("invalid grant")]
    InvalidGrant,
    #[error("database error")]
    Db(#[from] DbErr),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

fn internal(error: AuthError) -> GrantError {
    GrantError::Internal(anyhow::anyhow!("oauth token hash: {error}"))
}

fn code_key(code: &str) -> String {
    format!("{KEY_CODE}{code}")
}

fn code_used_key(code: &str) -> String {
    format!("{KEY_CODE_USED}{code}")
}

fn code_replayed_key(code: &str) -> String {
    format!("{KEY_CODE_REPLAYED}{code}")
}

/// コードの消費と使用済みの記録を 1 本で行う。間に別の交換が割り込めない。
/// 返却: `F<保存した値>` = 未使用だった、`U<許可の id>` = 使用済み、nil = 無い。
static TAKE_CODE_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
        local payload = redis.call('GETDEL', KEYS[1])
        if payload then
            redis.call('SET', KEYS[2], cjson.decode(payload).grant_id, 'EX', ARGV[1])
            return 'F' .. payload
        end
        local used = redis.call('GET', KEYS[2])
        if used then
            redis.call('SET', KEYS[3], '1', 'EX', ARGV[1])
            return 'U' .. used
        end
        return false
        "#,
    )
});

/// 32 バイト乱数の認可コードを発行し、TTL 付きで保存する。
pub async fn issue_code(
    redis: &RedisConnection,
    pending: &PendingAuthorization,
) -> Result<String, anyhow::Error> {
    let mut buf = [0u8; 32];
    crate::random::fill(&mut buf);
    let code = URL_SAFE_NO_PAD.encode(buf);

    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let _: () = redis::cmd("SET")
        .arg(code_key(&code))
        .arg(serde_json::to_string(pending)?)
        .arg("EX")
        .arg(CODE_TTL_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET oauth code: {e}"))?;
    Ok(code)
}

/// 認可コードを一度きり取り出す。使用済みのコードがもう一度来たら、その印を残す。
pub async fn take_code(redis: &RedisConnection, code: &str) -> Result<CodeState, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let taken: Option<String> = TAKE_CODE_SCRIPT
        .key(code_key(code))
        .key(code_used_key(code))
        .key(code_replayed_key(code))
        .arg(CODE_TTL_SECS)
        .invoke_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis take oauth code: {e}"))?;
    let Some(taken) = taken else {
        return Ok(CodeState::Unknown);
    };
    if let Some(payload) = taken.strip_prefix('F') {
        let pending =
            serde_json::from_str(payload).map_err(|e| anyhow::anyhow!("oauth code decode: {e}"))?;
        return Ok(CodeState::Fresh(Box::new(pending)));
    }
    let grant_id = taken
        .strip_prefix('U')
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| anyhow::anyhow!("oauth code state decode"))?;
    Ok(CodeState::Used(grant_id))
}

/// 使用済みのコードがもう一度来たか。交換の commit のあとで読む。
pub async fn code_was_replayed(redis: &RedisConnection, code: &str) -> Result<bool, anyhow::Error> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let exists: i64 = redis::cmd("EXISTS")
        .arg(code_replayed_key(code))
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis EXISTS oauth code replay: {e}"))?;
    Ok(exists > 0)
}

/// 消費したコードから許可を作り、作ったあとで二重使用を確かめる（コードの交換の本体）。
///
/// 同じコードの 2 つの交換は、後から来た側が印を置いてから許可を取り消す
/// （[`take_code`] の `Used`）。その取り消しが、先に来た側の commit より前に走ると空振りになる。
/// そこで、先に来た側は commit のあとで印を読み、在れば自分で取り消す。どの順でも許可は残らない。
/// 印を読めなかったとき（Redis の失敗）も、確かめられないまま有効な許可を残さない。
pub async fn create_grant_for_code(
    redis: &RedisConnection,
    db: &DatabaseConnection,
    secret: &str,
    code: &str,
    pending: &PendingAuthorization,
) -> Result<IssuedTokens, GrantError> {
    let tokens = create_grant(db, secret, pending).await?;
    match code_was_replayed(redis, code).await {
        Ok(false) => Ok(tokens),
        Ok(true) => {
            revoke_grant(db, pending.grant_id).await?;
            Err(GrantError::InvalidGrant)
        }
        Err(error) => {
            revoke_grant(db, pending.grant_id).await?;
            Err(GrantError::Internal(error))
        }
    }
}

/// `oauth_tokens` に 1 行足し、平文を返す。
async fn insert_token<C: ConnectionTrait>(
    conn: &C,
    secret: &str,
    grant_id: Uuid,
    kind: &str,
    now: DateTime<Utc>,
) -> Result<String, GrantError> {
    let (prefix, expires_at) = if kind == oauth_tokens::KIND_REFRESH {
        (
            REFRESH_TOKEN_PREFIX,
            now + Duration::days(REFRESH_TOKEN_TTL_DAYS),
        )
    } else {
        (
            ACCESS_TOKEN_PREFIX,
            now + Duration::seconds(ACCESS_TOKEN_TTL_SECS),
        )
    };
    let (token, token_hash) = auth::generate_prefixed_token(prefix, secret).map_err(internal)?;
    oauth_tokens::ActiveModel {
        id: Set(Uuid::new_v4()),
        grant_id: Set(grant_id),
        kind: Set(kind.to_string()),
        token_hash: Set(token_hash),
        expires_at: Set(expires_at.fixed_offset()),
        consumed_at: Set(None),
        // 試験の表は `sync()` が entity から作るので `DEFAULT now()` が付かない。明示して入れる
        created_at: Set(now.fixed_offset()),
    }
    .insert(conn)
    .await?;
    Ok(token)
}

/// 許可の期限。リフレッシュトークンを出さない許可は、アクセストークンと同じ寿命にする。
fn grant_expiry(now: DateTime<Utc>, allow_refresh: bool) -> DateTime<Utc> {
    if allow_refresh {
        now + Duration::days(REFRESH_TOKEN_TTL_DAYS)
    } else {
        now + Duration::seconds(ACCESS_TOKEN_TTL_SECS)
    }
}

/// 交換の時点で使える利用者か。
fn user_can_hold_grants(user: &users::Model) -> bool {
    !user.is_bot() && !user.is_suspended
}

/// 許可の行と最初のトークンの組を作る（コードの交換）。
///
/// 利用者の行を `FOR SHARE` で持つ。パスワードの再設定は同じ行を更新してから PAT を取り消すので、
/// 再設定が先なら `sessions_revoked_at` で止まり、交換が先なら再設定が新しい許可も取り消す。
pub async fn create_grant(
    db: &DatabaseConnection,
    secret: &str,
    pending: &PendingAuthorization,
) -> Result<IssuedTokens, GrantError> {
    let txn = db.begin().await?;
    let user = users::Entity::find_by_id(pending.user_id)
        .lock(LockType::Share)
        .one(&txn)
        .await?
        .ok_or(GrantError::InvalidGrant)?;
    if pending.issued_at_ms <= 0
        || !user_can_hold_grants(&user)
        || user
            .sessions_revoked_at
            .is_some_and(|revoked_at| revoked_at.timestamp_millis() >= pending.issued_at_ms)
    {
        return Err(GrantError::InvalidGrant);
    }

    let now = Utc::now();
    // 許可の行の `token_hash` に対応する平文は誰も持たない
    let (_, placeholder_hash) =
        auth::generate_prefixed_token(GRANT_PLACEHOLDER_PREFIX, secret).map_err(internal)?;
    personal_tokens::ActiveModel {
        id: Set(pending.grant_id),
        name: Set(pending.client_name.clone()),
        token_last_four: Set(String::new()),
        token_hash: Set(placeholder_hash),
        expires_at: Set(Some(
            grant_expiry(now, pending.allow_refresh).fixed_offset(),
        )),
        last_used_at: Set(None),
        revoked: Set(false),
        user_id: Set(pending.user_id),
        scopes: Set(ScopeList(pending.scopes.clone())),
        allowed_project_ids: Set(None),
        oauth_client_id: Set(Some(pending.client_id.clone())),
    }
    .insert(&txn)
    .await?;
    auth::bind_personal_token_tenants(&txn, pending.grant_id, &pending.tenant_ids).await?;

    let access_token = insert_token(
        &txn,
        secret,
        pending.grant_id,
        oauth_tokens::KIND_ACCESS,
        now,
    )
    .await?;
    let refresh_token = if pending.allow_refresh {
        Some(
            insert_token(
                &txn,
                secret,
                pending.grant_id,
                oauth_tokens::KIND_REFRESH,
                now,
            )
            .await?,
        )
    } else {
        None
    };
    txn.commit().await?;

    Ok(IssuedTokens {
        access_token,
        refresh_token,
        expires_in: ACCESS_TOKEN_TTL_SECS,
        scopes: pending.scopes.clone(),
    })
}

/// リフレッシュトークンを回し、新しい組を返す。
///
/// 回し済みのトークンがもう一度来たら、猶予なく許可ごと取り消す（応答が届かなかった再送と
/// 同時の更新も含む）。同じトークンの同時の要求は、許可の行のロックで直列になる。
pub async fn rotate_refresh_token(
    db: &DatabaseConnection,
    settings: &Settings,
    refresh_token: &str,
    client_id: &str,
) -> Result<IssuedTokens, GrantError> {
    if !refresh_token.starts_with(REFRESH_TOKEN_PREFIX) {
        return Err(GrantError::InvalidGrant);
    }
    let secret = &settings.personal_token_secret;
    let token_hash = auth::create_personal_token_hash(refresh_token, secret).map_err(internal)?;

    let txn = db.begin().await?;
    let find_token = || {
        oauth_tokens::Entity::find()
            .filter(oauth_tokens::Column::TokenHash.eq(token_hash.as_str()))
            .filter(oauth_tokens::Column::Kind.eq(oauth_tokens::KIND_REFRESH))
    };
    // 先にロックなしで親を知り、許可の行 → トークンの行の順にロックする
    let unlocked = find_token()
        .one(&txn)
        .await?
        .ok_or(GrantError::InvalidGrant)?;
    let grant = personal_tokens::Entity::find_by_id(unlocked.grant_id)
        .lock(LockType::NoKeyUpdate)
        .one(&txn)
        .await?
        .ok_or(GrantError::InvalidGrant)?;
    let token = find_token()
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or(GrantError::InvalidGrant)?;

    if token.consumed_at.is_some() {
        revoke_grant(&txn, grant.id).await?;
        txn.commit().await?;
        return Err(GrantError::InvalidGrant);
    }

    let now = Utc::now();
    let now_fixed = now.fixed_offset();
    let expired = |expires_at: Option<&sea_orm::prelude::DateTimeWithTimeZone>| {
        expires_at.is_none_or(|expires_at| *expires_at <= now_fixed)
    };
    if expired(Some(&token.expires_at))
        || grant.revoked
        || expired(grant.expires_at.as_ref())
        || grant.oauth_client_id.as_deref() != Some(client_id)
        || !client_is_allowed(settings, client_id)
        || auth::personal_token_tenant_ids(&txn, grant.id)
            .await?
            .is_empty()
    {
        return Err(GrantError::InvalidGrant);
    }
    let user = users::Entity::find_by_id(grant.user_id)
        .one(&txn)
        .await?
        .ok_or(GrantError::InvalidGrant)?;
    if !user_can_hold_grants(&user) {
        return Err(GrantError::InvalidGrant);
    }

    oauth_tokens::Entity::update_many()
        .col_expr(oauth_tokens::Column::ConsumedAt, Expr::value(now_fixed))
        .filter(oauth_tokens::Column::Id.eq(token.id))
        .exec(&txn)
        .await?;
    let access_token = insert_token(&txn, secret, grant.id, oauth_tokens::KIND_ACCESS, now).await?;
    let new_refresh_token =
        insert_token(&txn, secret, grant.id, oauth_tokens::KIND_REFRESH, now).await?;
    personal_tokens::Entity::update_many()
        .col_expr(
            personal_tokens::Column::ExpiresAt,
            Expr::value(grant_expiry(now, true).fixed_offset()),
        )
        .filter(personal_tokens::Column::Id.eq(grant.id))
        .exec(&txn)
        .await?;
    // 期限の切れたトークンの行を掃除する（回し済みのリフレッシュトークンも期限までは残し、
    // 再利用を見つけられるようにする）
    oauth_tokens::Entity::delete_many()
        .filter(oauth_tokens::Column::GrantId.eq(grant.id))
        .filter(oauth_tokens::Column::ExpiresAt.lt(now_fixed))
        .exec(&txn)
        .await?;
    txn.commit().await?;

    Ok(IssuedTokens {
        access_token,
        refresh_token: Some(new_refresh_token),
        expires_in: ACCESS_TOKEN_TTL_SECS,
        scopes: grant.scopes.0,
    })
}

/// Bearer のアクセストークンを検証し、親の許可の行を返す。
///
/// 許可リストから外したクライアントの許可は、外した時点で使えなくなる
/// （リストに戻せば、期限の内なら再び使える）。束縛・停止などの検査は、PAT と同じ経路が続けて行う。
pub async fn authenticate_access_token(
    db: &DatabaseConnection,
    settings: &Settings,
    token: &str,
) -> Result<personal_tokens::Model, AuthError> {
    let token_hash = auth::create_personal_token_hash(token, &settings.personal_token_secret)?;
    let now = Utc::now().fixed_offset();
    let token = oauth_tokens::Entity::find()
        .filter(oauth_tokens::Column::TokenHash.eq(token_hash))
        .filter(oauth_tokens::Column::Kind.eq(oauth_tokens::KIND_ACCESS))
        .one(db)
        .await?
        .ok_or(AuthError::Unauthorized)?;
    if token.expires_at <= now {
        return Err(AuthError::Unauthorized);
    }
    let grant = personal_tokens::Entity::find_by_id(token.grant_id)
        .one(db)
        .await?
        .ok_or(AuthError::Unauthorized)?;
    let Some(client_id) = grant.oauth_client_id.as_deref() else {
        return Err(AuthError::Unauthorized);
    };
    if grant.revoked
        || grant.expires_at.is_none_or(|expires_at| expires_at <= now)
        || !client_is_allowed(settings, client_id)
    {
        return Err(AuthError::Unauthorized);
    }
    Ok(grant)
}

/// 許可を取り消し、そのトークンの行を消す。PAT の行（`oauth_client_id` が NULL）には触れない。
/// 行が無くてもエラーにしない。
pub async fn revoke_grant<C: ConnectionTrait>(conn: &C, grant_id: Uuid) -> Result<(), DbErr> {
    personal_tokens::Entity::update_many()
        .col_expr(personal_tokens::Column::Revoked, Expr::value(true))
        .filter(personal_tokens::Column::Id.eq(grant_id))
        .filter(personal_tokens::Column::OauthClientId.is_not_null())
        .exec(conn)
        .await?;
    oauth_tokens::Entity::delete_many()
        .filter(oauth_tokens::Column::GrantId.eq(grant_id))
        .exec(conn)
        .await?;
    Ok(())
}

/// アクセストークンかリフレッシュトークンから、その許可を取り消す（RFC 7009）。
/// 見つからなくてもエラーにしない。`client_id` が渡されていて許可のものと違えば、何もしない。
pub async fn revoke_by_token(
    db: &DatabaseConnection,
    secret: &str,
    token: &str,
    client_id: Option<&str>,
) -> Result<(), GrantError> {
    if !token.starts_with(ACCESS_TOKEN_PREFIX) && !token.starts_with(REFRESH_TOKEN_PREFIX) {
        return Ok(());
    }
    let token_hash = auth::create_personal_token_hash(token, secret).map_err(internal)?;
    let Some(token) = oauth_tokens::Entity::find()
        .filter(oauth_tokens::Column::TokenHash.eq(token_hash))
        .one(db)
        .await?
    else {
        return Ok(());
    };
    let Some(grant) = personal_tokens::Entity::find_by_id(token.grant_id)
        .one(db)
        .await?
    else {
        return Ok(());
    };
    if client_id.is_some_and(|client_id| grant.oauth_client_id.as_deref() != Some(client_id)) {
        return Ok(());
    }
    revoke_grant(db, grant.id).await?;
    Ok(())
}

/// 利用者の使える許可と、その束縛（昇順）。失効済み・期限切れ・束縛の尽きたもの・
/// 許可リストに無いクライアントのものは出さない。
pub async fn list_grants(
    db: &DatabaseConnection,
    settings: &Settings,
    user_id: Uuid,
) -> Result<Vec<(personal_tokens::Model, Vec<Uuid>)>, DbErr> {
    let grants = personal_tokens::Entity::find()
        .filter(personal_tokens::Column::UserId.eq(user_id))
        .filter(personal_tokens::Column::OauthClientId.is_not_null())
        .filter(personal_tokens::Column::Revoked.eq(false))
        .filter(personal_tokens::Column::ExpiresAt.gt(Utc::now().fixed_offset()))
        .order_by_asc(personal_tokens::Column::Name)
        .order_by_asc(personal_tokens::Column::Id)
        .all(db)
        .await?;
    let ids: Vec<Uuid> = grants.iter().map(|grant| grant.id).collect();
    let mut bindings = auth::personal_token_tenant_ids_by_token(db, &ids).await?;
    Ok(grants
        .into_iter()
        .filter(|grant| {
            grant
                .oauth_client_id
                .as_deref()
                .is_some_and(|client_id| client_is_allowed(settings, client_id))
        })
        .filter_map(|grant| {
            let tenant_ids = bindings.remove(&grant.id)?;
            Some((grant, tenant_ids))
        })
        .collect())
}

/// 自分の許可を 1 件引く（設定画面の解除）。PAT の行と他人の許可は `None`。
pub async fn find_owned_grant(
    db: &DatabaseConnection,
    user_id: Uuid,
    grant_id: Uuid,
) -> Result<Option<personal_tokens::Model>, DbErr> {
    personal_tokens::Entity::find_by_id(grant_id)
        .filter(personal_tokens::Column::UserId.eq(user_id))
        .filter(personal_tokens::Column::OauthClientId.is_not_null())
        .one(db)
        .await
}
