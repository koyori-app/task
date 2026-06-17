use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use axum_valid::Valid;
use sea_orm::sea_query::{Expr, Order};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, prelude::Uuid,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use utoipa::ToSchema;
use validator::Validate;

use crate::AppState;
use crate::auth_helpers::require_member_or_owner;
use crate::entities::{
    notification_settings, notifications, project_members, projects, task_watchers, tasks, tenants,
    users,
};
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::handlers::tasks::resolve_task;
use crate::openapi::CrudErrors;
use crate::utils::notifications::{DEFAULT_IN_APP_EVENTS, KNOWN_EVENT_TYPES, ensure_watcher};

/// ユーザーがアクセス可能なプロジェクトID一覧を返す（メンバー or テナントオーナー）。
/// list / count / read-all / read-one で共用するアクセス制御ロジック。
async fn accessible_project_ids(
    db: &sea_orm::DatabaseConnection,
    user_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    let member_project_ids: HashSet<Uuid> = project_members::Entity::find()
        .filter(project_members::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|m| m.project_id)
        .collect();
    let owned_tenant_ids: Vec<Uuid> = tenants::Entity::find()
        .filter(tenants::Column::OwnerId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|t| t.id)
        .collect();
    let owner_project_ids: HashSet<Uuid> = if owned_tenant_ids.is_empty() {
        HashSet::new()
    } else {
        projects::Entity::find()
            .filter(projects::Column::TenantId.is_in(owned_tenant_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|p| p.id)
            .collect()
    };
    Ok(member_project_ids
        .into_iter()
        .chain(owner_project_ids)
        .collect())
}

/// アクセス可能なプロジェクトに属するタスクID一覧を返す。
/// プロジェクトがない場合は空Vecを返す。
async fn accessible_task_ids(
    db: &sea_orm::DatabaseConnection,
    project_ids: &HashSet<Uuid>,
) -> Result<Vec<Uuid>, AppError> {
    if project_ids.is_empty() {
        return Ok(vec![]);
    }
    Ok(tasks::Entity::find()
        .select_only()
        .column(tasks::Column::Id)
        .filter(tasks::Column::ProjectId.is_in(project_ids.iter().cloned().collect::<Vec<_>>()))
        .into_tuple::<Uuid>()
        .all(db)
        .await?)
}

/// 通知クエリにアクセス制御条件を追加するヘルパー。
/// task_id IS NULL（タスクに紐付かない通知）は常に許可。
fn accessible_notification_condition(task_ids: Vec<Uuid>) -> Condition {
    Condition::any()
        .add(notifications::Column::TaskId.is_null())
        .add(notifications::Column::TaskId.is_in(task_ids))
}

fn validate_known_event_types(events: &Vec<String>) -> Result<(), validator::ValidationError> {
    for e in events {
        if !KNOWN_EVENT_TYPES.contains(&e.as_str()) {
            return Err(validator::ValidationError::new("unknown_event_type"));
        }
    }
    Ok(())
}

#[derive(Serialize, ToSchema)]
pub struct WatcherUser {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct WatcherListResponse {
    pub watchers: Vec<WatcherUser>,
}

#[derive(Serialize, ToSchema)]
pub struct NotificationTaskSummary {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub seq_id: i32,
    pub title: String,
}

#[derive(Serialize, ToSchema)]
pub struct NotificationItem {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub notification_type: String,
    #[schema(nullable)]
    pub task: Option<NotificationTaskSummary>,
    #[schema(value_type = serde_json::Value)]
    pub payload: serde_json::Value,
    #[schema(nullable, value_type = Option<String>, format = "date-time")]
    pub read_at: Option<chrono::DateTime<chrono::Utc>>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct NotificationListResponse {
    pub unread_count: u64,
    pub notifications: Vec<NotificationItem>,
}

#[derive(Deserialize, ToSchema)]
pub struct ListNotificationsQuery {
    pub unread: Option<bool>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub struct NotificationSettingsResponse {
    pub email_events: Vec<String>,
    pub in_app_events: Vec<String>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateNotificationSettingsRequest {
    #[validate(custom(function = "validate_known_event_types"))]
    pub email_events: Vec<String>,
    #[validate(custom(function = "validate_known_event_types"))]
    pub in_app_events: Vec<String>,
}

#[utoipa::path(get, path = "/{id}/watchers", tag = "Tasks", responses((status = 200, body = WatcherListResponse), CrudErrors))]
#[axum::debug_handler]
pub async fn list_watchers(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<WatcherListResponse>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    let rows = task_watchers::Entity::find()
        .filter(task_watchers::Column::TaskId.eq(task.id))
        .order_by_asc(task_watchers::Column::CreatedAt)
        .all(&state.db)
        .await?;
    let user_ids: Vec<Uuid> = rows.iter().map(|w| w.user_id).collect();
    let users_map: HashMap<Uuid, String> = if user_ids.is_empty() {
        HashMap::new()
    } else {
        users::Entity::find()
            .filter(users::Column::Id.is_in(user_ids))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|u| (u.id, u.username))
            .collect()
    };
    Ok(Json(WatcherListResponse {
        watchers: rows
            .into_iter()
            .map(|w| WatcherUser {
                id: w.user_id,
                name: users_map
                    .get(&w.user_id)
                    .cloned()
                    .unwrap_or_else(|| "unknown".into()),
                created_at: w.created_at,
            })
            .collect(),
    }))
}

#[utoipa::path(post, path = "/{id}/watch", tag = "Tasks", responses((status = 201), CrudErrors))]
#[axum::debug_handler]
pub async fn start_watch(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    ensure_watcher(&state.db, task.id, auth.user_id).await?;
    Ok(StatusCode::CREATED)
}

#[utoipa::path(delete, path = "/{id}/watch", tag = "Tasks", responses((status = 204), CrudErrors))]
#[axum::debug_handler]
pub async fn stop_watch(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, tenant_id, project_id, auth.user_id).await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    task_watchers::Entity::delete_many()
        .filter(task_watchers::Column::TaskId.eq(task.id))
        .filter(task_watchers::Column::UserId.eq(auth.user_id))
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/me/notifications", tag = "Notifications", responses((status = 200, body = NotificationListResponse), CrudErrors))]
#[axum::debug_handler]
pub async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListNotificationsQuery>,
) -> Result<Json<NotificationListResponse>, AppError> {
    auth.require_session()?;

    let accessible_proj_ids = accessible_project_ids(&state.db, auth.user_id).await?;
    let task_ids = accessible_task_ids(&state.db, &accessible_proj_ids).await?;

    let unread_count: u64 = notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(auth.user_id))
        .filter(notifications::Column::ReadAt.is_null())
        .filter(accessible_notification_condition(task_ids.clone()))
        .count(&state.db)
        .await?;

    let limit = q.limit.unwrap_or(50).min(100);
    let offset = q.offset.unwrap_or(0);
    let mut query =
        notifications::Entity::find().filter(notifications::Column::UserId.eq(auth.user_id));
    if q.unread == Some(true) {
        query = query.filter(notifications::Column::ReadAt.is_null());
    }
    // DBクエリレベルでアクセス可能な通知のみ取得する（ページング後に絞り込むと件数が減る）
    query = query.filter(accessible_notification_condition(task_ids.clone()));
    let rows = query
        .order_by(
            Expr::cust("CASE WHEN read_at IS NULL THEN 0 ELSE 1 END"),
            Order::Asc,
        )
        .order_by_desc(notifications::Column::CreatedAt)
        .limit(limit)
        .offset(offset)
        .all(&state.db)
        .await?;

    let notification_task_ids: Vec<Uuid> = rows.iter().filter_map(|n| n.task_id).collect();
    let tasks_map: HashMap<Uuid, tasks::Model> = if notification_task_ids.is_empty() {
        HashMap::new()
    } else {
        tasks::Entity::find()
            .filter(tasks::Column::Id.is_in(notification_task_ids))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|t| (t.id, t))
            .collect()
    };

    Ok(Json(NotificationListResponse {
        unread_count,
        notifications: rows
            .into_iter()
            .map(|row| {
                let task = row.task_id.and_then(|tid| {
                    tasks_map.get(&tid).map(|t| NotificationTaskSummary {
                        id: t.id,
                        seq_id: t.seq_id,
                        title: t.title.clone(),
                    })
                });
                NotificationItem {
                    id: row.id,
                    notification_type: row.notification_type,
                    task,
                    payload: row.payload.clone().into(),
                    read_at: row.read_at,
                    created_at: row.created_at,
                }
            })
            .collect(),
    }))
}

#[utoipa::path(patch, path = "/me/notifications/{id}/read", tag = "Notifications", responses((status = 200, body = NotificationItem), CrudErrors))]
#[axum::debug_handler]
pub async fn mark_notification_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<NotificationItem>, AppError> {
    auth.require_session()?;

    let notification = notifications::Entity::find_by_id(id)
        .filter(notifications::Column::UserId.eq(auth.user_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // task に紐づく通知はアクセス可能なプロジェクトか確認
    if let Some(tid) = notification.task_id {
        let proj_ids = accessible_project_ids(&state.db, auth.user_id).await?;
        let task = tasks::Entity::find_by_id(tid).one(&state.db).await?;
        if let Some(t) = task {
            if !proj_ids.contains(&t.project_id) {
                return Err(AppError::NotFound);
            }
        }
    }

    let row = if notification.read_at.is_some() {
        notification
    } else {
        let mut active: notifications::ActiveModel = notification.into();
        active.read_at = Set(Some(chrono::Utc::now()));
        active.update(&state.db).await?
    };
    let task = if let Some(tid) = row.task_id {
        tasks::Entity::find_by_id(tid)
            .one(&state.db)
            .await?
            .map(|t| NotificationTaskSummary {
                id: t.id,
                seq_id: t.seq_id,
                title: t.title,
            })
    } else {
        None
    };
    Ok(Json(NotificationItem {
        id: row.id,
        notification_type: row.notification_type,
        task,
        payload: row.payload.clone().into(),
        read_at: row.read_at,
        created_at: row.created_at,
    }))
}

#[utoipa::path(patch, path = "/me/notifications/read-all", tag = "Notifications", responses((status = 204), CrudErrors))]
#[axum::debug_handler]
pub async fn mark_all_notifications_read(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, AppError> {
    auth.require_session()?;

    let accessible_proj_ids = accessible_project_ids(&state.db, auth.user_id).await?;
    let task_ids = accessible_task_ids(&state.db, &accessible_proj_ids).await?;

    let mut update = notifications::Entity::update_many()
        .col_expr(
            notifications::Column::ReadAt,
            Expr::value(chrono::Utc::now()),
        )
        .filter(notifications::Column::UserId.eq(auth.user_id))
        .filter(notifications::Column::ReadAt.is_null());
    update = update.filter(accessible_notification_condition(task_ids));
    update.exec(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/me/notification-settings/{project_id}", tag = "Notifications", responses((status = 200, body = NotificationSettingsResponse), CrudErrors))]
#[axum::debug_handler]
pub async fn get_notification_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
) -> Result<Json<NotificationSettingsResponse>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::ReadTask)?;
    let project = projects::Entity::find_by_id(project_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    auth.ensure_tenant_access(&state, project.tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, project.tenant_id, project_id, auth.user_id).await?;
    let settings = notification_settings::Entity::find()
        .filter(notification_settings::Column::UserId.eq(auth.user_id))
        .filter(notification_settings::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?;
    Ok(Json(match settings {
        Some(s) => NotificationSettingsResponse {
            email_events: s.email_events,
            in_app_events: s.in_app_events,
        },
        None => NotificationSettingsResponse {
            email_events: vec![],
            in_app_events: DEFAULT_IN_APP_EVENTS
                .iter()
                .map(|e| (*e).to_string())
                .collect(),
        },
    }))
}

#[utoipa::path(put, path = "/me/notification-settings/{project_id}", tag = "Notifications", responses((status = 200, body = NotificationSettingsResponse), CrudErrors))]
#[axum::debug_handler]
pub async fn update_notification_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Valid(Json(payload)): Valid<Json<UpdateNotificationSettingsRequest>>,
) -> Result<Json<NotificationSettingsResponse>, AppError> {
    auth.require_scope(crate::entities::scopes::Scope::WriteTask)?;
    let project = projects::Entity::find_by_id(project_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    auth.ensure_tenant_access(&state, project.tenant_id, Some(project_id))
        .await?;
    require_member_or_owner(&state, project.tenant_id, project_id, auth.user_id).await?;
    let existing = notification_settings::Entity::find()
        .filter(notification_settings::Column::UserId.eq(auth.user_id))
        .filter(notification_settings::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?;
    let model = if let Some(row) = existing {
        let mut active: notification_settings::ActiveModel = row.into();
        active.email_events = Set(payload.email_events.clone());
        active.in_app_events = Set(payload.in_app_events.clone());
        active.update(&state.db).await?
    } else {
        notification_settings::ActiveModel {
            user_id: Set(auth.user_id),
            project_id: Set(project_id),
            email_events: Set(payload.email_events.clone()),
            in_app_events: Set(payload.in_app_events.clone()),
        }
        .insert(&state.db)
        .await?
    };
    Ok(Json(NotificationSettingsResponse {
        email_events: model.email_events,
        in_app_events: model.in_app_events,
    }))
}
