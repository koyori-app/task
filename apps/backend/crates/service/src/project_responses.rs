use sea_orm::{ConnectionTrait, prelude::Uuid};

use crate::error::AppError;
use crate::forge::repository;
use entity::projects;
use payload::projects::ProjectResponse;

/// projects::Model の集合から、連携先のリポジトリを埋めた ProjectResponse を組み立てる。
/// 入力の順を保つ。連携先は 1 クエリで引く。
///
/// 認可と絞り込みを済ませ、返すと決まったモデルだけを渡す
pub async fn build_project_responses<C: ConnectionTrait>(
    db: &C,
    models: Vec<projects::Model>,
) -> Result<Vec<ProjectResponse>, AppError> {
    let project_ids: Vec<Uuid> = models.iter().map(|project| project.id).collect();
    let mut repositories = repository::current_for_projects(db, &project_ids).await?;
    Ok(models
        .into_iter()
        .map(|model| {
            let repo = repositories.remove(&model.id).map(repository::to_response);
            ProjectResponse::from_parts(model, repo)
        })
        .collect())
}

pub async fn build_project_response<C: ConnectionTrait>(
    db: &C,
    model: projects::Model,
) -> Result<ProjectResponse, AppError> {
    let repo = repository::current(db, model.id)
        .await?
        .map(repository::to_response);
    Ok(ProjectResponse::from_parts(model, repo))
}
