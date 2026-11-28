//! プロジェクトの現在の連携先（GitHub だけ）。

use std::collections::HashMap;

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, prelude::Uuid};

use crate::error::AppError;
use crate::forge::events::ForgeRepo;
use crate::github::webhook_events::HOST_URL;
use entity::github_integrations;
use payload::forge::TaskForgeRepository;

fn from_integration(integration: github_integrations::Model) -> ForgeRepo {
    ForgeRepo {
        host: "github".into(),
        host_url: HOST_URL.into(),
        repo_owner: integration.repo_owner,
        repo_name: integration.repo_name,
    }
}

/// プロジェクトの現在の連携先。連携が無ければ `None`
pub async fn current<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<Option<ForgeRepo>, AppError> {
    Ok(github_integrations::Entity::find()
        .filter(github_integrations::Column::ProjectId.eq(project_id))
        .one(db)
        .await?
        .map(from_integration))
}

/// 複数のプロジェクトの連携先を 1 回で引く。連携の無いプロジェクトは結果に入らない
pub async fn current_for_projects<C: ConnectionTrait>(
    db: &C,
    project_ids: &[Uuid],
) -> Result<HashMap<Uuid, ForgeRepo>, AppError> {
    if project_ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(github_integrations::Entity::find()
        .filter(github_integrations::Column::ProjectId.is_in(project_ids.iter().copied()))
        .all(db)
        .await?
        .into_iter()
        .map(|integration| (integration.project_id, from_integration(integration)))
        .collect())
}

/// 応答の形。トークン類は載せない
pub fn to_response(repo: ForgeRepo) -> TaskForgeRepository {
    TaskForgeRepository {
        html_url: format!("{}/{}/{}", repo.host_url, repo.repo_owner, repo.repo_name),
        host: repo.host,
        host_url: repo.host_url,
        repo_owner: repo.repo_owner,
        repo_name: repo.repo_name,
    }
}
