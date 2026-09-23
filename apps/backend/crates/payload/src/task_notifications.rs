use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use common::notifications::KNOWN_EVENT_TYPES;

fn validate_known_event_types(events: &Vec<String>) -> Result<(), validator::ValidationError> {
    for e in events {
        if !KNOWN_EVENT_TYPES.contains(&e.as_str()) {
            return Err(validator::ValidationError::new("unknown_event_type"));
        }
    }
    Ok(())
}

#[derive(Serialize, ToSchema)]
pub struct WatcherUser {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct WatcherListResponse {
    pub watchers: Vec<WatcherUser>,
}

#[derive(Serialize, ToSchema)]
pub struct NotificationTaskSummary {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub seq_id: i32,
    pub title: String,
}

/// 通知の遷移先。`type` で種類を見分ける。
///
/// テナント・プロジェクトは通知行の `project` で返すので、ここには重複して持たない。
#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationTarget {
    Task {
        #[schema(value_type = String, format = "uuid")]
        task_id: Uuid,
    },
}

/// 通知が属するプロジェクト。遷移先の URL を組み立てるのに使う。
#[derive(Serialize, ToSchema)]
pub struct NotificationProjectSummary {
    #[schema(value_type = String, format = "uuid")]
    pub tenant_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub key: String,
}

#[derive(Serialize, ToSchema)]
pub struct NotificationItem {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub notification_type: String,
    pub project: NotificationProjectSummary,
    #[schema(nullable)]
    pub task: Option<NotificationTaskSummary>,
    #[schema(value_type = serde_json::Value)]
    pub payload: serde_json::Value,
    pub target: NotificationTarget,
    /// この行を指すカーソル。catch-up では最後に受け取った（最新の）行のこの値を
    /// 控えておき、次回 `after` に渡す
    pub cursor: String,
    #[schema(nullable, value_type = Option<String>, format = "date-time")]
    pub read_at: Option<chrono::DateTime<chrono::Utc>>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct NotificationListResponse {
    /// 未読の総数。`unread` / `kind` / `cursor` / `after` の指定に関係なく、
    /// 見える通知全体で数える
    pub unread_count: u64,
    /// 続きを引く鍵。`null` なら取り切っている。
    ///
    /// - `cursor` 指定時・無指定時: ページ内の最も古い行。次の `cursor` に渡す
    /// - `after` 指定時: ページ内の最も新しい行。次の `after` に渡す
    #[schema(required, nullable)]
    pub next_cursor: Option<String>,
    pub notifications: Vec<NotificationItem>,
}

/// 種別の分類。`review.` 接頭辞の有無で分ける。
#[derive(Deserialize, ToSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationKind {
    Task,
    Review,
}

/// 通知一覧のクエリ。
///
/// 並びは `created_at DESC, id DESC` のみ（未読優先はしない。既読化で順位が
/// 変わるとカーソルが成り立たない）。ただし `after` 指定時だけは古い順
/// （`created_at ASC, id ASC`）で返す。新しい側から `limit` 件を切ると、
/// `after` との間の行を引く手段が無くなるため。
#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListNotificationsQuery {
    /// 未読のみ
    pub unread: Option<bool>,
    /// `task` / `review`
    #[param(inline)]
    pub kind: Option<NotificationKind>,
    /// 既定 50、上限 100
    pub limit: Option<u64>,
    /// このカーソルより古い行（通常のページ送り）。前のページの `next_cursor` を渡す
    pub cursor: Option<String>,
    /// このカーソルより新しい行（catch-up）。最後に受け取った行の `cursor`、
    /// または前の `after` 呼び出しの `next_cursor` を渡す。`cursor` と同時指定は 400
    pub after: Option<String>,
}

/// 一覧の既定件数と上限。
pub const DEFAULT_NOTIFICATIONS_LIMIT: u64 = 50;
pub const MAX_NOTIFICATIONS_LIMIT: u64 = 100;

#[derive(Serialize, ToSchema)]
pub struct NotificationSettingsResponse {
    pub email_events: Vec<String>,
    pub in_app_events: Vec<String>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateNotificationSettingsRequest {
    #[validate(custom(function = "validate_known_event_types"))]
    pub email_events: Vec<String>,
    #[validate(custom(function = "validate_known_event_types"))]
    pub in_app_events: Vec<String>,
}
