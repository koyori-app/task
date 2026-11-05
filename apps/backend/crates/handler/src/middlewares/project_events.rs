use axum::{
    Extension,
    extract::{Path, Request},
    http::Method,
    middleware::Next,
    response::Response,
};
use common::cache::redis::RedisConnection;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ProjectPath {
    project_id: Uuid,
}

/// タスク配下の更新が成功したら、そのプロジェクトの購読者へ合図を送る。
/// 保存は済んでいるので、発行の失敗で応答を失敗させない。
pub async fn publish_tasks_changed(
    Extension(redis): Extension<RedisConnection>,
    Path(ProjectPath { project_id }): Path<ProjectPath>,
    req: Request,
    next: Next,
) -> Response {
    let mutates = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let res = next.run(req).await;
    if mutates
        && res.status().is_success()
        && let Err(e) = service::project_events::publish_tasks_changed(&redis, project_id).await
    {
        tracing::warn!(%project_id, error = %e, "failed to publish project event");
    }
    res
}
