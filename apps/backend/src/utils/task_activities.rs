use regex::Regex;
use sea_orm::entity::prelude::Json;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    prelude::Uuid,
};
use std::sync::LazyLock;

use crate::entities::{project_members, project_statuses, task_activities, tasks, users};
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

pub async fn status_name<C: ConnectionTrait>(db: &C, status_id: Uuid) -> Result<String, AppError> {
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

pub async fn extract_mentions<C: ConnectionTrait>(
    db: &C,
    body: &str,
    project_id: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    // Collect unique usernames preserving first-occurrence order
    let mut seen = std::collections::HashSet::new();
    let usernames: Vec<&str> = MENTION_RE
        .captures_iter(body)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str()))
        .filter(|u| seen.insert(*u))
        .collect();

    if usernames.is_empty() {
        return Ok(vec![]);
    }

    // Fetch project members to enforce project boundary
    let member_ids: std::collections::HashSet<Uuid> = project_members::Entity::find()
        .filter(project_members::Column::ProjectId.eq(project_id))
        .all(db)
        .await?
        .into_iter()
        .map(|m| m.user_id)
        .collect();

    // Single batch query for all mentioned users
    let matched = users::Entity::find()
        .filter(users::Column::Username.is_in(usernames))
        .all(db)
        .await?;

    let mut user_ids: Vec<Uuid> = Vec::new();
    let mut seen_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for u in matched {
        if member_ids.contains(&u.id) && seen_ids.insert(u.id) {
            user_ids.push(u.id);
        }
    }
    Ok(user_ids)
}
