use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use axum_valid::Valid;
use chrono::{DateTime, Utc};
use common::notifications::{
    TYPE_ASSIGNED, TYPE_COMMENT_ADDED, TYPE_DEADLINE_SOON, TYPE_MENTIONED, TYPE_PR_MERGED,
    TYPE_REVIEW_FINDING_CHANGED, TYPE_REVIEW_ROUND_CREATED, TYPE_STATUS_CHANGED,
};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::sea_query::{Expr, LikeExpr, Order};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, prelude::Uuid,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::AppState;
use crate::auth_helpers::visible_project_ids;
use crate::error::AppError;
use crate::extractors::{AuthMethod, AuthUser};
use crate::handlers::tasks::resolve_task;
use crate::openapi::CrudErrors;
use common::cursor::{decode_cursor, encode_cursor};
use entity::scopes::Scope;
use entity::{
    notification_settings, notifications, projects, task_watchers, tasks, tenant_members, tenants,
    users,
};
use payload::task_notifications::*;
use service::notifications::{DEFAULT_IN_APP_EVENTS, ensure_watcher};

/// ユーザーがアクセス可能なプロジェクトID一覧を返す（メンバー or テナントオーナー）。
/// list / count / read-all / read-one で共用するアクセス制御ロジック。
async fn accessible_project_ids(
    db: &sea_orm::DatabaseConnection,
    user_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
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

    // 所属テナントのプロジェクトのうち、メンバー未指定のものと自分が指定されたもの（#568）
    let joined_tenant_ids: Vec<Uuid> = tenant_members::Entity::find()
        .filter(tenant_members::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|m| m.tenant_id)
        .collect();
    let member_project_ids: HashSet<Uuid> = if joined_tenant_ids.is_empty() {
        HashSet::new()
    } else {
        let candidate_ids: Vec<Uuid> = projects::Entity::find()
            .filter(projects::Column::TenantId.is_in(joined_tenant_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|p| p.id)
            .collect();
        visible_project_ids(db, candidate_ids, user_id).await?
    };

    Ok(member_project_ids
        .into_iter()
        .chain(owner_project_ids)
        .collect())
}

/// 通知の視界。list / read / read-all / unread_count はすべてこれ 1 つで絞る。
struct NotificationScope {
    /// 見てよい通知の `project_id`。
    project_ids: HashSet<Uuid>,
    /// `project_id IS NULL` の通知（レビュー通知にプロジェクトを持たせる前の行）を見せるか。
    /// どのプロジェクトのものか判別できないので、束縛の無いセッションにだけ見せる。
    include_unscoped: bool,
    /// PAT は操作に必要なスコープを持つ通知種別だけ。セッションは制限しない。
    notification_types: Option<Vec<&'static str>>,
}

/// PAT に `read:task` / `write:task` で見せる通知種別。
const PAT_TASK_NOTIFICATION_TYPES: [&str; 6] = [
    TYPE_ASSIGNED,
    TYPE_MENTIONED,
    TYPE_STATUS_CHANGED,
    TYPE_COMMENT_ADDED,
    TYPE_DEADLINE_SOON,
    TYPE_PR_MERGED,
];
/// PAT に `read:review` / `write:review` で見せる通知種別。
const PAT_REVIEW_NOTIFICATION_TYPES: [&str; 2] =
    [TYPE_REVIEW_ROUND_CREATED, TYPE_REVIEW_FINDING_CHANGED];

/// 認証方式ごとの通知の視界を出す。
///
/// セッションは本人が入れるプロジェクトすべて。PAT は「PAT のテナントのプロジェクト
/// ∩ `allowed_project_ids`」まで絞る（バインドは所属の証明ではないので、所属判定
/// 込みの `accessible_project_ids` との積を取る）。
async fn notification_scope_for(
    db: &sea_orm::DatabaseConnection,
    auth: &AuthUser,
    task_scope: Scope,
    review_scope: Scope,
) -> Result<NotificationScope, AppError> {
    let allow_tasks = auth.require_scope(task_scope).is_ok();
    let allow_reviews = auth.require_scope(review_scope).is_ok();
    if !allow_tasks && !allow_reviews {
        return Err(AppError::Forbidden);
    }
    let mut project_ids = accessible_project_ids(db, auth.user_id).await?;
    let AuthMethod::PersonalToken {
        tenant_id,
        allowed_project_ids,
        ..
    } = &auth.method
    else {
        return Ok(NotificationScope {
            project_ids,
            include_unscoped: true,
            notification_types: None,
        });
    };

    let tenant_project_ids: HashSet<Uuid> = projects::Entity::find()
        .filter(projects::Column::TenantId.eq(*tenant_id))
        .all(db)
        .await?
        .into_iter()
        .map(|p| p.id)
        .collect();
    project_ids.retain(|id| {
        tenant_project_ids.contains(id)
            && allowed_project_ids
                .as_ref()
                .is_none_or(|allowed| allowed.contains(id))
    });
    let mut notification_types = Vec::new();
    if allow_tasks {
        notification_types.extend(PAT_TASK_NOTIFICATION_TYPES);
    }
    if allow_reviews {
        notification_types.extend(PAT_REVIEW_NOTIFICATION_TYPES);
    }
    Ok(NotificationScope {
        project_ids,
        include_unscoped: false,
        notification_types: Some(notification_types),
    })
}

/// 通知クエリにアクセス制御条件を追加するヘルパー。
///
/// プロジェクトの視界と、操作に必要なスコープを持つ通知種別の両方で絞る。
fn accessible_notification_condition(scope: &NotificationScope) -> Condition {
    let mut condition = Condition::any().add(
        notifications::Column::ProjectId
            .is_in(scope.project_ids.iter().copied().collect::<Vec<_>>()),
    );
    if scope.include_unscoped {
        condition = condition.add(notifications::Column::ProjectId.is_null());
    }
    let mut condition = Condition::all().add(condition);
    if let Some(types) = &scope.notification_types {
        condition = condition.add(notifications::Column::NotificationType.is_in(types.clone()));
    }
    condition
}

#[utoipa::path(get, path = "/{id}/watchers", tag = "Tasks", responses((status = 200, body = WatcherListResponse), CrudErrors))]
#[axum::debug_handler]
pub async fn list_watchers(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<WatcherListResponse>, AppError> {
    auth.require_scope(entity::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
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
                created_at: w.created_at.with_timezone(&Utc),
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
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
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
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;
    task_watchers::Entity::delete_many()
        .filter(task_watchers::Column::TaskId.eq(task.id))
        .filter(task_watchers::Column::UserId.eq(auth.user_id))
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 通知一覧のカーソル。並び順（`created_at, id`）のキーをそのまま持つ。
#[derive(Serialize, Deserialize)]
struct NotificationCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn row_cursor(row: &notifications::Model) -> String {
    encode_cursor(&NotificationCursor {
        created_at: row.created_at.with_timezone(&Utc),
        id: row.id,
    })
}

/// 通知行をレスポンスへ変換する。タスクとプロジェクトの要約をまとめて引く。
async fn notification_items(
    db: &sea_orm::DatabaseConnection,
    rows: Vec<notifications::Model>,
) -> Result<Vec<NotificationItem>, AppError> {
    let task_ids: Vec<Uuid> = rows.iter().filter_map(|n| n.task_id).collect();
    let tasks_map: HashMap<Uuid, tasks::Model> = if task_ids.is_empty() {
        HashMap::new()
    } else {
        tasks::Entity::find()
            .filter(tasks::Column::Id.is_in(task_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|t| (t.id, t))
            .collect()
    };
    let project_ids: HashSet<Uuid> = rows.iter().filter_map(|n| n.project_id).collect();
    let projects_map: HashMap<Uuid, projects::Model> = if project_ids.is_empty() {
        HashMap::new()
    } else {
        projects::Entity::find()
            .filter(projects::Column::Id.is_in(project_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|p| (p.id, p))
            .collect()
    };

    Ok(rows
        .into_iter()
        .map(|row| {
            let task = row.task_id.and_then(|tid| {
                tasks_map.get(&tid).map(|t| NotificationTaskSummary {
                    id: t.id,
                    seq_id: t.seq_id,
                    title: t.title.clone(),
                })
            });
            let project = row.project_id.and_then(|pid| {
                projects_map.get(&pid).map(|p| NotificationProjectSummary {
                    tenant_id: p.tenant_id,
                    id: p.id,
                    key: p.key.clone(),
                })
            });
            NotificationItem {
                id: row.id,
                cursor: row_cursor(&row),
                notification_type: row.notification_type,
                project_id: row.project_id,
                project,
                task,
                payload: row.payload,
                read_at: row.read_at.map(|dt| dt.with_timezone(&Utc)),
                created_at: row.created_at.with_timezone(&Utc),
            }
        })
        .collect())
}

#[utoipa::path(
    get,
    path = "/me/notifications",
    tag = "Notifications",
    params(ListNotificationsQuery),
    responses((status = 200, body = NotificationListResponse), CrudErrors)
)]
#[axum::debug_handler]
pub async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListNotificationsQuery>,
) -> Result<Json<NotificationListResponse>, AppError> {
    let scope =
        notification_scope_for(&state.db, &auth, Scope::ReadTask, Scope::ReadReview).await?;
    if q.cursor.is_some() && q.after.is_some() {
        return Err(AppError::BadRequestDetail(
            "cursor and after cannot be used together".into(),
        ));
    }

    let unread_count: u64 = notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(auth.user_id))
        .filter(notifications::Column::ReadAt.is_null())
        .filter(accessible_notification_condition(&scope))
        .count(&state.db)
        .await?;

    let limit = q
        .limit
        .unwrap_or(DEFAULT_NOTIFICATIONS_LIMIT)
        .clamp(1, MAX_NOTIFICATIONS_LIMIT);
    let mut query =
        notifications::Entity::find().filter(notifications::Column::UserId.eq(auth.user_id));
    if q.unread == Some(true) {
        query = query.filter(notifications::Column::ReadAt.is_null());
    }
    // `_` は LIKE の 1 文字ワイルドカードなので逃がす
    let review_prefix = LikeExpr::new(r"review\_%").escape('\\');
    match q.kind {
        Some(NotificationKind::Review) => {
            query = query.filter(notifications::Column::NotificationType.like(review_prefix));
        }
        Some(NotificationKind::Task) => {
            query = query.filter(notifications::Column::NotificationType.not_like(review_prefix));
        }
        None => {}
    }
    // DBクエリレベルでアクセス可能な通知のみ取得する（ページング後に絞り込むと件数が減る）
    query = query.filter(accessible_notification_condition(&scope));

    // 続きは offset ではなくカーソルで継ぐ。通知は先頭（新しい側）に積まれるので、
    // offset だとページの間に 1 件届くだけで境界の行が二重に出る。
    // 不等式は並びと同じ (created_at, id) の組で比べる（同時刻の行の順序は未定義なので
    // id のタイブレーカーが無いと境界で重複・欠落が出る）
    let newer = q.after.is_some();
    if let Some(raw) = q.after.as_deref().or(q.cursor.as_deref()) {
        let c: NotificationCursor = decode_cursor(raw)?;
        let at: DateTimeWithTimeZone = c.created_at.into();
        let (created, tie) = if newer {
            (
                notifications::Column::CreatedAt.gt(at),
                notifications::Column::Id.gt(c.id),
            )
        } else {
            (
                notifications::Column::CreatedAt.lt(at),
                notifications::Column::Id.lt(c.id),
            )
        };
        query = query.filter(
            Condition::any().add(created).add(
                Condition::all()
                    .add(notifications::Column::CreatedAt.eq(at))
                    .add(tie),
            ),
        );
    }
    // after は古い順。新しい側から切ると after との間の行を引く手段が無くなる
    let order = if newer { Order::Asc } else { Order::Desc };

    // 「まだ残っているか」は 1 件多く引いて確かめる
    let mut rows = query
        .order_by(notifications::Column::CreatedAt, order.clone())
        .order_by(notifications::Column::Id, order)
        .limit(limit + 1)
        .all(&state.db)
        .await?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next_cursor = if has_more {
        rows.last().map(row_cursor)
    } else {
        None
    };

    Ok(Json(NotificationListResponse {
        unread_count,
        next_cursor,
        notifications: notification_items(&state.db, rows).await?,
    }))
}

#[utoipa::path(patch, path = "/me/notifications/{id}/read", tag = "Notifications", responses((status = 200, body = NotificationItem), CrudErrors))]
#[axum::debug_handler]
pub async fn mark_notification_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<NotificationItem>, AppError> {
    let scope =
        notification_scope_for(&state.db, &auth, Scope::WriteTask, Scope::WriteReview).await?;

    let notification = notifications::Entity::find_by_id(id)
        .filter(notifications::Column::UserId.eq(auth.user_id))
        .filter(accessible_notification_condition(&scope))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let row = if notification.read_at.is_some() {
        notification
    } else {
        let mut active: notifications::ActiveModel = notification.into();
        active.read_at = Set(Some(chrono::Utc::now().into()));
        active.update(&state.db).await?
    };
    notification_items(&state.db, vec![row])
        .await?
        .pop()
        .map(Json)
        .ok_or(AppError::NotFound)
}

#[utoipa::path(patch, path = "/me/notifications/read-all", tag = "Notifications", responses((status = 204), CrudErrors))]
#[axum::debug_handler]
pub async fn mark_all_notifications_read(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, AppError> {
    let scope =
        notification_scope_for(&state.db, &auth, Scope::WriteTask, Scope::WriteReview).await?;

    let mut update = notifications::Entity::update_many()
        .col_expr(
            notifications::Column::ReadAt,
            Expr::value(chrono::Utc::now()),
        )
        .filter(notifications::Column::UserId.eq(auth.user_id))
        .filter(notifications::Column::ReadAt.is_null());
    update = update.filter(accessible_notification_condition(&scope));
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
    auth.require_scope(entity::scopes::Scope::ReadTask)?;
    let project = projects::Entity::find_by_id(project_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    auth.ensure_tenant_access(&state, project.tenant_id, Some(project_id))
        .await?;
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
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    let project = projects::Entity::find_by_id(project_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    auth.ensure_tenant_access(&state, project.tenant_id, Some(project_id))
        .await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use common::notifications::{KNOWN_EVENT_TYPES, TYPE_REVIEW_ROUND_ANY};

    /// 種別を common に足したのに PAT の一覧へ書き足し忘れると、CLI は設定値として
    /// 受け付けるのに PAT の一覧には出ない。購読の印（`review_round_any`）は通知行を
    /// 作らないので除く。
    #[test]
    fn pat_notification_types_cover_every_known_event_type() {
        let mut pat: Vec<&str> = PAT_TASK_NOTIFICATION_TYPES
            .into_iter()
            .chain(PAT_REVIEW_NOTIFICATION_TYPES)
            .collect();
        pat.sort_unstable();
        let mut known: Vec<&str> = KNOWN_EVENT_TYPES
            .iter()
            .copied()
            .filter(|t| *t != TYPE_REVIEW_ROUND_ANY)
            .collect();
        known.sort_unstable();
        assert_eq!(pat, known);
    }
}
