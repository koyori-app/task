use common::error::AppError;
use payload::task_query::*;
use sea_orm::{ConnectionTrait, DbBackend, FromQueryResult, Statement};
use uuid::Uuid;
#[derive(FromQueryResult)]
struct CountRow {
    id: Uuid,
    count: i64,
}
#[derive(FromQueryResult)]
struct PriorityRow {
    priority: entity::tasks::TaskPriority,
    count: i64,
}
#[derive(FromQueryResult)]
struct TotalRow {
    count: i64,
}
pub async fn facets<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<TaskFacetsResponse, AppError> {
    let predicate = "t.project_id = $1 AND t.deleted_at IS NULL AND t.is_archived = false";
    let mut groups = Vec::new();
    for (key, join) in [
        ("t.status_id", ""),
        ("x.label_id", "JOIN task_labels x ON x.task_id = t.id"),
        ("x.user_id", "JOIN task_assignees x ON x.task_id = t.id"),
        ("t.created_by", ""),
    ] {
        let sql = format!(
            "SELECT {key} AS id, COUNT(DISTINCT t.id)::bigint AS count FROM tasks t {join} WHERE {predicate} GROUP BY {key} ORDER BY {key}"
        );
        groups.push(
            CountRow::find_by_statement(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                [project_id.into()],
            ))
            .all(db)
            .await?
            .into_iter()
            .map(|r| FacetCount {
                id: r.id,
                count: r.count as u64,
            })
            .collect(),
        );
    }
    let priorities = PriorityRow::find_by_statement(Statement::from_sql_and_values(DbBackend::Postgres, format!("SELECT t.priority, COUNT(*)::bigint AS count FROM tasks t WHERE {predicate} GROUP BY t.priority ORDER BY t.priority"), [project_id.into()])).all(db).await?.into_iter().map(|r| PriorityFacetCount {priority:r.priority.into(),count:r.count as u64}).collect();
    let mut missing = Vec::new();
    for table in ["task_assignees", "task_labels"] {
        let row = TotalRow::find_by_statement(Statement::from_sql_and_values(DbBackend::Postgres, format!("SELECT COUNT(*)::bigint AS count FROM tasks t WHERE {predicate} AND NOT EXISTS (SELECT 1 FROM {table} x WHERE x.task_id = t.id)"), [project_id.into()])).one(db).await?.ok_or_else(|| anyhow::anyhow!("count query returned no row"))?;
        missing.push(row.count as u64);
    }
    let mut groups = groups.into_iter();
    Ok(TaskFacetsResponse {
        statuses: groups.next().unwrap(),
        labels: groups.next().unwrap(),
        assignees: groups.next().unwrap(),
        authors: groups.next().unwrap(),
        priorities,
        unassigned: missing[0],
        unlabeled: missing[1],
    })
}
