//! マージ時副作用の確認済み操作（docs/features/tasks/9.github-tasks.md §8 / §9）。

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use sea_orm::prelude::Uuid;

use crate::AppState;
use crate::error::{AppError, ServerError};
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/pull-requests/{pull_request_id}/merge-effects/acknowledge",
    tag = "Tasks",
    summary = "マージ時の自動完了を未適用のまま確認済みにする",
    description = "タスクは完了させず、`merge_effects_processed_at` と `merge_effects_acknowledged_by` を設定して未適用表示を解消する。",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("pull_request_id" = Uuid, Path, description = "PR の ID"),
    ),
    responses(
        (status = 200, description = "確認済みにしました"),
        (status = 409, description = "マージ済みでない、または処理済み・確認済み", body = ServerError),
        CrudErrors,
    )
)]
pub async fn acknowledge_merge_effects(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, pull_request_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    service::forge::merge_effects::acknowledge_merge_effects(
        &state.db,
        project_id,
        pull_request_id,
        auth.user_id,
    )
    .await?;
    Ok(StatusCode::OK)
}
