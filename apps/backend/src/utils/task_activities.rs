use regex::Regex;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    prelude::Uuid,
};
use sea_orm::entity::prelude::Json;
use std::sync::LazyLock;

use crate::entities::{project_statuses, task_activities, tasks, users};
use crate::error::AppError;

static MENTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@([a-zA-Z0-9_-]+)").expect("mention regex"));

pub async fn record_activity<C: ConnectionTrait>(
    db: &C,
    task_id: Uuid,
    user_id: Option<Uuid>,
    event_type: &str,
    payload: Json,
) -> Result<(), AppError> {
    task_activities::ActiveModel {
        id: Set(Uuid::new_v4()),
        task_id: Set(task_id),
        user_id: Set(user_id),
        event_type: Set(event_type.to_string()),
        payload: Set(payload),
        created_at: Set(chrono::Utc::now()),
    }
    .insert(db)
    .await?;
    Ok(())
}

pub async fn status_name<C: ConnectionTrait>(
    db: &C,
    status_id: Uuid,
) -> Result<String, AppError> {
    project_statuses::Entity::find_by_id(status_id)
        .one(db)
        .await?
        .map(|s| s.name)
        .ok_or(AppError::NotFound)
}

pub fn priority_label(priority: tasks::TaskPriority) -> &'static str {
    match priority {
        tasks::TaskPriority::CriticalFire => "critical_fire",
        tasks::TaskPriority::Critical => "critical",
        tasks::TaskPriority::High => "high",
        tasks::TaskPriority::Medium => "medium",
        tasks::TaskPriority::Low => "low",
        tasks::TaskPriority::Trivial => "trivial",
    }
}

pub async fn extract_mentions<C: ConnectionTrait>(db: &C, body: &str) -> Result<Vec<Uuid>, AppError> {
    let mut user_ids = Vec::new();
    for cap in MENTION_RE.captures_iter(body) {
        let username = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
        if let Some(user) = users::Entity::find()
            .filter(users::Column::Username.eq(username))
            .one(db)
            .await?
        {
            if !user_ids.contains(&user.id) {
                user_ids.push(user.id);
            }
        }
    }
    Ok(user_ids)
}
