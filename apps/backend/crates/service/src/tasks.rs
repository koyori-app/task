//! タスクの共通ロジック。

use sea_orm::sea_query::LockType;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DbBackend, DbErr, EntityTrait,
    FromQueryResult, QuerySelect, Statement, prelude::Uuid,
};

use common::error::AppError;
use entity::{
    labels, milestones, project_statuses, project_task_counters, sprints, task_assignees,
    task_labels, tasks,
};
use payload::tasks::CreateTaskRequest;
use sea_orm::{ActiveEnum, ColumnTrait, QueryFilter};

use crate::access::require_project_access;
use crate::custom_fields::{ensure_required_custom_fields, upsert_task_custom_field_values};
use crate::notifications::{notify_assigned, notify_description_mentioned};
use crate::task_activities::{extract_mentions, record_activity};
use crate::webhooks;

/// プロジェクト内の次の連番を採番する。
/// 行ロックで直列化するため、必ずトランザクション上で呼ぶこと。
pub async fn next_seq_id<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<i32, sea_orm::DbErr> {
    let counter = lock_task_counter(db, project_id).await?;
    let new_seq = counter.last_seq + 1;
    let mut active: project_task_counters::ActiveModel = counter.into();
    active.last_seq = Set(new_seq);
    Ok(active.update(db).await?.last_seq)
}

/// 採番カウンター行を、番号を進めずに `FOR UPDATE` でロックして返す。
/// 行が無ければ 0 で作ってからロックするので、プロジェクトで最初の作成同士も
/// INSERT の衝突にならず、ここで直列化される。
/// 一括作成は sprint をまとめてロックする前に呼び、単体作成と同じ
/// 「カウンター → sprint」の順に揃える。
pub async fn lock_task_counter<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<project_task_counters::Model, sea_orm::DbErr> {
    crate::db::execute_bound(
        db,
        "INSERT INTO project_task_counters (project_id, last_seq) VALUES ($1, 0) \
         ON CONFLICT (project_id) DO NOTHING",
        vec![project_id.into()],
    )
    .await?;
    project_task_counters::Entity::find_by_id(project_id)
        .lock(LockType::Update)
        .one(db)
        .await?
        .ok_or_else(|| DbErr::RecordNotFound("project_task_counters".to_string()))
}

/// 単体作成の本体の文脈（認可済みのテナント・プロジェクト・実行者）。
pub struct CreateTaskContext {
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    pub actor_id: Uuid,
}

