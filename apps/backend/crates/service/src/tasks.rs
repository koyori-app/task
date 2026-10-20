//! タスクの共通ロジック。

use sea_orm::sea_query::LockType;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DbBackend, DbErr, EntityTrait,
    FromQueryResult, QuerySelect, Statement, prelude::Uuid,
};

use entity::project_task_counters;

/// プロジェクト内の次の連番を採番する。
/// 行ロックで直列化するため、必ずトランザクション上で呼ぶこと。
pub async fn next_seq_id<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<i32, sea_orm::DbErr> {
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
