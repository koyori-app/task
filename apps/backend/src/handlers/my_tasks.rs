use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use axum_valid::Valid;
use chrono::{Duration, NaiveDate, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait,
    JoinType, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait,
    TransactionTrait, prelude::Uuid, sea_query::LockType,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::AppState;
use crate::entities::{
    drive_folders, project_members, project_statuses, project_task_counters, projects,
    scopes::Scope, task_assignees, tasks, users,
};
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use crate::utils::db::is_postgres_unique_violation;
use crate::utils::task_activities::record_activity;

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListMyTasksQuery {
    #[serde(default = "default_filter")]
    pub filter: String,
    #[serde(default = "default_include_personal")]
    pub include_personal: bool,
    pub project_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_filter() -> String {
    "all".to_string()
}

fn default_include_personal() -> bool {
    true
}

fn default_limit() -> u64 {
    50
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct QuickCaptureRequest {
    #[validate(length(min = 1, max = 255))]
    pub title: String,
    pub soft_deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub priority: Option<tasks::TaskPriority>,
    pub note: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct MyTaskProjectInfo {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub key: String,
    pub is_personal: bool,
}

#[derive(Serialize, ToSchema)]
pub struct MyTaskStatusInfo {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    pub color: String,
}

#[derive(Serialize, ToSchema)]
pub struct MyTaskItem {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub seq_id: i32,
    pub seq_key: String,
    pub title: String,
    pub status: MyTaskStatusInfo,
    pub priority: tasks::TaskPriority,
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub soft_deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub hard_deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub project: MyTaskProjectInfo,
    pub is_personal: bool,
}

#[derive(Serialize, ToSchema)]
pub struct MyTasksListResponse {
    pub tasks: Vec<MyTaskItem>,
    pub total: u64,
}

fn personal_project_key(user_id: Uuid) -> String {
    let id_hex = user_id.simple().to_string().to_ascii_uppercase();
    format!("ME{}", &id_hex[..4])
}

async fn next_seq_id(db: &sea_orm::DatabaseTransaction, project_id: Uuid) -> Result<i32, AppError> {
    let existing = project_task_counters::Entity::find_by_id(project_id)
        .lock(LockType::Update)
        .one(db)
        .await?;
    Ok(match existing {
        Some(c) => {
            let new_seq = c.last_seq + 1;
            let mut active: project_task_counters::ActiveModel = c.into();
            active.last_seq = Set(new_seq);
            active.update(db).await?.last_seq
        }
        None => {
            project_task_counters::ActiveModel {
                project_id: Set(project_id),
                last_seq: Set(1),
            }
            .insert(db)
            .await?
            .last_seq
        }
    })
}

async fn default_status_id<C: ConnectionTrait>(db: &C, project_id: Uuid) -> Result<Uuid, AppError> {
    project_statuses::Entity::find()
        .filter(project_statuses::Column::ProjectId.eq(project_id))
        .filter(project_statuses::Column::IsDefault.eq(true))
        .one(db)
        .await?
        .map(|s| s.id)
        .ok_or(AppError::NotFound)
}

async fn seed_personal_project_defaults<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    project_statuses::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        name: Set("Todo".into()),
        color: Set("#6b7280".into()),
        position: Set(0),
        is_default: Set(true),
        is_done_state: Set(false),
        created_at: Set(Utc::now().into()),
    }
    .insert(db)
    .await?;

    project_statuses::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        name: Set("Done".into()),
        color: Set("#22c55e".into()),
        position: Set(1),
        is_default: Set(false),
        is_done_state: Set(true),
        created_at: Set(Utc::now().into()),
    }
    .insert(db)
    .await?;

    project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        user_id: Set(user_id),
        role: Set(project_members::ProjectRole::Admin),
    }
    .insert(db)
    .await?;

    project_task_counters::ActiveModel {
        project_id: Set(project_id),
        last_seq: Set(0),
    }
    .insert(db)
    .await?;

    Ok(())
}

