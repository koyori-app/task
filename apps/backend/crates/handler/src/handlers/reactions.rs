//! コメントとタスク本体の絵文字リアクション。
//!
//! 付ける・外すはコメント投稿と同じ権限（`write:task` + テナント/プロジェクトの検査）。

use axum::{
    Json,
    extract::{Path, State},
};
use sea_orm::prelude::Uuid;

use crate::AppState;
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::handlers::tasks::resolve_task;
use crate::openapi::CrudErrors;
use entity::scopes::Scope;
use payload::reactions::{ReactionSummary, ReactionUsersResponse, validate_emoji};
use service::reactions::{self, ReactionTarget};

/// 権限を通してからタスクを引く。形の悪い絵文字の 400 より認可の拒否を先に返す。
async fn authorized_task_id(
    state: &AppState,
    auth: &AuthUser,
    scope: Scope,
    (tenant_id, project_id, id): (Uuid, Uuid, &str),
    emoji: &str,
) -> Result<Uuid, AppError> {
    auth.require_scope(scope)?;
    auth.ensure_tenant_access(state, tenant_id, Some(project_id))
        .await?;
    let task = resolve_task(state, tenant_id, project_id, id).await?;
    validate_emoji(emoji).map_err(|_| AppError::BadRequest)?;
    Ok(task.id)
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}/reactions/{emoji}",
    tag = "Tasks",
    summary = "タスクにリアクションを付ける（冪等）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("emoji" = String, Path, description = "絵文字（パーセントエンコード）"),
    ),
    responses(
        (status = 200, description = "その絵文字の集計", body = ReactionSummary),
        CrudErrors,
    )
)]
pub async fn add_task_reaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, emoji)): Path<(Uuid, Uuid, String, String)>,
) -> Result<Json<ReactionSummary>, AppError> {
    let task_id = authorized_task_id(
        &state,
        &auth,
        Scope::WriteTask,
        (tenant_id, project_id, &id),
        &emoji,
    )
    .await?;
    let target = ReactionTarget::Task(task_id);
    reactions::add(&state.db, task_id, target, auth.user_id, &emoji).await?;
    Ok(Json(
        reactions::summary(&state.db, target, &emoji, auth.user_id).await?,
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}/reactions/{emoji}",
    tag = "Tasks",
    summary = "タスクから自分のリアクションを外す",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("emoji" = String, Path, description = "絵文字（パーセントエンコード）"),
    ),
    responses(
        (status = 200, description = "その絵文字の集計", body = ReactionSummary),
        CrudErrors,
    )
)]
pub async fn remove_task_reaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, emoji)): Path<(Uuid, Uuid, String, String)>,
) -> Result<Json<ReactionSummary>, AppError> {
    let task_id = authorized_task_id(
        &state,
        &auth,
        Scope::WriteTask,
        (tenant_id, project_id, &id),
        &emoji,
    )
    .await?;
    let target = ReactionTarget::Task(task_id);
    reactions::remove(&state.db, target, auth.user_id, &emoji).await?;
    Ok(Json(
        reactions::summary(&state.db, target, &emoji, auth.user_id).await?,
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}/reactions/{emoji}/users",
    tag = "Tasks",
    summary = "タスクのリアクションを押した全員",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("emoji" = String, Path, description = "絵文字（パーセントエンコード）"),
    ),
    responses(
        (status = 200, description = "押した全員（押した順）", body = ReactionUsersResponse),
        CrudErrors,
    )
)]
pub async fn list_task_reaction_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, emoji)): Path<(Uuid, Uuid, String, String)>,
) -> Result<Json<ReactionUsersResponse>, AppError> {
    let task_id = authorized_task_id(
        &state,
        &auth,
        Scope::ReadTask,
        (tenant_id, project_id, &id),
        &emoji,
    )
    .await?;
    Ok(Json(
        reactions::users_of(&state.db, ReactionTarget::Task(task_id), &emoji).await?,
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    put,
    path = "/{id}/comments/{cid}/reactions/{emoji}",
    tag = "Tasks",
    summary = "コメントにリアクションを付ける（冪等。削除済みには付けられない）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("cid" = Uuid, Path, description = "コメントID"),
        ("emoji" = String, Path, description = "絵文字（パーセントエンコード）"),
    ),
    responses(
        (status = 200, description = "その絵文字の集計", body = ReactionSummary),
        CrudErrors,
    )
)]
pub async fn add_comment_reaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, cid, emoji)): Path<(Uuid, Uuid, String, Uuid, String)>,
) -> Result<Json<ReactionSummary>, AppError> {
    let task_id = authorized_task_id(
        &state,
        &auth,
        Scope::WriteTask,
        (tenant_id, project_id, &id),
        &emoji,
    )
    .await?;
    let target = ReactionTarget::Comment(cid);
    reactions::add(&state.db, task_id, target, auth.user_id, &emoji).await?;
    Ok(Json(
        reactions::summary(&state.db, target, &emoji, auth.user_id).await?,
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}/comments/{cid}/reactions/{emoji}",
    tag = "Tasks",
    summary = "コメントから自分のリアクションを外す",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("cid" = Uuid, Path, description = "コメントID"),
        ("emoji" = String, Path, description = "絵文字（パーセントエンコード）"),
    ),
    responses(
        (status = 200, description = "その絵文字の集計（削除済みコメントは常に 0 件）", body = ReactionSummary),
        CrudErrors,
    )
)]
pub async fn remove_comment_reaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, cid, emoji)): Path<(Uuid, Uuid, String, Uuid, String)>,
) -> Result<Json<ReactionSummary>, AppError> {
    let task_id = authorized_task_id(
        &state,
        &auth,
        Scope::WriteTask,
        (tenant_id, project_id, &id),
        &emoji,
    )
    .await?;
    let comment = reactions::find_comment(&state.db, task_id, cid).await?;
    let target = ReactionTarget::Comment(cid);
    reactions::remove(&state.db, target, auth.user_id, &emoji).await?;
    // 削除済みコメントのリアクションは一覧と同じく見せない
    if comment.deleted_at.is_some() {
        return Ok(Json(ReactionSummary::empty(emoji)));
    }
    Ok(Json(
        reactions::summary(&state.db, target, &emoji, auth.user_id).await?,
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}/comments/{cid}/reactions/{emoji}/users",
    tag = "Tasks",
    summary = "コメントのリアクションを押した全員",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID"),
        ("cid" = Uuid, Path, description = "コメントID"),
        ("emoji" = String, Path, description = "絵文字（パーセントエンコード）"),
    ),
    responses(
        (status = 200, description = "押した全員（押した順。削除済みコメントは常に 0 件）", body = ReactionUsersResponse),
        CrudErrors,
    )
)]
pub async fn list_comment_reaction_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, cid, emoji)): Path<(Uuid, Uuid, String, Uuid, String)>,
) -> Result<Json<ReactionUsersResponse>, AppError> {
    let task_id = authorized_task_id(
        &state,
        &auth,
        Scope::ReadTask,
        (tenant_id, project_id, &id),
        &emoji,
    )
    .await?;
    let comment = reactions::find_comment(&state.db, task_id, cid).await?;
    // 削除済みコメントのリアクションは一覧と同じく見せない
    if comment.deleted_at.is_some() {
        return Ok(Json(ReactionUsersResponse {
            emoji,
            count: 0,
            users: Vec::new(),
        }));
    }
    Ok(Json(
        reactions::users_of(&state.db, ReactionTarget::Comment(cid), &emoji).await?,
    ))
}
