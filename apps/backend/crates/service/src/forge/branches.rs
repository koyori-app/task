//! ブランチ名の `KEY-N` によるタスクとのリンク（docs/features/tasks/9.github-tasks.md §5 の
//! `BranchCreated` / `BranchDeleted`）。アクティビティは積まない。

use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, Statement, prelude::Uuid};

use entity::projects;

use super::events::ForgeRepo;
use super::task_refs;

/// `project_id` は webhook を受けた連携のプロジェクト。`KEY-N` はそのテナント内で解決し、
/// リンク行はリンク先タスクのプロジェクトに置く（コミットと同じ規則）。
pub async fn apply_branch_created(
    db: &DatabaseConnection,
    project_id: Uuid,
    repo: &ForgeRepo,
    branch: &str,
    html_url: &str,
) -> Result<(), anyhow::Error> {
    let Some(project) = projects::Entity::find_by_id(project_id).one(db).await? else {
        return Ok(());
    };
    let targets = task_refs::resolve(db, project.tenant_id, &task_refs::extract(branch)).await?;
    for target in targets {
        db.execute_raw(Statement::from_sql_and_values(
            db.get_database_backend(),
            "INSERT INTO forge_branch_links (id, project_id, task_id, host, host_url, repo_owner,
                                             repo_name, branch, html_url, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
             ON CONFLICT (task_id, host, host_url, repo_owner, repo_name, branch) DO NOTHING",
            [
                Uuid::new_v4().into(),
                target.project_id.into(),
                target.task_id.into(),
                repo.host.clone().into(),
                repo.host_url.clone().into(),
                repo.repo_owner.clone().into(),
                repo.repo_name.clone().into(),
                branch.into(),
                html_url.into(),
            ],
        ))
        .await?;
    }
    Ok(())
}

/// 消すのは webhook を受けた連携のテナントのリンクだけ（他テナントの行は、そのテナントの連携に届く同じイベントで消える）。
pub async fn apply_branch_deleted(
    db: &DatabaseConnection,
    project_id: Uuid,
    repo: &ForgeRepo,
    branch: &str,
) -> Result<(), anyhow::Error> {
    db.execute_raw(Statement::from_sql_and_values(
        db.get_database_backend(),
        "DELETE FROM forge_branch_links
         WHERE host = $2 AND host_url = $3 AND repo_owner = $4 AND repo_name = $5 AND branch = $6
           AND project_id IN (SELECT id FROM projects
                              WHERE tenant_id = (SELECT tenant_id FROM projects WHERE id = $1))",
        [
            project_id.into(),
            repo.host.clone().into(),
            repo.host_url.clone().into(),
            repo.repo_owner.clone().into(),
            repo.repo_name.clone().into(),
            branch.into(),
        ],
    ))
    .await?;
    Ok(())
}
