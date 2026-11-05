//! プロジェクトのイベント（Redis pub/sub）。規則は docs/dev/design/project-events.md。

use std::future::ready;

use common::cache::redis::RedisConnection;
use futures::{Stream, StreamExt};
use uuid::Uuid;

/// 「タスクが変わった」の合図。内容は載せず、受け手が権限のある API で取り直す。
const TASKS_CHANGED: &str = r#"{"kind":"tasks_changed"}"#;

fn channel(project_id: Uuid) -> String {
    format!("project-events:{project_id}")
}

pub async fn publish_tasks_changed(
    redis: &RedisConnection,
    project_id: Uuid,
) -> anyhow::Result<()> {
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let _: i64 = redis::cmd("PUBLISH")
        .arg(channel(project_id))
        .arg(TASKS_CHANGED)
        .query_async(&mut conn)
        .await?;
    Ok(())
}

/// 購読を済ませてから返す。返った後の発行は取りこぼさない。
/// Redis との接続が切れるとストリームは終わる（再購読はしない）。
pub async fn subscribe(
    redis: &RedisConnection,
    project_id: Uuid,
) -> anyhow::Result<impl Stream<Item = String> + Send + use<>> {
    let mut pubsub = redis.conn.factory().get_async_pubsub().await?;
    pubsub.subscribe(channel(project_id)).await?;
    Ok(pubsub
        .into_on_message()
        .filter_map(|msg| ready(msg.get_payload::<String>().ok())))
}