pub(crate) async fn get_or_create_personal_project(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<projects::Model, AppError> {
    if let Some(existing) = projects::Entity::find()
        .filter(projects::Column::TenantId.eq(tenant_id))
        .filter(projects::Column::IsPersonal.eq(true))
        .filter(projects::Column::PersonalOwnerId.eq(user_id))
        .one(&state.db)
        .await?
    {
        return Ok(existing);
    }

    let user = users::Entity::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let mut key = personal_project_key(user_id);
    let name = format!("{}'s Inbox", user.username);

    loop {
        let txn = state.db.begin().await?;

        // txn 内で再確認（並行リクエストが先に作成した場合に対応）
        if let Some(existing) = projects::Entity::find()
            .filter(projects::Column::TenantId.eq(tenant_id))
            .filter(projects::Column::IsPersonal.eq(true))
            .filter(projects::Column::PersonalOwnerId.eq(user_id))
            .one(&txn)
            .await?
        {
            txn.commit().await?;
            return Ok(existing);
        }

        let project_id = Uuid::new_v4();
        let project = projects::ActiveModel {
            id: Set(project_id),
            name: Set(name.clone()),
            description: Set(String::new()),
            tenant_id: Set(tenant_id),
            icon_emoji: Set(None),
            icon_url: Set(None),
            key: Set(key.clone()),
            is_personal: Set(true),
            personal_owner_id: Set(Some(user_id)),
        };

        let model = match project.insert(&txn).await {
            Ok(model) => model,
            Err(e) if is_postgres_unique_violation(&e) => {
                txn.rollback().await?;
                // 並行リクエストが同時に personal project を作成した可能性を確認
                if let Some(existing) = projects::Entity::find()
                    .filter(projects::Column::TenantId.eq(tenant_id))
                    .filter(projects::Column::IsPersonal.eq(true))
                    .filter(projects::Column::PersonalOwnerId.eq(user_id))
                    .one(&state.db)
                    .await?
                {
                    return Ok(existing);
                }
                // key 衝突: 新しい key と新しい txn で再試行
                let suffix = Uuid::new_v4().simple().to_string().to_ascii_uppercase();
                key = format!("ME{}", &suffix[..4]);
                continue;
            }
            Err(e) => {
                txn.rollback().await?;
                return Err(e.into());
            }
        };

        drive_folders::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(model.name.clone()),
            parent_id: Set(None),
            tenant_id: Set(tenant_id),
            project_id: Set(Some(model.id)),
            created_by: Set(user_id),
            created_at: Set(Utc::now().into()),
        }
        .insert(&txn)
        .await?;

        seed_personal_project_defaults(&txn, model.id, user_id).await?;
        txn.commit().await?;
        return Ok(model);
    }
}

fn apply_my_tasks_filter(
    mut query: sea_orm::Select<tasks::Entity>,
    filter: &str,
) -> Result<sea_orm::Select<tasks::Entity>, AppError> {
    let today: NaiveDate = Utc::now().date_naive();
    match filter {
        "all" => Ok(query),
        "today" => {
            query = query.filter(
                Condition::any()
                    .add(sea_orm::sea_query::Expr::cust_with_values(
                        "tasks.soft_deadline IS NOT NULL AND tasks.soft_deadline::date = $1::date",
                        vec![sea_orm::Value::from(today)],
                    ))
                    .add(sea_orm::sea_query::Expr::cust_with_values(
                        "tasks.hard_deadline IS NOT NULL AND tasks.hard_deadline::date = $1::date",
                        vec![sea_orm::Value::from(today)],
                    )),
            );
            Ok(query)
        }
        "week" => {
            let end = today + Duration::days(7);
            query = query.filter(
                Condition::any()
                    .add(sea_orm::sea_query::Expr::cust_with_values(
                        "tasks.soft_deadline IS NOT NULL AND tasks.soft_deadline::date BETWEEN $1::date AND $2::date",
                        vec![sea_orm::Value::from(today), sea_orm::Value::from(end)],
                    ))
                    .add(sea_orm::sea_query::Expr::cust_with_values(
                        "tasks.hard_deadline IS NOT NULL AND tasks.hard_deadline::date BETWEEN $1::date AND $2::date",
                        vec![sea_orm::Value::from(today), sea_orm::Value::from(end)],
                    )),
            );
            Ok(query)
        }
        "no_due_date" => Ok(query
            .filter(tasks::Column::SoftDeadline.is_null())
            .filter(tasks::Column::HardDeadline.is_null())),
        "overdue" => Ok(query
            .join(JoinType::InnerJoin, tasks::Relation::ProjectStatuses.def())
            .filter(tasks::Column::HardDeadline.is_not_null())
            .filter(tasks::Column::HardDeadline.lt(Utc::now()))
            .filter(project_statuses::Column::IsDoneState.eq(false))),
        _ => Err(AppError::BadRequest),
    }
}

