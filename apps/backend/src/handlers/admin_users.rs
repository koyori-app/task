//! 管理者専用 — ユーザー管理 API（`/v1/admin/users`）

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use axum_valid::Valid;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, ExprTrait, PaginatorTrait, QueryFilter, Statement, TransactionTrait,
};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entities::{
    drive_files, drive_folder_shares, drive_folders, personal_tokens, project_members, tenants,
    users,
};
use crate::error::AppError;
use crate::extractors::AdminUser;
use crate::handlers::admin_audit::record_audit;
use crate::openapi::CrudErrors;
use crate::utils::auth::{create_password_hash, generate_email_verification_token};
use crate::utils::db::is_postgres_unique_violation;
use crate::utils::email::normalize_email;
use crate::utils::password_reset;
use crate::utils::password_reset_delivery;
use crate::utils::auth::AuthError;
use crate::AppState;

fn auth_error_to_app(e: AuthError) -> AppError {
    match e {
        AuthError::Internal(err) => AppError::Internal(err),
        _ => AppError::Internal(anyhow::anyhow!("{e}")),
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema, Validate)]
pub struct AdminCreateUserRequest {
    #[validate(length(min = 3))]
    pub username: String,
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub email_verified: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema, Validate)]
pub struct AdminUpdateUserRequest {
    pub is_admin: Option<bool>,
    pub is_suspended: Option<bool>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema, Validate)]
pub struct AdminPasswordResetRequest {
    #[validate(email)]
    pub send_to: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdminPasswordResetResponse {
    pub message: String,
}

async fn table_exists(db: &DatabaseConnection, table: &str) -> Result<bool, AppError> {
    let sql = format!(
        "SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = '{table}'
        )"
    );
    let row = db
        .query_one_raw(Statement::from_string(db.get_database_backend(), sql))
        .await?;
    Ok(row
        .and_then(|r| r.try_get_by_index::<bool>(0).ok())
        .unwrap_or(false))
}

async fn column_exists(db: &DatabaseConnection, table: &str, column: &str) -> Result<bool, AppError> {
    let sql = format!(
        "SELECT EXISTS (
            SELECT FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = '{table}' AND column_name = '{column}'
        )"
    );
    let row = db
        .query_one_raw(Statement::from_string(db.get_database_backend(), sql))
        .await?;
    Ok(row
        .and_then(|r| r.try_get_by_index::<bool>(0).ok())
        .unwrap_or(false))
}

async fn revoke_user_sessions(db: &DatabaseConnection, user_id: Uuid) -> Result<(), AppError> {
    if column_exists(db, "users", "sessions_revoked_at").await? {
        let sql = format!(
            "UPDATE users SET sessions_revoked_at = NOW() WHERE id = '{user_id}'"
        );
        db.execute_unprepared(&sql).await?;
    }
    Ok(())
}

async fn reset_2fa_for_user(db: &DatabaseConnection, user_id: Uuid) -> Result<(), AppError> {
    if table_exists(db, "totp_credentials").await? {
        let sql = format!("DELETE FROM totp_credentials WHERE user_id = '{user_id}'");
        db.execute_unprepared(&sql).await?;
    }
    if table_exists(db, "recovery_codes").await? {
        let sql = format!("DELETE FROM recovery_codes WHERE user_id = '{user_id}'");
        db.execute_unprepared(&sql).await?;
    }
    if column_exists(db, "users", "totp_enabled").await? {
        let sql = format!("UPDATE users SET totp_enabled = false WHERE id = '{user_id}'");
        db.execute_unprepared(&sql).await?;
    }
    revoke_user_sessions(db, user_id).await?;
    Ok(())
}

