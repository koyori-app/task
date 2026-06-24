use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::entities::audit_logs;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditLogQuery {
    /// アクション名の前方一致フィルタ
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub actor_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    /// 期間開始（ISO 8601）
    pub from: Option<String>,
    /// 期間終了（ISO 8601）
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u64,
    pub cursor: Option<String>,
}

fn default_limit() -> u64 {
    50
}

#[derive(Serialize, ToSchema)]
pub struct AuditLogListResponse {
    pub logs: Vec<audit_logs::Model>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
