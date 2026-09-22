use sea_orm::prelude::{DateTimeWithTimeZone, Uuid};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

use entity::{webhook_deliveries, webhooks};

/// Webhook。secret は返さない（作成時の応答だけ [`CreateWebhookResponse`] で平文を返す）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WebhookResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub project_id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    /// `json` | `discord`
    pub format: String,
    pub is_active: bool,
    /// 打ち止めになった配信の連続回数。5 で `is_active` が false になる
    pub failure_streak: i16,
    #[schema(value_type = String, format = "uuid")]
    pub created_by: Uuid,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeWithTimeZone,
}

impl From<webhooks::Model> for WebhookResponse {
    fn from(model: webhooks::Model) -> Self {
        Self {
            id: model.id,
            project_id: model.project_id,
            url: model.url,
            events: model.events,
            format: model.format,
            is_active: model.is_active,
            failure_streak: model.failure_streak,
            created_by: model.created_by,
            created_at: model.created_at,
        }
    }
}

/// 作成時の応答。secret を平文で返すのはこのときだけ。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateWebhookResponse {
    #[serde(flatten)]
    pub webhook: WebhookResponse,
    pub secret: String,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct CreateWebhookRequest {
    /// 送信先。https 必須。private / localhost 宛ては 400
    #[validate(length(min = 1, max = 2048))]
    pub url: String,
    /// 署名用のシークレット（16 文字以上）
    pub secret: String,
    /// 購読するイベント（1 つ以上）
    pub events: Vec<String>,
    /// `json`（既定）| `discord`
    pub format: Option<String>,
}

/// 指定したものだけ変える。`is_active` を true に戻すと `failure_streak` も 0 に戻る。
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct UpdateWebhookRequest {
    #[validate(length(min = 1, max = 2048))]
    pub url: Option<String>,
    pub secret: Option<String>,
    pub events: Option<Vec<String>>,
    pub format: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WebhookDeliveryResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub webhook_id: Uuid,
    pub event: String,
    #[schema(value_type = Object)]
    pub payload: serde_json::Value,
    #[schema(nullable)]
    pub status_code: Option<i32>,
    /// 試行した回数（最大 5）
    pub attempt: i16,
    /// 次に試す時刻。null なら完了（成功または打ち止め）
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub next_attempt_at: Option<DateTimeWithTimeZone>,
    #[schema(nullable)]
    pub last_error: Option<String>,
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub delivered_at: Option<DateTimeWithTimeZone>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeWithTimeZone,
}

impl From<webhook_deliveries::Model> for WebhookDeliveryResponse {
    fn from(model: webhook_deliveries::Model) -> Self {
        Self {
            id: model.id,
            webhook_id: model.webhook_id,
            event: model.event,
            payload: model.payload,
            status_code: model.status_code,
            attempt: model.attempt,
            next_attempt_at: model.next_attempt_at,
            last_error: model.last_error,
            delivered_at: model.delivered_at,
            created_at: model.created_at,
        }
    }
}

#[derive(Debug, Deserialize, Validate, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeliveryListQuery {
    /// 返す件数（1〜100、既定 50）。新しい順
    #[validate(range(min = 1, max = 100))]
    pub limit: Option<u64>,
}
