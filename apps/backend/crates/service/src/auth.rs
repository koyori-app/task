use std::sync::OnceLock;

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use thiserror::Error;
use tracing::{error, warn};

use chrono::{DateTime, Utc};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::sea_query::Expr;
use sea_orm::sea_query::{LockType, Query, SelectStatement};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::{ServerError, internal_server_error};
use entity::device_tokens;
use entity::personal_token_tenants;
use entity::personal_tokens::{self, Entity as PersonalTokenEntity};
use sea_orm::DatabaseConnection;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
    #[error("unauthorized")]
    Unauthorized,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("forbidden")]
    Forbidden,
    #[error("email not verified")]
    EmailNotVerified,
    #[error("invalid verification token")]
    InvalidVerificationToken,
    #[error("no such user")]
    UserNotFound,
    #[error("email already verified")]
    EmailAlreadyVerified,
    #[error("duplicate email")]
    DuplicateEmail,
    #[error("too many requests")]
    TooManyRequests,
    #[error("account suspended")]
    Suspended,
    /// 認証メールジョブのキュー投入に失敗した（未認証ユーザーは残し再送 API で回復する）。
    #[error("verification email enqueue failed")]
    VerificationEmailEnqueueFailed(#[source] anyhow::Error),
    #[error("bad request")]
    BadRequest,
    #[error("passkey not found")]
    PasskeyNotFound,
    #[error("passkey limit exceeded")]
    PasskeyLimitExceeded,
    #[error("cannot remove last authentication method")]
    LastAuthMethod,
    #[error("possible credential clone detected")]
    PossibleCredentialClone,
    #[error("passkey registration already in progress")]
    RegistrationInProgress,
    #[error("webauthn error")]
    WebAuthn(#[from] webauthn_rs::prelude::WebauthnError),
    #[error("invalid 2fa code")]
    InvalidTwoFactorCode,
    #[error("2fa already enabled")]
    TwoFactorAlreadyEnabled,
    #[error("2fa not enabled")]
    TwoFactorNotEnabled,
    #[error("invalid password reset token")]
    InvalidPasswordResetToken,
    #[error("password reset token not found")]
    PasswordResetTokenNotFound,
    #[error("invalid current password")]
    InvalidCurrentPassword,
    #[error("password not set")]
    PasswordNotSet,
    #[error("recent authentication required")]
    ReauthRequired,
}

impl From<sea_orm::DbErr> for AuthError {
    fn from(err: sea_orm::DbErr) -> Self {
        AuthError::Internal(err.into())
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            AuthError::Internal(e) => {
                error!("auth error: {:#?}", e);
                internal_server_error().into_response()
            }
            AuthError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(ServerError {
                    message: "unauthorized".into(),
                }),
            )
                .into_response(),
            AuthError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                Json(ServerError {
                    message: "invalid-credentials".into(),
                }),
            )
                .into_response(),
            AuthError::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(ServerError {
                    message: "forbidden".into(),
                }),
            )
                .into_response(),
            AuthError::EmailNotVerified => (
                StatusCode::FORBIDDEN,
                Json(ServerError {
                    message: "email-not-verified".into(),
                }),
            )
                .into_response(),
            AuthError::InvalidVerificationToken => (
                StatusCode::BAD_REQUEST,
                Json(ServerError {
                    message: "invalid-verification-token".into(),
                }),
            )
                .into_response(),
            AuthError::UserNotFound => (
                StatusCode::NOT_FOUND,
                Json(ServerError {
                    message: "not-found".into(),
                }),
            )
                .into_response(),
            AuthError::EmailAlreadyVerified => (
                StatusCode::CONFLICT,
                Json(ServerError {
                    message: "email-already-verified".into(),
                }),
            )
                .into_response(),
            AuthError::DuplicateEmail => (
                StatusCode::CONFLICT,
                Json(ServerError {
                    message: "email-already-exists".into(),
                }),
            )
                .into_response(),
            AuthError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ServerError {
                    message: "too-many-requests".into(),
                }),
            )
                .into_response(),
            AuthError::Suspended => (
                StatusCode::FORBIDDEN,
                Json(ServerError {
                    message: "account-suspended".into(),
                }),
            )
                .into_response(),
            AuthError::VerificationEmailEnqueueFailed(e) => {
                warn!("verification email enqueue failed: {:#?}", e);
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ServerError {
                        message: "verification-email-enqueue-failed".into(),
                    }),
                )
                    .into_response()
            }
            AuthError::BadRequest => (
                StatusCode::BAD_REQUEST,
                Json(ServerError {
                    message: "bad-request".into(),
                }),
            )
                .into_response(),
            AuthError::PasskeyNotFound => (
                StatusCode::NOT_FOUND,
                Json(ServerError {
                    message: "passkey-not-found".into(),
                }),
            )
                .into_response(),
            AuthError::PasskeyLimitExceeded => (
                StatusCode::CONFLICT,
                Json(ServerError {
                    message: "passkey-limit-exceeded".into(),
                }),
            )
                .into_response(),
            AuthError::LastAuthMethod => (
                StatusCode::FORBIDDEN,
                Json(ServerError {
                    message: "last-auth-method".into(),
                }),
            )
                .into_response(),
            AuthError::PossibleCredentialClone => (
                StatusCode::UNAUTHORIZED,
                Json(ServerError {
                    message: "possible-credential-clone".into(),
                }),
            )
                .into_response(),
            AuthError::RegistrationInProgress => (
                StatusCode::CONFLICT,
                Json(ServerError {
                    message: "passkey-registration-in-progress".into(),
                }),
            )
                .into_response(),
            AuthError::WebAuthn(e) => {
                warn!("webauthn error: {e:?}");
                (
                    StatusCode::BAD_REQUEST,
                    Json(ServerError {
                        message: "webauthn-error".into(),
                    }),
                )
                    .into_response()
            }
            AuthError::InvalidTwoFactorCode => (
                StatusCode::UNAUTHORIZED,
                Json(ServerError {
                    message: "invalid-2fa-code".into(),
                }),
            )
                .into_response(),
            AuthError::TwoFactorAlreadyEnabled => (
                StatusCode::CONFLICT,
                Json(ServerError {
                    message: "2fa-already-enabled".into(),
                }),
            )
                .into_response(),
            AuthError::TwoFactorNotEnabled => (
                StatusCode::BAD_REQUEST,
                Json(ServerError {
                    message: "2fa-not-enabled".into(),
                }),
            )
                .into_response(),
            AuthError::InvalidPasswordResetToken => (
                StatusCode::BAD_REQUEST,
                Json(ServerError {
                    message: "invalid-password-reset-token".into(),
                }),
            )
                .into_response(),
            AuthError::PasswordResetTokenNotFound => (
                StatusCode::NOT_FOUND,
                Json(ServerError {
                    message: "password-reset-token-not-found".into(),
                }),
            )
                .into_response(),
            AuthError::InvalidCurrentPassword => (
                StatusCode::BAD_REQUEST,
                Json(ServerError {
                    message: "invalid-current-password".into(),
                }),
            )
                .into_response(),
            AuthError::PasswordNotSet => (
                StatusCode::BAD_REQUEST,
                Json(ServerError {
                    message: "password-not-set".into(),
                }),
            )
                .into_response(),
            AuthError::ReauthRequired => (
                StatusCode::FORBIDDEN,
                Json(ServerError {
                    message: "reauth-required".into(),
                }),
            )
                .into_response(),
        }
    }
}

