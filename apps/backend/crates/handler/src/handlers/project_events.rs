use std::{convert::Infallible, time::Duration};

use axum::{
    Extension,
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures::{Stream, StreamExt};
use tokio::sync::watch;
use uuid::Uuid;

use crate::{AppState, error::AppError, extractors::AuthUser, openapi::CrudErrors};

/// 本番の `/api` を中継する Bun は無通信 10 秒で接続を切るので、それより短くする。
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(5);

#[utoipa::path(
    get,
    path = "/",
    tag = "Tasks",
    summary = "プロジェクトのタスク変更を購読（SSE）",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, description = "タスクが変わるたびに `event: tasks` を流す", content_type = "text/event-stream", body = String),
        CrudErrors,
    )
)]
pub async fn stream_project_events(
    State(state): State<AppState>,
    Extension(mut shutdown): Extension<watch::Receiver<bool>>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    auth.require_scope(entity::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;

    // shortcut: 認可は接続時だけで、外されたメンバーにも切断まで「変わった」の合図は届く。内容を載せるなら接続中も検査する
    let messages = service::project_events::subscribe(&state.redis_client, project_id).await?;
    let stream = messages
        .map(|data| Ok(Event::default().event("tasks").data(data)))
        // 終わらない応答が残ると graceful shutdown が完了せず、ワーカーの停止まで進まない
        .take_until(async move {
            let _ = shutdown.wait_for(|stop| *stop).await;
        });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(KEEP_ALIVE_INTERVAL)))
}