fn build_my_task_item(
    task: tasks::Model,
    project: &projects::Model,
    status: &project_statuses::Model,
) -> MyTaskItem {
    MyTaskItem {
        id: task.id,
        seq_id: task.seq_id,
        seq_key: format!("{}-{}", project.key, task.seq_id),
        title: task.title,
        status: MyTaskStatusInfo {
            id: status.id,
            name: status.name.clone(),
            color: status.color.clone(),
        },
        priority: task.priority,
        soft_deadline: task.soft_deadline,
        hard_deadline: task.hard_deadline,
        project: MyTaskProjectInfo {
            id: project.id,
            name: project.name.clone(),
            key: project.key.clone(),
            is_personal: project.is_personal,
        },
        is_personal: project.is_personal,
    }
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/personal-project",
    tag = "My Tasks",
    summary = "個人プロジェクトを取得（未存在なら自動生成）",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    responses(
        (status = 200, description = "個人プロジェクト", body = projects::Model),
        CrudErrors,
    )
)]
pub async fn get_personal_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<projects::Model>, AppError> {
    auth.require_scope(Scope::ReadProject)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;
    let project = get_or_create_personal_project(&state, tenant_id, auth.user_id).await?;
    Ok(Json(project))
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/tasks",
    tag = "My Tasks",
    summary = "自分に割り当てられたタスク一覧（テナント横断）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ListMyTasksQuery,
    ),
    responses(
        (status = 200, description = "My Tasks 一覧", body = MyTasksListResponse),
        CrudErrors,
    )
)]
pub async fn list_my_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Query(q): Query<ListMyTasksQuery>,
) -> Result<Json<MyTasksListResponse>, AppError> {
    auth.require_scope(Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;

    let mut query = tasks::Entity::find()
        .filter(tasks::Column::DeletedAt.is_null())
        .filter(tasks::Column::IsArchived.eq(false))
        .filter(sea_orm::sea_query::Expr::cust_with_values(
            "EXISTS (
                SELECT 1 FROM task_assignees
                WHERE task_assignees.task_id = tasks.id
                  AND task_assignees.user_id = $1
            )",
            vec![sea_orm::Value::from(auth.user_id)],
        ))
        .filter(sea_orm::sea_query::Expr::cust_with_values(
            "EXISTS (
                SELECT 1 FROM projects
                WHERE projects.id = tasks.project_id
                  AND projects.tenant_id = $1
            )",
            vec![sea_orm::Value::from(tenant_id)],
        ));

    if !q.include_personal {
        query = query.filter(sea_orm::sea_query::Expr::cust(
            "EXISTS (
                SELECT 1 FROM projects
                WHERE projects.id = tasks.project_id
                  AND projects.is_personal = false
            )",
        ));
    }

    if let Some(project_id) = q.project_id {
        query = query.filter(tasks::Column::ProjectId.eq(project_id));
    }

    query = apply_my_tasks_filter(query, &q.filter)?;

    let limit = std::cmp::min(q.limit, 200);
    let total = query.clone().count(&state.db).await?;
    let task_rows = query
        .order_by_desc(tasks::Column::CreatedAt)
        .offset(q.offset)
        .limit(limit)
        .all(&state.db)
        .await?;

    let project_ids: Vec<Uuid> = task_rows.iter().map(|t| t.project_id).collect();
    let status_ids: Vec<Uuid> = task_rows.iter().map(|t| t.status_id).collect();

    let projects_map: std::collections::HashMap<Uuid, projects::Model> = if project_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        projects::Entity::find()
            .filter(projects::Column::Id.is_in(project_ids))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|p| (p.id, p))
            .collect()
    };

    let statuses_map: std::collections::HashMap<Uuid, project_statuses::Model> =
        if status_ids.is_empty() {
            std::collections::HashMap::new()
        } else {
            project_statuses::Entity::find()
                .filter(project_statuses::Column::Id.is_in(status_ids))
                .all(&state.db)
                .await?
                .into_iter()
                .map(|s| (s.id, s))
                .collect()
        };

    let mut items = Vec::with_capacity(task_rows.len());
    for task in task_rows {
        let project = projects_map
            .get(&task.project_id)
            .ok_or(AppError::NotFound)?;
        let status = statuses_map
            .get(&task.status_id)
            .ok_or(AppError::NotFound)?;
        items.push(build_my_task_item(task, project, status));
    }

    Ok(Json(MyTasksListResponse {
        tasks: items,
        total,
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/tasks",
    tag = "My Tasks",
    summary = "クイックキャプチャ（個人プロジェクトへタスク作成）",
    params(("tenant_id" = Uuid, Path, description = "テナントID")),
    request_body = QuickCaptureRequest,
    responses(
        (status = 201, description = "作成されたタスク", body = tasks::Model),
        CrudErrors,
    )
)]
pub async fn create_my_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Valid(Json(payload)): Valid<Json<QuickCaptureRequest>>,
) -> Result<(StatusCode, Json<tasks::Model>), AppError> {
    auth.require_scope(Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, None).await?;

    let personal = get_or_create_personal_project(&state, tenant_id, auth.user_id).await?;
    let status_id = default_status_id(&state.db, personal.id).await?;

    let txn = state.db.begin().await?;
    let seq_id = next_seq_id(&txn, personal.id).await?;
    let priority = payload.priority.unwrap_or(tasks::TaskPriority::Medium);

    let model = tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(personal.id),
        seq_id: Set(seq_id),
        title: Set(payload.title),
        description: Set(payload.note),
        status_id: Set(status_id),
        priority: Set(priority),
        progress_pct: Set(0),
        parent_task_id: Set(None),
        milestone_id: Set(None),
        sprint_id: Set(None),
        soft_deadline: Set(payload.soft_deadline),
        hard_deadline: Set(None),
        estimated_minutes: Set(None),
        is_archived: Set(false),
        created_by: Set(auth.user_id),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        completed_at: Set(None),
        deleted_at: Set(None),
    }
    .insert(&txn)
    .await?;

    task_assignees::ActiveModel {
        id: Set(Uuid::new_v4()),
        task_id: Set(model.id),
        user_id: Set(auth.user_id),
        role: Set("assignee".into()),
        assigned_at: Set(Utc::now()),
    }
    .insert(&txn)
    .await?;

    record_activity(
        &txn,
        model.id,
        Some(auth.user_id),
        "task_created",
        serde_json::json!({}).into(),
    )
    .await?;

    txn.commit().await?;
    Ok((StatusCode::CREATED, Json(model)))
}