/// salt の長さ（バイト）。argon2 0.5.3 の `SaltString::generate` と同じ 16 バイト（PHC の文字列では 22 字）。
const ARGON2_SALT_LEN: usize = 16;
const ARGON2_PRODUCTION_COSTS: (u32, u32, u32) = (131072, 3, 2);
const ARGON2_TEST_COSTS: (u32, u32, u32) = (8192, 1, 1);

fn argon2_test_mode() -> bool {
    static TEST_MODE: OnceLock<bool> = OnceLock::new();
    *TEST_MODE.get_or_init(|| {
        // Never permit reduced password hashing costs in release builds.
        cfg!(debug_assertions)
            && std::env::var("ARGON2_TEST_MODE")
                .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
                .unwrap_or(false)
    })
}

fn argon2_costs() -> (u32, u32, u32) {
    if argon2_test_mode() {
        ARGON2_TEST_COSTS
    } else {
        ARGON2_PRODUCTION_COSTS
    }
}

pub fn argon2_params() -> Result<Argon2<'static>, AuthError> {
    argon2_with_costs(argon2_costs())
}

fn argon2_with_costs(
    (memory_cost, time_cost, parallelism): (u32, u32, u32),
) -> Result<Argon2<'static>, AuthError> {
    let params = argon2::Params::new(
        memory_cost,
        time_cost,
        parallelism,
        None, // output length
    )
    .map_err(|e| AuthError::Internal(anyhow::anyhow!("argon2 params: {e}")))?;

    Ok(Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
    ))
}

