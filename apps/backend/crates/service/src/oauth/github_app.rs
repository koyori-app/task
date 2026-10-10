//! GitHub App のユーザーアクセストークン（PR を利用者本人の名義で Approve する連携）。
//!
//! 連携は `oauth_connections` に provider = `github_app` で保存する（ログイン用の連携と
//! 同じ表・同じ暗号鍵）。トークンは既定で 8 時間で切れ、リフレッシュトークンは更新の
//! たびに回転する（古いものは使えなくなる）。

use auth_core::client::refresh_access_token;
use auth_core::crypto::{decrypt_token, encrypt_token};
use chrono::Utc;
use entity::oauth_connections;
use reqwest::Client;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QuerySelect, TransactionTrait,
};
use uuid::Uuid;

use super::registry::{GithubAppProvider, get_credentials};
use super::settings::{GITHUB_APP_PROVIDER, OAuthSettings};
use auth_core::provider::OAuthProvider;

/// 期限がこれより近ければ、使う前に更新する（API を 1 往復するあいだに切れない余裕）。
const REFRESH_MARGIN_SECS: i64 = 60;

/// 本人名義の GitHub 操作に使うトークンと、その GitHub アカウント。
pub struct GithubUserToken {
    pub access_token: String,
    /// GitHub のユーザー ID（数値）。改名しても変わらないので、自分のレビューの判定に使う
    pub github_user_id: i64,
}

/// `user_id` の GitHub App 連携からトークンを取り出す。連携が無ければ `Ok(None)`。
///
/// 期限が近ければリフレッシュトークンで更新し、回転した新しいトークンを保存し直す。
/// 更新は連携の行をロックして直列にする——同じ利用者の更新が 2 本並ぶと、後の 1 本は
/// 先に回転して無効になったリフレッシュトークンを使って失敗する。ロックを待った側は
/// 更新済みの期限を読むので、更新し直さない。
pub async fn github_app_user_token(
    db: &DatabaseConnection,
    http: &Client,
    settings: &OAuthSettings,
    user_id: Uuid,
) -> Result<Option<GithubUserToken>, anyhow::Error> {
    let txn = db.begin().await?;
    let Some(connection) = oauth_connections::Entity::find()
        .filter(oauth_connections::Column::UserId.eq(user_id))
        .filter(oauth_connections::Column::Provider.eq(GITHUB_APP_PROVIDER))
        .lock_exclusive()
        .one(&txn)
        .await?
    else {
        return Ok(None);
    };

    let github_user_id: i64 = connection.provider_user_id.parse()?;
    let key = settings.encryption_key.as_str();
    let expiring = connection.token_expires_at.is_some_and(|at| {
        at.with_timezone(&Utc) - Utc::now() < chrono::Duration::seconds(REFRESH_MARGIN_SECS)
    });

    if !expiring {
        let access_token_enc = connection
            .access_token_enc
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("github_app connection has no access token"))?;
        txn.commit().await?;
        return Ok(Some(GithubUserToken {
            access_token: decrypt_token(key, access_token_enc)?,
            github_user_id,
        }));
    }

    // ponytail: HTTP の往復のあいだ行ロックを持つ。更新は 8 時間に 1 回なので待ちは問題にならない
    let refresh_token = decrypt_token(
        key,
        connection
            .refresh_token_enc
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("github_app token expired and has no refresh token"))?,
    )?;
    let endpoints = GithubAppProvider.endpoints(http).await?;
    let credentials = get_credentials(GITHUB_APP_PROVIDER, settings)?;
    let token = refresh_access_token(http, &endpoints, &credentials, &refresh_token).await?;

    let mut active: oauth_connections::ActiveModel = connection.into();
    active.access_token_enc = Set(Some(encrypt_token(key, &token.access_token)?));
    if let Some(rotated) = &token.refresh_token {
        active.refresh_token_enc = Set(Some(encrypt_token(key, rotated)?));
    }
    active.token_expires_at = Set(token.expires_at.map(Into::into));
    active.updated_at = Set(Utc::now().into());
    active.update(&txn).await?;
    txn.commit().await?;

    Ok(Some(GithubUserToken {
        access_token: token.access_token,
        github_user_id,
    }))
}
