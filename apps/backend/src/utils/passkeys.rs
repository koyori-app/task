//! パスキー DB ↔ webauthn-rs `Passkey` 変換

use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
};
use webauthn_rs::prelude::{Credential, Passkey};

use crate::entities::passkeys::{self, Entity as PasskeyEntity};

pub const MAX_PASSKEYS_PER_USER: u64 = 20;

pub fn passkey_to_model_fields(
    passkey: &Passkey,
) -> Result<(Vec<u8>, Vec<u8>, Option<Vec<u8>>, i64), anyhow::Error> {
    let credential_id = passkey.cred_id().to_vec();
    let public_key = serde_json::to_vec(passkey)?;
    let credential: Credential = passkey.clone().into();
    let sign_count = credential.counter as i64;
    Ok((credential_id, public_key, None, sign_count))
}

pub fn model_to_passkey(model: &passkeys::Model) -> Result<Passkey, anyhow::Error> {
    serde_json::from_slice(&model.public_key).map_err(|e| anyhow::anyhow!("passkey deserialize: {e}"))
}

pub async fn load_user_passkeys(
    db: &DatabaseConnection,
    user_id: uuid::Uuid,
) -> Result<Vec<Passkey>, anyhow::Error> {
    let rows = PasskeyEntity::find()
        .filter(passkeys::Column::UserId.eq(user_id))
        .all(db)
        .await?;

    rows.iter().map(model_to_passkey).collect()
}

pub async fn count_user_passkeys(
    db: &DatabaseConnection,
    user_id: uuid::Uuid,
) -> Result<u64, sea_orm::DbErr> {
    PasskeyEntity::find()
        .filter(passkeys::Column::UserId.eq(user_id))
        .count(db)
        .await
}

pub async fn find_by_credential_id(
    db: &DatabaseConnection,
    credential_id: &[u8],
) -> Result<Option<passkeys::Model>, sea_orm::DbErr> {
    PasskeyEntity::find()
        .filter(passkeys::Column::CredentialId.eq(credential_id))
        .one(db)
        .await
}

/// 最後の認証手段削除ガード（仕様書 §7）
pub async fn is_last_auth_method(
    db: &DatabaseConnection,
    user_id: uuid::Uuid,
    passkey_count: u64,
) -> Result<bool, anyhow::Error> {
    if passkey_count != 1 {
        return Ok(false);
    }

    let user = crate::entities::users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("user not found"))?;

    if !user.password_hash.is_empty() {
        return Ok(false);
    }

    Ok(oauth_connection_count(db, user_id).await? == 0)
}

async fn oauth_connection_count(
    _db: &DatabaseConnection,
    _user_id: uuid::Uuid,
) -> Result<u64, anyhow::Error> {
    // oauth_connections テーブルは別ブランチで実装予定。未実装時は 0 件扱い。
    Ok(0)
}