/// パスワードをハッシュ化する関数
///
/// Argon2idアルゴリズムを使用し、ランダムなソルトを生成してハッシュ化します。
///
/// # Arguments
/// * `password` - ハッシュ化するパスワードの文字列
///
/// # Errors
/// * `AuthError::Internal` - ハッシュ化プロセスでエラーが発生した場合に返されます。
///
/// # Returns
/// * `Ok(String)` - ハッシュ化されたパスワードを含む文字
pub fn create_password_hash(password: &str) -> Result<String, AuthError> {
    password_hash_with_costs(password, argon2_costs())
}

fn password_hash_with_costs(password: &str, costs: (u32, u32, u32)) -> Result<String, AuthError> {
    let salt = crate::random::bytes::<ARGON2_SALT_LEN>();
    let argon2 = argon2_with_costs(costs)?;

    let hash = argon2
        .hash_password_with_salt(password.as_bytes(), &salt)
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("password hash: {e}")))?;

    Ok(hash.to_string())
}

/// 存在しないユーザー向けのダミーハッシュ。ログイン時に常に Argon2 検証を走らせ、
/// メールアドレスの有無による応答時間差（タイミング攻撃）を抑える。
const PRODUCTION_DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=131072,t=3,p=2$0UUArODQDWduujvFlpWtKg$GDp6SlCwV4PIue/EfTr+nJVjlFnycyxtCfnJMnjlIjU";

pub fn dummy_password_hash() -> Result<&'static str, AuthError> {
    if !argon2_test_mode() {
        return Ok(PRODUCTION_DUMMY_PASSWORD_HASH);
    }

    static TEST_DUMMY_PASSWORD_HASH: OnceLock<Result<String, String>> = OnceLock::new();
    match TEST_DUMMY_PASSWORD_HASH.get_or_init(|| {
        create_password_hash("test-dummy-password").map_err(|error| error.to_string())
    }) {
        Ok(hash) => Ok(hash.as_str()),
        Err(error) => Err(AuthError::Internal(anyhow::anyhow!(
            "test dummy password hash: {error}"
        ))),
    }
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, AuthError> {
    let parsed_hash = PasswordHash::new(password_hash)
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("invalid password hash: {e}")))?;

    let argon2 = argon2_params()?;
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// メール認証用トークンを生成する。
pub fn generate_email_verification_token() -> String {
    let mut buf = [0u8; 32];
    crate::random::fill(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

// --- Personal token helpers ---
type HmacSha256 = Hmac<Sha256>;

/// Device Token の接頭辞。Bearer の値がこれで始まれば PAT ではなく Device Token として引く。
pub const DEVICE_TOKEN_PREFIX: &str = "kdt_";

/// `<prefix><base64url>` 形式のトークンと、DBに保存するHMACハッシュを返す。
pub(crate) fn generate_prefixed_token(
    prefix: &str,
    secret: &str,
) -> Result<(String, String), AuthError> {
    let mut buf = [0u8; 32];
    crate::random::fill(&mut buf);
    let token = format!("{prefix}{}", URL_SAFE_NO_PAD.encode(buf));
    let token_hash = create_personal_token_hash(&token, secret)?;
    Ok((token, token_hash))
}

/// `pat_<base64url>` 形式のトークンと、DBに保存するHMACハッシュを返す。
pub fn generate_personal_token(secret: &str) -> Result<(String, String), AuthError> {
    generate_prefixed_token("pat_", secret)
}

/// `kdt_<base64url>` 形式の Device Token と、DBに保存するHMACハッシュを返す（PAT と同じハッシュ）。
pub fn generate_device_token(secret: &str) -> Result<(String, String), AuthError> {
    generate_prefixed_token(DEVICE_TOKEN_PREFIX, secret)
}

/// サーバー側で保持するトークンのハッシュを作る。
/// HMAC-SHA256(secret, token) を Base64URL でエンコードして返す。
pub fn create_personal_token_hash(token: &str, secret: &str) -> Result<String, AuthError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| AuthError::Internal(anyhow::anyhow!("hmac init: {e}")))?;
    mac.update(token.as_bytes());
    let result = mac.finalize().into_bytes();
    Ok(URL_SAFE_NO_PAD.encode(result.as_slice()))
}

