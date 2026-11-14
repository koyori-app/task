//! タスクの共通ロジック。

use chrono::NaiveDate;
use sea_orm::sea_query::LockType;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ConnectionTrait, DbBackend, DbErr, EntityTrait, FromQueryResult, QuerySelect, Statement,
    prelude::{DateTimeWithTimeZone, Uuid},
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

/// 作成時の締切（Soft / Hard）と開始予定日。リクエストに無い項目は親の値を初期値にする。
/// 片方の締切だけを指定し、親から引き継ぐもう片方と Soft < Hard にならないときは、
/// 引き継ぐ側を入れない（指定された値を優先し、引き継ぎで 400 を増やさない）。
fn schedule_with_parent_defaults(
    soft: Option<DateTimeWithTimeZone>,
    hard: Option<DateTimeWithTimeZone>,
    planned_start_on: Option<NaiveDate>,
    parent: Option<&tasks::Model>,
) -> (
    Option<DateTimeWithTimeZone>,
    Option<DateTimeWithTimeZone>,
    Option<NaiveDate>,
) {
    let Some(parent) = parent else {
        return (soft, hard, planned_start_on);
    };
    let planned_start_on = planned_start_on.or(parent.planned_start_on);
    let (soft, hard) = match (soft, hard) {
        (Some(s), Some(h)) => (Some(s), Some(h)),
        (None, None) => (parent.soft_deadline, parent.hard_deadline),
        (Some(s), None) => (Some(s), parent.hard_deadline.filter(|ph| s < *ph)),
        (None, Some(h)) => (parent.soft_deadline.filter(|ps| *ps < h), Some(h)),
    };
    (soft, hard, planned_start_on)
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
    let parent = match payload.parent_task_id {
        Some(pid) => Some(
            tasks::Entity::find_by_id(pid)
                .filter(tasks::Column::ProjectId.eq(ctx.project_id))
                .filter(tasks::Column::DeletedAt.is_null())
                .one(txn)
                .await?
                .ok_or(AppError::NotFound)?,
        ),
        None => None,
    };
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

    let (soft_deadline, hard_deadline, planned_start_on) = schedule_with_parent_defaults(
        payload.soft_deadline.map(Into::into),
        payload.hard_deadline.map(Into::into),
        payload.planned_start_on,
        parent.as_ref(),
    );

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
        soft_deadline: Set(soft_deadline),
        hard_deadline: Set(hard_deadline),
        planned_start_on: Set(planned_start_on),
        require_all_assignees_done: Set(payload.require_all_assignees_done),
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
            completed_at: Set(None),
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
    // 作成直後の担当者はみな未完了なので、全員の完了が必要なら完了ステータスでは作れない
    if status.is_done_state {
        crate::task_completion::ensure_can_enter_done(txn, &model).await?;
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

/// 類似タスク候補の 1 件（docs/dev/design/task-similarity-search.md の「service」）。
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> DateTimeWithTimeZone {
        s.parse().unwrap()
    }

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn parent(soft: Option<&str>, hard: Option<&str>, start: Option<&str>) -> tasks::Model {
        tasks::Model {
            id: Uuid::nil(),
            project_id: Uuid::nil(),
            seq_id: 1,
            title: "parent".into(),
            description: None,
            status_id: Uuid::nil(),
            priority: tasks::TaskPriority::Medium,
            progress_pct: 0,
            parent_task_id: None,
            milestone_id: None,
            sprint_id: None,
            soft_deadline: soft.map(dt),
            hard_deadline: hard.map(dt),
            planned_start_on: start.map(d),
            require_all_assignees_done: false,
            estimated_minutes: None,
            is_archived: false,
            created_by: Uuid::nil(),
            created_at: dt("2026-11-01T00:00:00Z"),
            updated_at: dt("2026-11-01T00:00:00Z"),
            completed_at: None,
            deleted_at: None,
            search_vector: None,
        }
    }

    fn p() -> tasks::Model {
        parent(
            Some("2026-12-10T00:00:00Z"),
            Some("2026-12-20T00:00:00Z"),
            Some("2026-12-01"),
        )
    }

    #[test]
    fn no_parent_returns_arguments_unchanged() {
        let got = schedule_with_parent_defaults(
            Some(dt("2026-12-01T00:00:00Z")),
            None,
            Some(d("2026-12-02")),
            None,
        );
        assert_eq!(
            got,
            (
                Some(dt("2026-12-01T00:00:00Z")),
                None,
                Some(d("2026-12-02"))
            )
        );
        assert_eq!(
            schedule_with_parent_defaults(None, None, None, None),
            (None, None, None)
        );
    }

    #[test]
    fn both_unset_copies_parent() {
        let got = schedule_with_parent_defaults(None, None, None, Some(&p()));
        assert_eq!(
            got,
            (
                Some(dt("2026-12-10T00:00:00Z")),
                Some(dt("2026-12-20T00:00:00Z")),
                Some(d("2026-12-01"))
            )
        );
    }

    #[test]
    fn both_given_wins_over_parent() {
        let got = schedule_with_parent_defaults(
            Some(dt("2026-12-12T00:00:00Z")),
            Some(dt("2026-12-18T00:00:00Z")),
            Some(d("2026-12-03")),
            Some(&p()),
        );
        assert_eq!(
            got,
            (
                Some(dt("2026-12-12T00:00:00Z")),
                Some(dt("2026-12-18T00:00:00Z")),
                Some(d("2026-12-03"))
            )
        );
    }

    #[test]
    fn soft_only_takes_parent_hard_when_consistent() {
        let (s, h, _) =
            schedule_with_parent_defaults(Some(dt("2026-12-15T00:00:00Z")), None, None, Some(&p()));
        assert_eq!(s, Some(dt("2026-12-15T00:00:00Z")));
        assert_eq!(h, Some(dt("2026-12-20T00:00:00Z")));
    }

    #[test]
    fn soft_only_skips_parent_hard_when_not_after_soft() {
        // 親の Hard と同時刻でも Soft < Hard にならないので入れない
        for soft in ["2026-12-20T00:00:00Z", "2026-12-25T00:00:00Z"] {
            let (s, h, start) =
                schedule_with_parent_defaults(Some(dt(soft)), None, None, Some(&p()));
            assert_eq!(s, Some(dt(soft)));
            assert_eq!(h, None);
            assert_eq!(start, Some(d("2026-12-01")));
        }
    }

    #[test]
    fn hard_only_takes_parent_soft_when_consistent() {
        let (s, h, _) =
            schedule_with_parent_defaults(None, Some(dt("2026-12-15T00:00:00Z")), None, Some(&p()));
        assert_eq!(s, Some(dt("2026-12-10T00:00:00Z")));
        assert_eq!(h, Some(dt("2026-12-15T00:00:00Z")));
    }

    #[test]
    fn hard_only_skips_parent_soft_when_not_before_hard() {
        for hard in ["2026-12-10T00:00:00Z", "2026-12-05T00:00:00Z"] {
            let (s, h, _) = schedule_with_parent_defaults(None, Some(dt(hard)), None, Some(&p()));
            assert_eq!(s, None);
            assert_eq!(h, Some(dt(hard)));
        }
    }

    #[test]
    fn parent_with_hard_only_and_child_soft_only() {
        let hard_only = parent(None, Some("2026-12-20T00:00:00Z"), None);
        // 矛盾しなければ親の Hard を引き継ぐ
        let (s, h, start) = schedule_with_parent_defaults(
            Some(dt("2026-12-15T00:00:00Z")),
            None,
            None,
            Some(&hard_only),
        );
        assert_eq!(s, Some(dt("2026-12-15T00:00:00Z")));
        assert_eq!(h, Some(dt("2026-12-20T00:00:00Z")));
        assert_eq!(start, None);
        // Soft が親の Hard 以降なら入れない
        let (s, h, _) = schedule_with_parent_defaults(
            Some(dt("2026-12-25T00:00:00Z")),
            None,
            None,
            Some(&hard_only),
        );
        assert_eq!(s, Some(dt("2026-12-25T00:00:00Z")));
        assert_eq!(h, None);
    }

    #[test]
    fn parent_with_soft_only_and_child_unspecified() {
        let soft_only = parent(Some("2026-12-10T00:00:00Z"), None, Some("2026-12-01"));
        let got = schedule_with_parent_defaults(None, None, None, Some(&soft_only));
        assert_eq!(
            got,
            (
                Some(dt("2026-12-10T00:00:00Z")),
                None,
                Some(d("2026-12-01"))
            )
        );
    }

    #[test]
    fn parent_with_soft_only_and_child_hard_only() {
        let soft_only = parent(Some("2026-12-10T00:00:00Z"), None, None);
        let (s, h, _) = schedule_with_parent_defaults(
            None,
            Some(dt("2026-12-15T00:00:00Z")),
            None,
            Some(&soft_only),
        );
        assert_eq!(s, Some(dt("2026-12-10T00:00:00Z")));
        assert_eq!(h, Some(dt("2026-12-15T00:00:00Z")));
        // Hard が親の Soft 以前なら Soft は入れない
        let (s, h, _) = schedule_with_parent_defaults(
            None,
            Some(dt("2026-12-05T00:00:00Z")),
            None,
            Some(&soft_only),
        );
        assert_eq!(s, None);
        assert_eq!(h, Some(dt("2026-12-05T00:00:00Z")));
    }

    #[test]
    fn parent_without_schedule_leaves_child_empty() {
        let got = schedule_with_parent_defaults(None, None, None, Some(&parent(None, None, None)));
        assert_eq!(got, (None, None, None));
    }
}
