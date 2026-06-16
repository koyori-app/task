use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    prelude::Uuid,
};
use sea_orm::sea_query::OnConflict;
use sea_orm::entity::prelude::Json;
use std::collections::HashSet;

use crate::entities::{
    notification_settings, notifications, task_watchers, users,
};
use crate::error::AppError;

pub const TYPE_ASSIGNED: &str = "assigned";
pub const TYPE_MENTIONED: &str = "mentioned";
pub const TYPE_STATUS_CHANGED: &str = "status_changed";
pub const TYPE_COMMENT_ADDED: &str = "comment_added";

pub const DEFAULT_IN_APP_EVENTS: &[&str] = &[
    TYPE_ASSIGNED,
    TYPE_MENTIONED,
    "deadline_soon",
    TYPE_COMMENT_ADDED,
    "pr_merged",
];

pub async fn ensure_watcher<C: ConnectionTrait>(
    db: &C,
    task_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    task_watchers::Entity::insert(task_watchers::ActiveModel {
        task_id: Set(task_id),
        user_id: Set(user_id),
        created_at: Set(chrono::Utc::now()),
    })
    .on_conflict(
        OnConflict::columns([task_watchers::Column::TaskId, task_watchers::Column::UserId])
            .do_nothing()
            .to_owned(),
    )
    .exec_without_returning(db)
    .await?;
    Ok(())
}

async fn in_app_enabled<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    project_id: Uuid,
    event_type: &str,
) -> Result<bool, AppError> {
    let settings = notification_settings::Entity::find()
        .filter(notification_settings::Column::UserId.eq(user_id))
        .filter(notification_settings::Column::ProjectId.eq(project_id))
        .one(db)
        .await?;

    let events: Vec<String> = match settings {
        Some(s) => s.in_app_events,
        None => DEFAULT_IN_APP_EVENTS
            .iter()
            .map(|e| (*e).to_string())
            .collect(),
    };
    Ok(events.iter().any(|e| e == event_type))
}

pub async fn create_notification<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    task_id: Option<Uuid>,
    notification_type: &str,
    payload: Json,
) -> Result<(), AppError> {
    notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        task_id: Set(task_id),
        notification_type: Set(notification_type.to_string()),
        payload: Set(payload),
        read_at: Set(None),
        created_at: Set(chrono::Utc::now()),
    }
    .insert(db)
    .await?;
    Ok(())
}

async fn notify_user_if_enabled<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    project_id: Uuid,
    task_id: Uuid,
    notification_type: &str,
    payload: Json,
) -> Result<(), AppError> {
    if !in_app_enabled(db, user_id, project_id, notification_type).await? {
        return Ok(());
    }
    create_notification(db, user_id, Some(task_id), notification_type, payload).await
}

pub async fn notify_assigned<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    assignee_id: Uuid,
    assigned_by: Uuid,
    role: &str,
) -> Result<(), AppError> {
    ensure_watcher(db, task_id, assignee_id).await?;
    let assigner_name = users::Entity::find_by_id(assigned_by)
        .one(db)
        .await?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".into());
    notify_user_if_enabled(
        db,
        assignee_id,
        project_id,
        task_id,
        TYPE_ASSIGNED,
        serde_json::json!({
            "assigned_by": assigner_name,
            "role": role,
        })
        .into(),
    )
    .await
}

pub async fn notify_watchers<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    notification_type: &str,
    payload: Json,
    exclude: &[Uuid],
) -> Result<(), AppError> {
    let exclude_set: HashSet<Uuid> = exclude.iter().copied().collect();
    let watchers = task_watchers::Entity::find()
        .filter(task_watchers::Column::TaskId.eq(task_id))
        .all(db)
        .await?;
    for watcher in watchers {
        if exclude_set.contains(&watcher.user_id) {
            continue;
        }
        notify_user_if_enabled(
            db,
            watcher.user_id,
            project_id,
            task_id,
            notification_type,
            payload.clone(),
        )
        .await?;
    }
    Ok(())
}

pub async fn notify_mentioned<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    mentioned_user_ids: &[Uuid],
    comment_id: Uuid,
    author_id: Uuid,
) -> Result<(), AppError> {
    let author_name = users::Entity::find_by_id(author_id)
        .one(db)
        .await?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".into());
    let payload: Json = serde_json::json!({
        "comment_id": comment_id,
        "author": author_name,
    })
    .into();
    for user_id in mentioned_user_ids {
        if *user_id == author_id {
            continue;
        }
        notify_user_if_enabled(
            db,
            *user_id,
            project_id,
            task_id,
            TYPE_MENTIONED,
            payload.clone(),
        )
        .await?;
    }
    Ok(())
}

pub async fn notify_comment_added<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    comment_id: Uuid,
    author_id: Uuid,
    mentioned_user_ids: &[Uuid],
) -> Result<(), AppError> {
    let author_name = users::Entity::find_by_id(author_id)
        .one(db)
        .await?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".into());
    let payload: Json = serde_json::json!({
        "comment_id": comment_id,
        "author": author_name,
    })
    .into();
    let mut exclude = vec![author_id];
    exclude.extend_from_slice(mentioned_user_ids);
    notify_watchers(
        db,
        project_id,
        task_id,
        TYPE_COMMENT_ADDED,
        payload,
        &exclude,
    )
    .await
}

pub async fn notify_status_changed<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    actor_id: Uuid,
    from: &str,
    to: &str,
) -> Result<(), AppError> {
    let actor_name = users::Entity::find_by_id(actor_id)
        .one(db)
        .await?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".into());
    notify_watchers(
        db,
        project_id,
        task_id,
        TYPE_STATUS_CHANGED,
        serde_json::json!({
            "from": from,
            "to": to,
            "changed_by": actor_name,
        })
        .into(),
        &[actor_id],
    )
    .await
}