/// DB から取得した PAT レコード（認証成功時）。
pub type PersonalTokenRecord = personal_tokens::Model;

/// Bearer トークンを検証し、有効な PAT レコードを返す。
pub async fn authenticate_personal_token(
    db: &DatabaseConnection,
    secret: &str,
    token_plaintext: &str,
) -> Result<PersonalTokenRecord, AuthError> {
    let token_hash = create_personal_token_hash(token_plaintext, secret)?;

    let token = PersonalTokenEntity::find()
        .filter(personal_tokens::Column::TokenHash.eq(token_hash))
        .one(db)
        .await?
        .ok_or(AuthError::Unauthorized)?;

    // OAuth の許可の行は PAT として通さない。平文は誰も持たないが、コードでも固定する
    // （許可は `oauth_server::grant::authenticate_access_token` が `/mcp` でだけ通す）
    if token.revoked || token.oauth_client_id.is_some() {
        return Err(AuthError::Unauthorized);
    }

    if let Some(expires) = &token.expires_at
        && expires < &Utc::now().fixed_offset()
    {
        return Err(AuthError::Unauthorized);
    }

    Ok(token)
}

/// PAT が結び付けられたテナント（`tenant_id` 昇順）。
pub async fn personal_token_tenant_ids<C: ConnectionTrait>(
    conn: &C,
    token_id: Uuid,
) -> Result<Vec<Uuid>, DbErr> {
    personal_token_tenants::Entity::find()
        .select_only()
        .column(personal_token_tenants::Column::TenantId)
        .filter(personal_token_tenants::Column::TokenId.eq(token_id))
        .order_by_asc(personal_token_tenants::Column::TenantId)
        .into_tuple::<Uuid>()
        .all(conn)
        .await
}

