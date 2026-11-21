//! 通知の一覧・既読と、プロジェクトごとの通知設定。通知の API はテナントをパスに持たない。

use axum::http::request::Parts;
use payload::task_notifications::{
    NotificationSettingsResponse, UpdateNotificationSettingsRequest,
};
use rmcp::{
    ErrorData,
    handler::server::{common::Extension, wrapper::Parameters},
    model::CallToolResult,
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use super::super::inner::{InnerApi, ToolFailure};
use super::super::resolve;
use super::{TaskMcp, blank_as_none, finish, parse_uuid, to_body};

#[derive(Deserialize, JsonSchema)]
struct ListNotificationsArgs {
    /// List only the unread notifications
    unread: Option<bool>,
    /// Notifications per page (1-100, default 50)
    limit: Option<u64>,
    /// next_cursor of the previous result, for the next page
    #[serde(default, deserialize_with = "blank_as_none")]
    cursor: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct NotificationArgs {
    /// Notification UUID
    notification_id: String,
}

#[derive(Deserialize, JsonSchema)]
struct NotificationSettingsArgs {
    /// Project key or UUID
    project: String,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant and project is a key
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct UpdateNotificationSettingsArgs {
    /// Project key or UUID
    project: String,
    /// Event types notified in the app, e.g. assigned or commented. Replaces the current list
    in_app_events: Option<Vec<String>>,
    /// Event types notified by email. Replaces the current list
    email_events: Option<Vec<String>>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant and project is a key
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

/// 通知設定のプロジェクト ID。UUID ならそのまま使い、テナントは解決しない
/// （CLI の `crates/cli/src/commands/notifications.rs` と同じ）。キーのときだけテナントから引く
async fn settings_project_id(
    api: &InnerApi,
    tenant: Option<&str>,
    project: &str,
) -> Result<String, ToolFailure> {
    if let Ok(uuid) = Uuid::parse_str(project) {
        return Ok(uuid.to_string());
    }
    let tenant_id = resolve::tenant_id(api, tenant).await?;
    resolve::project_id(api, &tenant_id, project).await
}

fn settings_segments(project_id: &str) -> [&str; 5] {
    ["v1", "users", "me", "notification-settings", project_id]
}

impl TaskMcp {
    async fn run_list_notifications(
        &self,
        parts: &Parts,
        args: ListNotificationsArgs,
    ) -> Result<Value, ToolFailure> {
        // 省略した項目はクエリに付けない（API の既定に任せる）
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(unread) = args.unread {
            query.push(("unread", unread.to_string()));
        }
        if let Some(limit) = args.limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(cursor) = args.cursor {
            query.push(("cursor", cursor));
        }
        self.inner(parts)?
            .get(&["v1", "users", "me", "notifications"], &query)
            .await
    }

    async fn run_update_notification_settings(
        &self,
        parts: &Parts,
        args: UpdateNotificationSettingsArgs,
    ) -> Result<Value, ToolFailure> {
        if args.in_app_events.is_none() && args.email_events.is_none() {
            return Err(ToolFailure::new("Pass in_app_events, email_events or both"));
        }
        let api = self.inner(parts)?;
        let project_id = settings_project_id(&api, args.tenant.as_deref(), &args.project).await?;
        let segments = settings_segments(&project_id);
        // PUT は両方の一覧を置き換えるので、渡さなかった側は今の値を送り直す
        let current: NotificationSettingsResponse =
            serde_json::from_value(api.get(&segments, &[]).await?).map_err(|error| {
                ToolFailure::new(format!(
                    "Could not read the current notification settings: {error}"
                ))
            })?;
        let body = to_body(&UpdateNotificationSettingsRequest {
            email_events: args.email_events.unwrap_or(current.email_events),
            in_app_events: args.in_app_events.unwrap_or(current.in_app_events),
        })?;
        api.put(&segments, &body).await
    }
}

#[tool_router(router = notifications_router, vis = "pub(super)")]
impl TaskMcp {
    #[tool(
        description = "List the notifications of the token owner, newest first. Pass next_cursor of the previous result as cursor for the next page. Needs the read:task scope.",
        annotations(read_only_hint = true)
    )]
    async fn list_notifications(
        &self,
        Parameters(args): Parameters<ListNotificationsArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_list_notifications(&parts, args).await)
    }

    #[tool(description = "Mark one notification as read. Needs the write:task scope.")]
    async fn mark_notification_read(
        &self,
        Parameters(args): Parameters<NotificationArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(
            async {
                let id = parse_uuid("notification_id", &args.notification_id)?.to_string();
                self.inner(&parts)?
                    .patch(&["v1", "users", "me", "notifications", &id, "read"])
                    .await
            }
            .await,
        )
    }

    #[tool(
        description = "Mark every notification the token can see as read. Cannot be undone: there is no way to mark them unread again. Needs the write:task scope."
    )]
    async fn mark_all_notifications_read(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(
            async {
                self.inner(&parts)?
                    .patch(&["v1", "users", "me", "notifications", "read-all"])
                    .await
            }
            .await,
        )
    }

    #[tool(
        description = "Show the token owner's notification settings of a project. Needs the read:task scope.",
        annotations(read_only_hint = true)
    )]
    async fn get_notification_settings(
        &self,
        Parameters(args): Parameters<NotificationSettingsArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(
            async {
                let api = self.inner(&parts)?;
                let project_id =
                    settings_project_id(&api, args.tenant.as_deref(), &args.project).await?;
                api.get(&settings_segments(&project_id), &[]).await
            }
            .await,
        )
    }

    #[tool(
        description = "Replace the in-app or email event types of the token owner's notification settings of a project. A list that is not passed keeps its current value; an empty list turns every event off. Needs the write:task scope."
    )]
    async fn update_notification_settings(
        &self,
        Parameters(args): Parameters<UpdateNotificationSettingsArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_update_notification_settings(&parts, args).await)
    }
}
