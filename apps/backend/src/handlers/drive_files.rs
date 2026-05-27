use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::Response,
};
use axum_valid::Valid;
use bytes::Bytes;
use chrono::Utc;
use futures::{SinkExt, channel::mpsc as fmpsc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entities::{
    drive_files, drive_folder_shares, drive_folders, project_members, scopes::Scope,
    tenants,
};
use crate::error::AppError;
use crate::extractors::{AuthUser, OptionalAuthUser};
use crate::openapi::CrudErrors;
use crate::utils::drive::{
    content_url, current_storage_type, effective_quota, guess_mime, is_tenant_owner,
    tenant_used_bytes,
};
use crate::utils::storage::{ByteStream, StorageError};
use crate::AppState;

const MAX_LIST_LIMIT: u32 = 200;
const DEFAULT_LIST_LIMIT: u32 = 50;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListFilesQuery {
    pub folder_id: Option<Uuid>,
    #[param(minimum = 1, maximum = 200)]
    #[serde(default = "default_list_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_list_limit() -> u32 {
    DEFAULT_LIST_LIMIT
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DriveFileResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub size: i64,
    pub mime_type: String,
    pub url: String,
    #[schema(value_type = String, format = "uuid", nullable)]
    pub folder_id: Option<Uuid>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListFilesResponse {
    pub files: Vec<DriveFileResponse>,
    pub total: u64,
}

#[derive(Validate, Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateFileRequest {
    #[validate(length(min = 1))]
    pub name: Option<String>,
    pub folder_id: Option<Option<Uuid>>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DriveUsageResponse {
    pub used_bytes: i64,
    #[schema(nullable)]
    pub quota_bytes: Option<i64>,
    #[schema(nullable)]
    pub system_max_bytes: Option<i64>,
    pub unlimited: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateQuotaRequest {
    pub quota_bytes: Option<i64>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ContentQuery {
    pub token: Option<String>,
}

fn drive_file_response(model: &drive_files::Model) -> DriveFileResponse {
    DriveFileResponse {
        id: model.id,
        name: model.name.clone(),
        size: model.size,
        mime_type: model.mime_type.clone(),
        url: content_url(model.id),
        folder_id: model.folder_id,
        created_at: model.created_at.into(),
    }
}

async fn load_tenant_file(
    state: &AppState,
    tenant_id: Uuid,
    file_id: Uuid,
) -> Result<drive_files::Model, AppError> {
    drive_files::Entity::find_by_id(file_id)
        .filter(drive_files::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

async fn load_folder_in_tenant(
    state: &AppState,
    tenant_id: Uuid,
    folder_id: Uuid,
) -> Result<drive_folders::Model, AppError> {
    drive_folders::Entity::find_by_id(folder_id)
        .filter(drive_folders::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

async fn is_project_member(
    state: &AppState,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let member = project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .filter(project_members::Column::UserId.eq(user_id))
        .one(&state.db)
        .await?;
    Ok(member.is_some())
}

async fn folder_has_user_share(
    state: &AppState,
    folder_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let mut current = Some(folder_id);
    while let Some(fid) = current {
        let share = drive_folder_shares::Entity::find()
            .filter(drive_folder_shares::Column::FolderId.eq(fid))
            .filter(drive_folder_shares::Column::SharedWithUserId.eq(user_id))
            .one(&state.db)
            .await?;
        if let Some(share) = share {
            if share
                .expires_at
                .map(|e| e > Utc::now().fixed_offset())
                .unwrap_or(true)
            {
                return Ok(true);
            }
        }
        let folder = drive_folders::Entity::find_by_id(fid)
            .one(&state.db)
            .await?;
        current = folder.and_then(|f| f.parent_id);
    }
    Ok(false)
}

async fn folder_has_token_share(
    state: &AppState,
    folder_id: Uuid,
    token: &str,
) -> Result<bool, AppError> {
    let mut current = Some(folder_id);
    while let Some(fid) = current {
        let share = drive_folder_shares::Entity::find()
            .filter(drive_folder_shares::Column::FolderId.eq(fid))
            .filter(drive_folder_shares::Column::ShareToken.eq(token))
            .one(&state.db)
            .await?;
        if let Some(share) = share {
            if share
                .expires_at
                .map(|e| e > Utc::now().fixed_offset())
                .unwrap_or(true)
            {
                return Ok(true);
            }
            return Ok(false);
        }
        let folder = drive_folders::Entity::find_by_id(fid)
            .one(&state.db)
            .await?;
        current = folder.and_then(|f| f.parent_id);
    }
    Ok(false)
}

async fn can_access_file_content(
    state: &AppState,
    file: &drive_files::Model,
    auth: &OptionalAuthUser,
    share_token: Option<&str>,
) -> Result<(), AppError> {
    if file.project_id.is_none() {
        return Ok(());
    }

    let project_id = file.project_id.expect("checked is_none above");

    if let Some(token) = share_token.filter(|t| !t.is_empty()) {
        if let Some(folder_id) = file.folder_id {
            if folder_has_token_share(state, folder_id, token).await? {
                return Ok(());
            }
        }
        return Err(AppError::Forbidden);
    }

    let Some(auth_user) = &auth.0 else {
        return Err(AppError::Forbidden);
    };

    if is_tenant_owner(state, file.tenant_id, auth_user.user_id).await? {
        return Ok(());
    }

    if is_project_member(state, project_id, auth_user.user_id).await? {
        return Ok(());
    }

    if let Some(folder_id) = file.folder_id {
        if folder_has_user_share(state, folder_id, auth_user.user_id).await? {
            return Ok(());
        }
    }

    Err(AppError::Forbidden)
}

fn list_limit(limit: u32) -> u32 {
    limit.clamp(1, MAX_LIST_LIMIT)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/files",
    summary = "ドライブファイル一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ListFilesQuery,
    ),
    responses(
        (status = 200, description = "ファイル一覧", body = ListFilesResponse),
        CrudErrors,
    )
)]
pub async fn list_files(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Query(query): Query<ListFilesQuery>,
) -> Result<Json<ListFilesResponse>, AppError> {
    auth.require_scope(Scope::ReadDrive)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;

    let limit = list_limit(query.limit);
    let mut selector = drive_files::Entity::find()
        .filter(drive_files::Column::TenantId.eq(tenant_id))
        .order_by_desc(drive_files::Column::CreatedAt);

    selector = match query.folder_id {
        Some(folder_id) => selector.filter(drive_files::Column::FolderId.eq(folder_id)),
        None => selector.filter(drive_files::Column::FolderId.is_null()),
    };

    let total = selector.clone().count(&state.db).await?;
    let files = selector
        .offset(query.offset as u64)
        .limit(limit as u64)
        .all(&state.db)
        .await?;

    Ok(Json(ListFilesResponse {
        files: files.iter().map(drive_file_response).collect(),
        total,
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/files",
    summary = "ドライブファイルアップロード",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    request_body(content_type = "multipart/form-data"),
    responses(
        (status = 201, description = "作成されたファイル", body = DriveFileResponse),
        CrudErrors,
    )
)]
pub async fn upload_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<DriveFileResponse>), AppError> {
    auth.require_scope(Scope::WriteDrive)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;

    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // 'name' と 'folder_id' は 'file' フィールドより前に送ること（ストリーミング設計の前提）。
    let mut display_name: Option<String> = None;
    let mut folder_id: Option<Uuid> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        match field.name() {
            Some("file") => {
                let original_filename = field.file_name().map(str::to_string);
                let content_type = field.content_type().map(str::to_string);

                let name = display_name
                    .or_else(|| original_filename.clone())
                    .ok_or(AppError::BadRequest)?;
                let mime_type = content_type.unwrap_or_else(|| {
                    guess_mime(original_filename.as_deref().unwrap_or(&name))
                });

                // クォータ事前チェック（既に上限に達していれば即拒否）
                let used = tenant_used_bytes(&state.db, tenant_id).await?;
                if let Some(q) = effective_quota(&tenant, &state.drive_config) {
                    if used >= q {
                        return Err(AppError::ContentTooLarge);
                    }
                }

                let folder_project_id = if let Some(fid) = folder_id {
                    let folder = load_folder_in_tenant(&state, tenant_id, fid).await?;
                    folder.project_id
                } else {
                    None
                };

                // Field<'a> は 'static でないため stream::unfold 不可。
                // mpsc channel の Receiver は 'static なので ByteStream として使える。
                // tokio::join! で pump と upload を同タスク内で並行実行する。
                let byte_count = Arc::new(AtomicU64::new(0));
                let counter = byte_count.clone();
                let max_bytes = state.drive_config.upload_max_bytes;

                let (mut tx, rx) = fmpsc::channel::<Result<Bytes, StorageError>>(8);
                let byte_stream: ByteStream = Box::pin(rx);

                let pump = async move {
                    let mut field = field;
                    loop {
                        match field.chunk().await {
                            Ok(Some(bytes)) => {
                                counter.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                                if tx.send(Ok(bytes)).await.is_err() {
                                    return;
                                }
                            }
                            Ok(None) => break,
                            Err(e) => {
                                let _ = tx.send(Err(StorageError::Other(e.to_string()))).await;
                                return;
                            }
                        }
                    }
                };

                let file_id = Uuid::new_v4();
                let storage_key = file_id.to_string();

                let (upload_result, _) = tokio::join!(
                    state.storage.upload(&storage_key, byte_stream, 0, &mime_type),
                    pump
                );
                upload_result.map_err(storage_to_app_error)?;

                let actual_size = byte_count.load(Ordering::Relaxed);

                // アップロード後にサイズ・クォータを確定チェック
                if actual_size == 0 {
                    let _ = state.storage.delete(&storage_key).await;
                    return Err(AppError::BadRequest);
                }
                if actual_size > max_bytes {
                    let _ = state.storage.delete(&storage_key).await;
                    return Err(AppError::ContentTooLarge);
                }
                // テナント行を FOR UPDATE でロックしてクォータ確定チェックと INSERT をアトミックに実行。
                // 並行アップロードが同時にクォータチェックを通過してしまう競合を防ぐ。
                let result: Result<drive_files::Model, AppError> = async {
                    let txn = state.db.begin().await?;
                    let tenant_q = tenants::Entity::find_by_id(tenant_id)
                        .lock_exclusive()
                        .one(&txn)
                        .await?
                        .ok_or(AppError::NotFound)?;
                    let used_now = tenant_used_bytes(&txn, tenant_id).await?;
                    if let Some(q) = effective_quota(&tenant_q, &state.drive_config) {
                        if used_now.saturating_add(actual_size as i64) > q {
                            return Err(AppError::ContentTooLarge);
                        }
                    }
                    let model = drive_files::ActiveModel {
                        id: Set(file_id),
                        name: Set(name),
                        size: Set(actual_size as i64),
                        mime_type: Set(mime_type),
                        storage_type: Set(current_storage_type()),
                        storage_key: Set(storage_key.clone()),
                        tenant_id: Set(tenant_id),
                        project_id: Set(folder_project_id),
                        uploader_id: Set(auth.user_id),
                        folder_id: Set(folder_id),
                        created_at: Set(Utc::now().fixed_offset()),
                    };
                    let saved = model.insert(&txn).await?;
                    txn.commit().await?;
                    Ok(saved)
                }
                .await;
                match result {
                    Ok(saved) => return Ok((StatusCode::CREATED, Json(drive_file_response(&saved)))),
                    Err(e) => {
                        let _ = state.storage.delete(&storage_key).await;
                        return Err(e);
                    }
                }
            }
            Some("name") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
                if !text.is_empty() {
                    display_name = Some(text);
                }
            }
            Some("folder_id") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
                if !text.is_empty() {
                    folder_id = Some(
                        Uuid::parse_str(text.trim()).map_err(|_| AppError::BadRequest)?,
                    );
                }
            }
            _ => {}
        }
    }

    Err(AppError::BadRequest) // 'file' フィールドなし
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/files/{id}",
    summary = "ドライブファイルメタデータ取得",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("id" = Uuid, Path, description = "ファイルID"),
    ),
    responses(
        (status = 200, description = "ファイルメタデータ", body = DriveFileResponse),
        CrudErrors,
    )
)]
pub async fn get_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DriveFileResponse>, AppError> {
    auth.require_scope(Scope::ReadDrive)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;
    let file = load_tenant_file(&state, tenant_id, id).await?;
    Ok(Json(drive_file_response(&file)))
}

#[axum::debug_handler]
#[utoipa::path(
    patch,
    path = "/files/{id}",
    summary = "ドライブファイル更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("id" = Uuid, Path, description = "ファイルID"),
    ),
    request_body = UpdateFileRequest,
    responses(
        (status = 200, description = "更新されたファイル", body = DriveFileResponse),
        CrudErrors,
    )
)]
pub async fn update_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateFileRequest>>,
) -> Result<Json<DriveFileResponse>, AppError> {
    auth.require_scope(Scope::WriteDrive)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;

    let file = load_tenant_file(&state, tenant_id, id).await?;
    let mut active: drive_files::ActiveModel = file.into();

    if let Some(name) = payload.name {
        active.name = Set(name);
    }

    if let Some(folder_id) = payload.folder_id {
        let project_id = if let Some(fid) = folder_id {
            let folder = load_folder_in_tenant(&state, tenant_id, fid).await?;
            folder.project_id
        } else {
            None
        };
        active.folder_id = Set(folder_id);
        active.project_id = Set(project_id);
    }

    let updated = active.update(&state.db).await?;
    Ok(Json(drive_file_response(&updated)))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/files/{id}",
    summary = "ドライブファイル削除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("id" = Uuid, Path, description = "ファイルID"),
    ),
    responses(
        (status = 204, description = "削除完了"),
        CrudErrors,
    )
)]
pub async fn delete_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(Scope::WriteDrive)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;

    let file = load_tenant_file(&state, tenant_id, id).await?;
    let storage_key = file.storage_key.clone();
    drive_files::Entity::delete_by_id(id)
        .exec(&state.db)
        .await?;
    let _ = state.storage.delete(&storage_key).await;
    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/files/{id}/content",
    summary = "ドライブファイル内容配信",
    params(
        ("id" = Uuid, Path, description = "ファイルID"),
        ContentQuery,
    ),
    responses(
        (status = 200, description = "ファイルバイナリ"),
        CrudErrors,
    )
)]
pub async fn get_file_content(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<ContentQuery>,
) -> Result<Response, AppError> {
    let file = drive_files::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    can_access_file_content(
        &state,
        &file,
        &auth,
        query.token.as_deref(),
    )
    .await?;

    let stream = state
        .storage
        .get_stream(&file.storage_key)
        .await
        .map_err(storage_to_app_error)?;

    let body = Body::from_stream(stream);
    let disposition = format!("inline; filename=\"{}\"", sanitize_filename(&file.name));

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, file.mime_type.as_str())
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(body)
        .map_err(|e| AppError::Internal(e.into()))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/usage",
    summary = "ドライブ使用量・クォータ取得",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 200, description = "使用量", body = DriveUsageResponse),
        CrudErrors,
    )
)]
pub async fn get_drive_usage(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<DriveUsageResponse>, AppError> {
    auth.require_scope(Scope::ReadDrive)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;

    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let used_bytes = tenant_used_bytes(&state.db, tenant_id).await?;
    let quota = effective_quota(&tenant, &state.drive_config);

    Ok(Json(DriveUsageResponse {
        used_bytes,
        quota_bytes: quota,
        system_max_bytes: state.drive_config.system_max_bytes_opt(),
        unlimited: quota.is_none(),
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    patch,
    path = "/quota",
    summary = "ドライブクォータ設定",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    request_body = UpdateQuotaRequest,
    responses(
        (status = 200, description = "更新後の使用量情報", body = DriveUsageResponse),
        CrudErrors,
    )
)]
pub async fn update_drive_quota(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Json(payload): Json<UpdateQuotaRequest>,
) -> Result<Json<DriveUsageResponse>, AppError> {
    auth.require_scope(Scope::AdminTenant)?;
    let tenant = auth.ensure_tenant_owner(&state, tenant_id).await?;

    if let Some(quota_bytes) = payload.quota_bytes {
        if quota_bytes < 0 {
            return Err(AppError::BadRequest);
        }
        if let Some(system_max) = state.drive_config.system_max_bytes_opt() {
            if quota_bytes > system_max {
                return Err(AppError::BadRequest);
            }
        }
    }

    let mut active: tenants::ActiveModel = tenant.into();
    active.drive_quota_bytes = Set(payload.quota_bytes);
    let updated = active.update(&state.db).await?;

    let used_bytes = tenant_used_bytes(&state.db, tenant_id).await?;
    let quota = effective_quota(&updated, &state.drive_config);

    Ok(Json(DriveUsageResponse {
        used_bytes,
        quota_bytes: quota,
        system_max_bytes: state.drive_config.system_max_bytes_opt(),
        unlimited: quota.is_none(),
    }))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c == '"' || c == '\r' || c == '\n' { '_' } else { c })
        .collect()
}

fn storage_to_app_error(err: StorageError) -> AppError {
    match err {
        StorageError::InvalidKey => AppError::BadRequest,
        other => AppError::Internal(other.into()),
    }
}