/// 複数の PAT の束縛を 1 クエリで引く。各値は昇順。束縛の無い PAT はキーが無い。
pub async fn personal_token_tenant_ids_by_token<C: ConnectionTrait>(
    conn: &C,
    token_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<Uuid>>, DbErr> {
    let mut out: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    if token_ids.is_empty() {
        return Ok(out);
    }
    let rows = personal_token_tenants::Entity::find()
        .filter(personal_token_tenants::Column::TokenId.is_in(token_ids.iter().copied()))
        .order_by_asc(personal_token_tenants::Column::TenantId)
        .all(conn)
        .await?;
    for row in rows {
        out.entry(row.token_id).or_default().push(row.tenant_id);
    }
    Ok(out)
}

/// PAT をテナントに結び付ける。`tenant_ids` は重複を含まないこと（主キー違反になる）。
pub async fn bind_personal_token_tenants<C: ConnectionTrait>(
    conn: &C,
    token_id: Uuid,
    tenant_ids: &[Uuid],
) -> Result<(), DbErr> {
    if tenant_ids.is_empty() {
        return Ok(());
    }
    personal_token_tenants::Entity::insert_many(tenant_ids.iter().map(|tenant_id| {
        personal_token_tenants::ActiveModel {
            token_id: sea_orm::ActiveValue::Set(token_id),
            tenant_id: sea_orm::ActiveValue::Set(*tenant_id),
        }
    }))
    .exec_without_returning(conn)
    .await?;
    Ok(())
}

/// そのテナントに結び付いた PAT の id を返す副問い合わせ（`Column::Id.in_subquery(..)` に渡す）。
pub fn token_ids_bound_to(tenant_id: Uuid) -> SelectStatement {
    Query::select()
        .column(personal_token_tenants::Column::TokenId)
        .from(personal_token_tenants::Entity)
        .and_where(personal_token_tenants::Column::TenantId.eq(tenant_id))
        .to_owned()
}

/// そのテナントへの束縛をすべての PAT から外し、束縛が 1 つも残らなくなった PAT を取り消す。
/// ほかのテナントにも結び付いた PAT は生かす。取り消した PAT（取り消し前の行）を返す。
///
/// 一括失効とテナントの削除で同じ処理を使う。テナントの削除は束縛を `ON DELETE CASCADE` で
/// 消すので、行だけ `revoked = false` で残らないよう、テナントを消す前に同じトランザクションで呼ぶ。
///
/// 対象のトークン行を id 順にロックしてから束縛を外す。ロックしないと、同じトークンに結び付いた
/// 別テナントの削除・一括失効が同時に走ったとき、互いに相手の束縛をまだ残っているものと見て
/// 取り消しを見送り、束縛が 0 件のまま `revoked = false` で残る。
///
/// 呼び出し側は先にテナント行をロックしてから呼ぶ（削除は FOR UPDATE、一括失効は FOR SHARE）。
/// 「テナント行 → トークン行 → 束縛行」の順をそろえるためで、同じテナントの削除と一括失効が
/// 逆順に待ち合ってデッドロックするのを防ぐ。この関数の中でテナント行を FOR UPDATE に上げると、
/// FOR SHARE を持って入る一括失効どうしがデッドロックするので、ここではロックしない。
pub async fn unbind_tenant_from_tokens<C: ConnectionTrait>(
    conn: &C,
    tenant_id: Uuid,
) -> Result<Vec<personal_tokens::Model>, DbErr> {
    let tokens = PersonalTokenEntity::find()
        .filter(personal_tokens::Column::Id.in_subquery(token_ids_bound_to(tenant_id)))
        .filter(personal_tokens::Column::Revoked.eq(false))
        .order_by_asc(personal_tokens::Column::Id)
        .lock(LockType::NoKeyUpdate)
        .all(conn)
        .await?;
    personal_token_tenants::Entity::delete_many()
        .filter(personal_token_tenants::Column::TenantId.eq(tenant_id))
        .exec(conn)
        .await?;
    let still_bound: std::collections::HashSet<Uuid> = personal_token_tenants::Entity::find()
        .filter(personal_token_tenants::Column::TokenId.is_in(tokens.iter().map(|token| token.id)))
        .all(conn)
        .await?
        .into_iter()
        .map(|binding| binding.token_id)
        .collect();
    let orphaned: Vec<personal_tokens::Model> = tokens
        .into_iter()
        .filter(|token| !still_bound.contains(&token.id))
        .collect();
    PersonalTokenEntity::update_many()
        .col_expr(personal_tokens::Column::Revoked, Expr::value(true))
        .filter(personal_tokens::Column::Id.is_in(orphaned.iter().map(|token| token.id)))
        .exec(conn)
        .await?;
    Ok(orphaned)
}

/// 利用者の PAT をまだ取り消していないものすべて取り消す。
///
/// `unbind_tenant_from_tokens` はトークン行を id 昇順でロックするので、こちらも同じ順で取る。
/// 走査順で行を更新すると、同じ利用者の PAT 2 本が 2 つのテナントに結び付いた状態で
/// 「テナントの削除」と重なったとき、取る順が食い違ってデッドロック（`40P01`）になる。
/// 副問い合わせの `ORDER BY id FOR NO KEY UPDATE` が id 昇順にロックしてから更新するので、
/// トランザクションの外からでも 1 文でこの順を守れる。
/// 利用者単位の取り消し（パスワード変更・利用者の停止・BOT の停止）はこれを使う。
pub async fn revoke_user_personal_tokens<C: ConnectionTrait>(
    conn: &C,
    user_id: Uuid,
) -> Result<(), DbErr> {
    let locked_in_id_order = Query::select()
        .column(personal_tokens::Column::Id)
        .from(personal_tokens::Entity)
        .and_where(personal_tokens::Column::UserId.eq(user_id))
        .and_where(personal_tokens::Column::Revoked.eq(false))
        .order_by(personal_tokens::Column::Id, sea_orm::Order::Asc)
        .lock(LockType::NoKeyUpdate)
        .to_owned();
    PersonalTokenEntity::update_many()
        .col_expr(personal_tokens::Column::Revoked, Expr::value(true))
        .filter(personal_tokens::Column::Id.in_subquery(locked_in_id_order))
        .exec(conn)
        .await?;
    Ok(())
}

/// PAT の `last_used_at` を書き直す間隔。CLI や BOT は短い間隔で連続して叩くので、毎回は書かない。
const PERSONAL_TOKEN_LAST_USED_WRITE_INTERVAL_SECS: i64 = 300;

/// 前回の使用から書き直す間隔が空いたか。未使用（None）は常に真。
fn last_used_is_stale(last_used_at: Option<DateTimeWithTimeZone>, now: DateTime<Utc>) -> bool {
    last_used_at.is_none_or(|t| {
        now.signed_duration_since(t).num_seconds() >= PERSONAL_TOKEN_LAST_USED_WRITE_INTERVAL_SECS
    })
}

/// 認証に成功した PAT の `last_used_at` を、間隔を空けて更新する。
///
/// 行は `update_many` で `id` を絞って書く。SELECT と UPDATE の間に行が消えても（テナントや
/// 利用者の削除による Cascade）0 件更新で済み、使用記録のために要求を落とさない。
pub async fn record_personal_token_use(
    db: &DatabaseConnection,
    token: &PersonalTokenRecord,
) -> Result<(), sea_orm::DbErr> {
    let now = Utc::now();
    if last_used_is_stale(token.last_used_at, now) {
        personal_tokens::Entity::update_many()
            .col_expr(
                personal_tokens::Column::LastUsedAt,
                Expr::value(now.fixed_offset()),
            )
            .filter(personal_tokens::Column::Id.eq(token.id))
            .exec(db)
            .await?;
    }
    Ok(())
}

/// Bearer の Device Token を検証し、有効なレコードを返す。
/// `users.sessions_revoked_at` との比較は利用者を引く側（extractor）で行う。
pub async fn authenticate_device_token(
    db: &DatabaseConnection,
    secret: &str,
    token_plaintext: &str,
) -> Result<device_tokens::Model, AuthError> {
    let token_hash = create_personal_token_hash(token_plaintext, secret)?;

    let token = device_tokens::Entity::find()
        .filter(device_tokens::Column::TokenHash.eq(token_hash))
        .one(db)
        .await?
        .ok_or(AuthError::Unauthorized)?;

    if token.revoked_at.is_some() || token.expires_at < Utc::now().fixed_offset() {
        return Err(AuthError::Unauthorized);
    }

    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_used_is_stale_respects_the_write_interval() {
        let now = Utc::now();
        let ago = |secs: i64| Some((now - chrono::Duration::seconds(secs)).fixed_offset());
        assert!(last_used_is_stale(None, now));
        assert!(!last_used_is_stale(ago(299), now));
        assert!(last_used_is_stale(ago(300), now));
        assert!(last_used_is_stale(ago(301), now));
        // 時計のずれで未来の値が入っていても書かない
        assert!(!last_used_is_stale(ago(-10), now));
    }

    #[test]
    fn dummy_password_hash_is_valid_argon2() {
        let hash = dummy_password_hash().unwrap();
        assert!(PasswordHash::new(hash).is_ok());
        assert!(!verify_password("wrong-password", hash).unwrap());
        if argon2_test_mode() {
            assert!(hash.contains("m=8192,t=1,p=1"));
        }
    }

    /// argon2 0.5.3 が作った PHC 文字列（本番の強さと、試験の強さ）。上げた後の版でも、モードに
    /// よらず、そのまま照合できる。強さは文字列の中から読むので、今のモードと違っていても通る。
    const SAMPLE_PASSWORD: &str = "correct horse battery staple";
    const PHC_PRODUCTION_ARGON2_0_5_3: &str = "$argon2id$v=19$m=131072,t=3,p=2$bwD5A/9hzRwJuN7wOkPEVg$qA/s4uqZ7dmDK7boA5o6KJ4czgUturHxaXuFI97cz88";
    const PHC_LOW_COST_ARGON2_0_5_3: &str = "$argon2id$v=19$m=8192,t=1,p=1$KBrnfqLAjUu9JcmphHH+5w$1barNo3OhDUE5H0IIa1Ouieysjwbax4EX0wPlChWYKU";

    #[test]
    fn phc_hashes_made_by_argon2_0_5_3_still_verify() {
        for hash in [PHC_PRODUCTION_ARGON2_0_5_3, PHC_LOW_COST_ARGON2_0_5_3] {
            assert!(verify_password(SAMPLE_PASSWORD, hash).unwrap(), "{hash}");
            assert!(!verify_password("wrong-password", hash).unwrap(), "{hash}");
        }
    }

    #[test]
    fn invalid_phc_string_is_an_internal_error_not_a_mismatch() {
        assert!(matches!(
            verify_password(SAMPLE_PASSWORD, "not-a-phc-string"),
            Err(AuthError::Internal(_))
        ));
    }

    /// 新しく作る hash の既定の強さ（algorithm・version・m・t・p）と形（salt 16 バイト・出力 32 バイト）が、
    /// 0.5.3 のときと同じ。変えるなら、既存の hash との段差（再 hash の方針）を決めてからにする。
    #[test]
    fn new_hashes_keep_the_default_strength_and_shape() {
        let production =
            password_hash_with_costs(SAMPLE_PASSWORD, ARGON2_PRODUCTION_COSTS).unwrap();
        assert!(
            production.starts_with("$argon2id$v=19$m=131072,t=3,p=2$"),
            "{production}"
        );
        let low = password_hash_with_costs(SAMPLE_PASSWORD, ARGON2_TEST_COSTS).unwrap();
        assert!(low.starts_with("$argon2id$v=19$m=8192,t=1,p=1$"), "{low}");

        for (hash, sample) in [
            (&production, PHC_PRODUCTION_ARGON2_0_5_3),
            (&low, PHC_LOW_COST_ARGON2_0_5_3),
        ] {
            let parts: Vec<&str> = hash.split('$').collect();
            let sample_parts: Vec<&str> = sample.split('$').collect();
            assert_eq!(parts.len(), sample_parts.len());
            assert_eq!(parts[4].len(), sample_parts[4].len(), "salt の長さ");
            assert_eq!(parts[5].len(), sample_parts[5].len(), "出力の長さ");
            assert!(verify_password(SAMPLE_PASSWORD, hash).unwrap());
        }

        // 今のモードで作る hash も、そのモードの強さになる。
        let current = create_password_hash(SAMPLE_PASSWORD).unwrap();
        let expected = if argon2_test_mode() {
            "$argon2id$v=19$m=8192,t=1,p=1$"
        } else {
            "$argon2id$v=19$m=131072,t=3,p=2$"
        };
        assert!(current.starts_with(expected), "{current}");
    }

    #[test]
    fn every_hash_and_token_draws_fresh_randomness() {
        let first = password_hash_with_costs(SAMPLE_PASSWORD, ARGON2_TEST_COSTS).unwrap();
        let second = password_hash_with_costs(SAMPLE_PASSWORD, ARGON2_TEST_COSTS).unwrap();
        assert_ne!(first, second, "同じパスワードでも salt が違う");
        assert_ne!(first.split('$').nth(4), second.split('$').nth(4));

        let token = generate_email_verification_token();
        assert_eq!(token.len(), 43, "32 バイトの base64url");
        assert_ne!(token, generate_email_verification_token());

        let (pat, pat_hash) = generate_personal_token("secret").unwrap();
        let (other, other_hash) = generate_personal_token("secret").unwrap();
        assert_ne!(pat, other);
        assert_ne!(pat_hash, other_hash);
        let (device, _) = generate_device_token("secret").unwrap();
        assert!(device.starts_with(DEVICE_TOKEN_PREFIX));
        assert_ne!(device, generate_device_token("secret").unwrap().0);
    }
}