/// 単体作成の本体。検証・採番・担当者・ラベル・カスタムフィールド・
/// メンション通知・アクティビティ・Webhook まで行う。
/// 呼び出し側がトランザクションを開き、commit する（採番の行ロックのため）。
pub async fn create_task_in_txn<C: ConnectionTrait>(
    txn: &C,
    ctx: &CreateTaskContext,
    payload: CreateTaskRequest,
) -> Result<tasks::Model, AppError> {
    if let (Some(s), Some(h)) = (payload.soft_deadline, payload.hard_deadline)
        && s >= h
    {
        return Err(AppError::BadRequest);
    }

    let status = project_statuses::Entity::find_by_id(payload.status_id)
        .filter(project_statuses::Column::ProjectId.eq(ctx.project_id))
        .one(txn)
        .await?
        .ok_or(AppError::NotFound)?;

    // parent_task_id / milestone_id が同一プロジェクトに属することを検証
    if let Some(pid) = payload.parent_task_id {
        tasks::Entity::find_by_id(pid)
            .filter(tasks::Column::ProjectId.eq(ctx.project_id))
            .filter(tasks::Column::DeletedAt.is_null())
            .one(txn)
            .await?
            .ok_or(AppError::NotFound)?;
    }
    if let Some(mid) = payload.milestone_id {
        milestones::Entity::find_by_id(mid)
            .filter(milestones::Column::ProjectId.eq(ctx.project_id))
            .one(txn)
            .await?
            .ok_or(AppError::NotFound)?;
    }
    // 採番(カウンター行ロック)を sprint の FOR UPDATE より前に取る。ロック順を「カウンター → sprint」に
    // 固定し、一括作成と単体作成の同時実行でのデッドロックを避ける(ロールバックで番号も戻る)。
    let seq_id = next_seq_id(txn, ctx.project_id).await?;
    if let Some(sid) = payload.sprint_id {
        let sprint = sprints::Entity::find_by_id(sid)
            .filter(sprints::Column::ProjectId.eq(ctx.project_id))
            .lock(LockType::Update)
            .one(txn)
            .await?
            .ok_or(AppError::NotFound)?;
        if sprint.status == sprints::SprintStatus::Completed {
            return Err(AppError::Conflict);
        }
    }

    let model = tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(ctx.project_id),
        seq_id: Set(seq_id),
        title: Set(payload.title),
        description: Set(payload.description),
        status_id: Set(payload.status_id),
        priority: Set(payload.priority.unwrap_or(tasks::TaskPriority::Medium)),
        progress_pct: Set(payload.progress_pct.unwrap_or(0)),
        parent_task_id: Set(payload.parent_task_id),
        milestone_id: Set(payload.milestone_id),
        sprint_id: Set(payload.sprint_id),
        soft_deadline: Set(payload.soft_deadline.map(Into::into)),
        hard_deadline: Set(payload.hard_deadline.map(Into::into)),
        planned_start_on: Set(payload.planned_start_on),
        estimated_minutes: Set(payload.estimated_minutes),
        is_archived: Set(false),
        created_by: Set(ctx.actor_id),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        completed_at: Set(status.is_done_state.then(|| chrono::Utc::now().into())),
        deleted_at: Set(None),
    }
    .insert(txn)
    .await?;

    for a in &payload.assignees {
        require_project_access(txn, ctx.tenant_id, ctx.project_id, a.user_id).await?;
        task_assignees::ActiveModel {
            id: Set(Uuid::new_v4()),
            task_id: Set(model.id),
            user_id: Set(a.user_id),
            role: Set(a.role.clone()),
            assigned_at: Set(chrono::Utc::now().into()),
        }
        .insert(txn)
        .await?;
        notify_assigned(
            txn,
            ctx.project_id,
            model.id,
            a.user_id,
            ctx.actor_id,
            &a.role,
        )
        .await?;
    }
    let unique_label_ids: Vec<Uuid> = {
        let mut v = payload.label_ids.clone();
        v.sort();
        v.dedup();
        v
    };
    if !unique_label_ids.is_empty() {
        let labels_in_project = labels::Entity::find()
            .filter(labels::Column::Id.is_in(unique_label_ids.clone()))
            .filter(labels::Column::ProjectId.eq(ctx.project_id))
            .all(txn)
            .await?;
        if labels_in_project.len() != unique_label_ids.len() {
            return Err(AppError::BadRequest);
        }
    }
    for lid in &unique_label_ids {
        task_labels::ActiveModel {
            task_id: Set(model.id),
            label_id: Set(*lid),
        }
        .insert(txn)
        .await?;
    }

    if status.is_done_state {
        ensure_required_custom_fields(
            txn,
            ctx.project_id,
            model.id,
            Some(&payload.custom_field_values),
        )
        .await?;
    }
    upsert_task_custom_field_values(txn, ctx.project_id, model.id, &payload.custom_field_values)
        .await?;

    // 説明文のメンション。宛先の境はコメントと同じ（`service::mentions`）。自分自身は通知しない
    if let Some(description) = model.description.as_deref() {
        let mentioned = extract_mentions(txn, description, ctx.project_id).await?;
        notify_description_mentioned(txn, ctx.project_id, model.id, &mentioned, ctx.actor_id)
            .await?;
    }

    record_activity(
        txn,
        model.id,
        Some(ctx.actor_id),
        "task_created",
        serde_json::json!({}),
    )
    .await?;
    webhooks::emit(
        txn,
        ctx.project_id,
        ctx.actor_id,
        webhooks::EVENT_TASK_CREATED,
        serde_json::json!({
            "task": {
                "id": model.id,
                "seq_id": model.seq_id,
                "title": &model.title,
                "priority": model.priority.to_value(),
                "status": &status.name,
                "assignees": payload
                    .assignees
                    .iter()
                    .map(|a| serde_json::json!({ "user_id": a.user_id, "role": a.role }))
                    .collect::<Vec<_>>(),
                "hard_deadline": model.hard_deadline,
                "created_by": model.created_by,
            },
        }),
    )
    .await?;
    Ok(model)
}

/// 類似タスク候補の 1 件（apps/backend/docs/task-similarity-search.md §5）。
#[derive(Debug, FromQueryResult)]
pub struct SimilarTask {
    pub id: Uuid,
    pub seq_id: i32,
    pub title: String,
    pub score: f32,
}

/// 同一プロジェクト内で title が `text` に trigram で似ているタスクを類似度の高い順に返す。
/// `text` は trim 済み・非空であること。
pub async fn find_similar(
    db: &impl ConnectionTrait,
    project_id: Uuid,
    text: &str,
    exclude_task_id: Option<Uuid>,
    limit: u64,
) -> Result<Vec<SimilarTask>, DbErr> {
    SimilarTask::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        r#"
        SELECT id, seq_id, title, similarity(title, $2)::real AS score
        FROM tasks
        WHERE project_id = $1
          AND deleted_at IS NULL
          AND ($3::uuid IS NULL OR id <> $3)
          AND title % $2
        -- score の同値で順序が揺れないよう id で固定（search_tasks と同じ理由）
        ORDER BY score DESC, id ASC
        LIMIT $4
        "#,
        [
            project_id.into(),
            text.into(),
            exclude_task_id.into(),
            (limit as i64).into(),
        ],
    ))
    .all(db)
    .await
}