async fn ensure_not_last_admin(db: &DatabaseConnection, target_id: Uuid) -> Result<(), AppError> {
    let target = users::Entity::find_by_id(target_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    if !target.is_admin {
        return Ok(());
    }
    let admin_count = users::Entity::find()
        .filter(users::Column::IsAdmin.eq(true))
        .count(db)
        .await?;
    if admin_count <= 1 {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

async fn delete_user_cascade(db: &DatabaseConnection, user_id: Uuid) -> Result<(), AppError> {
    let txn = db.begin().await?;

    if table_exists(db, "task_assignees").await? {
        let sql = format!("DELETE FROM task_assignees WHERE user_id = '{user_id}'");
        txn.execute_unprepared(&sql).await?;
    }
    if table_exists(db, "tasks").await? {
        let sql = format!("DELETE FROM tasks WHERE created_by = '{user_id}'");
        txn.execute_unprepared(&sql).await?;
    }
    if table_exists(db, "milestones").await? {
        let sql = format!("DELETE FROM milestones WHERE created_by = '{user_id}'");
        txn.execute_unprepared(&sql).await?;
    }

    drive_files::Entity::delete_many()
        .filter(drive_files::Column::UploaderId.eq(user_id))
        .exec(&txn)
        .await?;

    drive_folders::Entity::delete_many()
        .filter(drive_folders::Column::CreatedBy.eq(user_id))
        .exec(&txn)
        .await?;

    drive_folder_shares::Entity::delete_many()
        .filter(
            drive_folder_shares::Column::SharedWithUserId
                .eq(user_id)
                .or(drive_folder_shares::Column::CreatedBy.eq(user_id)),
        )
        .exec(&txn)
        .await?;

    project_members::Entity::delete_many()
        .filter(project_members::Column::UserId.eq(user_id))
        .exec(&txn)
        .await?;

    personal_tokens::Entity::delete_many()
        .filter(personal_tokens::Column::UserId.eq(user_id))
        .exec(&txn)
        .await?;

    tenants::Entity::delete_many()
        .filter(tenants::Column::OwnerId.eq(user_id))
        .exec(&txn)
        .await?;

    users::Entity::delete_by_id(user_id).exec(&txn).await?;

    txn.commit().await?;
    Ok(())
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "Admin Users",
    summary = "全ユーザー一覧",
    responses(
        (status = 200, description = "ユーザー一覧", body = [users::Model]),
        CrudErrors,
    )
)]
pub async fn list_users(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<users::Model>>, AppError> {
    let list = users::Entity::find().all(&state.db).await?;
    Ok(Json(list))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "Admin Users",
    summary = "ユーザー作成（管理者）",
    request_body = AdminCreateUserRequest,
    responses(
        (status = 201, description = "作成されたユーザー", body = users::Model),
        CrudErrors,
    )
)]
pub async fn create_user(
    State(state): State<AppState>,
    admin: AdminUser,
    headers: HeaderMap,
    Valid(Json(payload)): Valid<Json<AdminCreateUserRequest>>,
) -> Result<(StatusCode, Json<users::Model>), AppError> {
    let email = normalize_email(&payload.email);
    let password_hash = create_password_hash(&payload.password).map_err(auth_error_to_app)?;
    let user_id = Uuid::new_v4();

    let user = users::ActiveModel {
        id: Set(user_id),
        username: Set(payload.username),
        bio: Set(Some(String::new())),
        avatar_url: Set(None),
        email: Set(email),
        email_verified: Set(payload.email_verified),
        password_hash: Set(password_hash),
        is_admin: Set(payload.is_admin),
        is_suspended: Set(false),
    };

    let model = user.insert(&state.db).await.map_err(|e| {
        if is_postgres_unique_violation(&e) {
            AppError::Conflict
        } else {
            AppError::from(e)
        }
    })?;

    record_audit(
        &state.db,
        admin.user_id,
        "user.create",
        "user",
        &user_id.to_string(),
        None,
        Some(serde_json::json!({
            "email": model.email,
            "is_admin": model.is_admin,
        })),
        &headers,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(model)))
}

#[axum::debug_handler]
#[utoipa::path(
    patch,
    path = "/{id}",
    tag = "Admin Users",
    summary = "ユーザー更新（is_admin / is_suspended）",
    request_body = AdminUpdateUserRequest,
    responses(
        (status = 200, description = "更新後のユーザー", body = users::Model),
        CrudErrors,
    )
)]
pub async fn update_user(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Valid(Json(payload)): Valid<Json<AdminUpdateUserRequest>>,
) -> Result<Json<users::Model>, AppError> {
    if payload.is_admin == Some(false) && admin.user_id == id {
        return Err(AppError::Forbidden);
    }

    let user = users::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if payload.is_admin == Some(false) && user.is_admin {
        ensure_not_last_admin(&state.db, id).await?;
    }

    let before_admin = user.is_admin;
    let before_suspended = user.is_suspended;

    let mut active: users::ActiveModel = user.into();
    if let Some(v) = payload.is_admin {
        active.is_admin = Set(v);
    }
    if let Some(v) = payload.is_suspended {
        active.is_suspended = Set(v);
    }
    let model = active.update(&state.db).await?;

    if let Some(is_admin) = payload.is_admin {
        if is_admin != before_admin {
            let action = if is_admin {
                "user.admin.grant"
            } else {
                "user.admin.revoke"
            };
            record_audit(
                &state.db,
                admin.user_id,
                action,
                "user",
                &id.to_string(),
                None,
                Some(serde_json::json!({ "before": before_admin, "after": is_admin })),
                &headers,
            )
            .await?;
        }
    }

    if let Some(is_suspended) = payload.is_suspended {
        if is_suspended != before_suspended {
            let action = if is_suspended {
                "user.suspend"
            } else {
                "user.unsuspend"
            };
            record_audit(
                &state.db,
                admin.user_id,
                action,
                "user",
                &id.to_string(),
                None,
                Some(serde_json::json!({ "before": before_suspended, "after": is_suspended })),
                &headers,
            )
            .await?;
        }
    }

    Ok(Json(model))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}",
    tag = "Admin Users",
    summary = "ユーザー強制削除",
    responses(
        (status = 204, description = "削除完了"),
        CrudErrors,
    )
)]
pub async fn delete_user(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    if admin.user_id == id {
        return Err(AppError::Forbidden);
    }

    let user = users::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if user.is_admin {
        ensure_not_last_admin(&state.db, id).await?;
    }

    delete_user_cascade(&state.db, id).await?;

    record_audit(
        &state.db,
        admin.user_id,
        "user.delete",
        "user",
        &id.to_string(),
        None,
        Some(serde_json::json!({ "email": user.email })),
        &headers,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/password-reset",
    tag = "Admin Users",
    summary = "パスワードリセットリンク生成・送信",
    request_body = AdminPasswordResetRequest,
    responses(
        (status = 200, description = "リセットメール送信", body = AdminPasswordResetResponse),
        CrudErrors,
    )
)]
pub async fn password_reset(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Valid(Json(payload)): Valid<Json<AdminPasswordResetRequest>>,
) -> Result<Json<AdminPasswordResetResponse>, AppError> {
    let user = users::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let send_to = payload
        .send_to
        .as_ref()
        .map(|e| normalize_email(e))
        .unwrap_or_else(|| user.email.clone());

    if !password_reset::try_acquire_rate_limit(&state.redis_client, &send_to)
        .await
        .map_err(|e| AppError::Internal(e))?
    {
        return Err(AppError::Conflict);
    }

    let token = generate_email_verification_token();
    password_reset::store_token(&state.redis_client, id, &token)
        .await
        .map_err(|e| AppError::Internal(e))?;

    password_reset_delivery::send_password_reset_email(
        &state.smtp_client,
        &send_to,
        &state.settings,
        &token,
    )
    .await
    .map_err(|e| AppError::Internal(e))?;

    record_audit(
        &state.db,
        admin.user_id,
        "user.password_reset",
        "user",
        &id.to_string(),
        None,
        Some(serde_json::json!({ "reset_email": send_to })),
        &headers,
    )
    .await?;

    Ok(Json(AdminPasswordResetResponse {
        message: "パスワードリセットメールを送信しました".into(),
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/reset-2fa",
    tag = "Admin Users",
    summary = "2FA 強制リセット",
    responses(
        (status = 204, description = "2FA リセット完了"),
        CrudErrors,
    )
)]
pub async fn reset_2fa(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    if users::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound);
    }

    reset_2fa_for_user(&state.db, id).await?;

    record_audit(
        &state.db,
        admin.user_id,
        "user.2fa.reset",
        "user",
        &id.to_string(),
        None,
        None,
        &headers,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}/passkeys/{passkey_id}",
    tag = "Admin Users",
    summary = "パスキー強制削除",
    responses(
        (status = 204, description = "削除完了"),
        CrudErrors,
    )
)]
pub async fn delete_passkey(
    State(state): State<AppState>,
    admin: AdminUser,
    Path((id, passkey_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    if users::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound);
    }

    if !table_exists(&state.db, "passkeys").await? {
        return Err(AppError::NotFound);
    }

    let sql = format!(
        "DELETE FROM passkeys WHERE id = '{passkey_id}' AND user_id = '{id}'"
    );
    let result = state
        .db
        .execute_unprepared(&sql)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    record_audit(
        &state.db,
        admin.user_id,
        "user.passkey.delete",
        "user",
        &id.to_string(),
        None,
        Some(serde_json::json!({ "passkey_id": passkey_id })),
        &headers,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}/oauth/{provider}",
    tag = "Admin Users",
    summary = "OAuth 連携強制解除",
    responses(
        (status = 204, description = "解除完了"),
        CrudErrors,
    )
)]
pub async fn delete_oauth(
    State(state): State<AppState>,
    admin: AdminUser,
    Path((id, provider)): Path<(Uuid, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    if users::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound);
    }

    if !table_exists(&state.db, "oauth_connections").await? {
        return Err(AppError::NotFound);
    }

    let provider_escaped = provider.replace('\'', "''");
    let sql = format!(
        "DELETE FROM oauth_connections WHERE user_id = '{id}' AND provider = '{provider_escaped}'"
    );
    let result = state
        .db
        .execute_unprepared(&sql)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    record_audit(
        &state.db,
        admin.user_id,
        "user.oauth.disconnect",
        "user",
        &id.to_string(),
        None,
        Some(serde_json::json!({ "provider": provider })),
        &headers,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
