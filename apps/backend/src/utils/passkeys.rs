//! パスキー DB ↔ webauthn-rs `Passkey` 変換

use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
};
use webauthn_rs::prelude::{
    AttestationMetadata, AuthenticationResult, Credential, Passkey,
};

use crate::entities::passkeys::{self, Entity as PasskeyEntity};
use crate::utils::auth::AuthError;

pub const MAX_PASSKEYS_PER_USER: u64 = 20;

/// `Passkey` の attestation メタデータから AAGUID（16 バイト）を抽出する。
/// 取得不可・ゼロ AAGUID の場合は `None`。
pub fn extract_aaguid(passkey: &Passkey) -> Option<Vec<u8>> {
    let credential: Credential = passkey.clone().into();
    let uuid = match credential.attestation.metadata {
        AttestationMetadata::Packed { aaguid } | AttestationMetadata::Tpm { aaguid, .. } => aaguid,
        AttestationMetadata::None => return None,
        _ => return None,
    };
    if uuid.is_nil() {
        return None;
    }
    Some(uuid.as_bytes().to_vec())
}

pub fn passkey_to_model_fields(
    passkey: &Passkey,
) -> Result<(Vec<u8>, Vec<u8>, Option<Vec<u8>>, i64), anyhow::Error> {
    let credential_id = passkey.cred_id().to_vec();
    let public_key = serde_json::to_vec(passkey)?;
    let credential: Credential = passkey.clone().into();
    let sign_count = credential.counter as i64;
    let aaguid = extract_aaguid(passkey);
    Ok((credential_id, public_key, aaguid, sign_count))
}

/// 認証結果の sign counter を検証する（リプレイ・クローン検知）。
pub fn verify_sign_counter(
    auth_result: &AuthenticationResult,
    stored_sign_count: i64,
) -> Result<(), AuthError> {
    verify_sign_counter_values(auth_result.counter(), stored_sign_count)
}

/// `verify_sign_counter` のコアロジック（テスト可能）。
pub(crate) fn verify_sign_counter_values(
    counter: u32,
    stored_sign_count: i64,
) -> Result<(), AuthError> {
    if counter > 0 {
        if counter <= stored_sign_count as u32 {
            return Err(AuthError::InvalidCredentials);
        }
        return Ok(());
    }
    // counter=0: 同期型パスキー等。以前に非ゼロ counter が記録されていればクローン疑い
    if stored_sign_count > 0 {
        return Err(AuthError::PossibleCredentialClone);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_counter_rejects_replay_when_counter_increments_normally() {
        assert!(verify_sign_counter_values(5, 4).is_ok());
    }

    #[test]
    fn sign_counter_rejects_equal_or_lower_counter() {
        assert!(matches!(
            verify_sign_counter_values(4, 4),
            Err(AuthError::InvalidCredentials)
        ));
        assert!(matches!(
            verify_sign_counter_values(3, 4),
            Err(AuthError::InvalidCredentials)
        ));
    }

    #[test]
    fn sign_counter_detects_possible_clone_when_counter_resets_to_zero() {
        assert!(matches!(
            verify_sign_counter_values(0, 10),
            Err(AuthError::PossibleCredentialClone)
        ));
    }

    #[test]
    fn sign_counter_allows_zero_counter_for_new_credentials() {
        assert!(verify_sign_counter_values(0, 0).is_ok());
    }

    #[test]
    fn max_passkeys_per_user_is_twenty() {
        assert_eq!(MAX_PASSKEYS_PER_USER, 20);
    }
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
