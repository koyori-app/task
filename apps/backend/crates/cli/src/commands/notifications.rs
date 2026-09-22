//! 通知のコマンド。仕様は `docs/features/tasks/5.notifications.md` §4。
//!
//! 種別の綴りは `common::notifications::KNOWN_EVENT_TYPES` をそのまま使う。
//! CLI 側に写しを置くと、サーバーが受け付ける集合と黙って食い違う。

use common::notifications::KNOWN_EVENT_TYPES;
use payload::task_notifications::{
    NotificationItem, NotificationListResponse, NotificationSettingsResponse,
    UpdateNotificationSettingsRequest,
};

use crate::Context;
use crate::cli::NotificationsCommand;
use crate::error::{CliError, Result};
use crate::output::{OutputOptions, print};
use crate::resolve::resolve_project;

pub async fn run(
    context: &Context,
    command: NotificationsCommand,
    output: OutputOptions,
) -> Result<i32> {
    match command {
        NotificationsCommand::List { unread, limit } => {
            let api = &context.connect()?;
            let mut query = vec![("limit", limit.to_string())];
            if unread {
                query.push(("unread", "true".into()));
            }
            let listing: NotificationListResponse = api
                .get(&["v1", "users", "me", "notifications"], &query)
                .await?;
            print(&listing, output);
        }
        NotificationsCommand::Read { id } => {
            let api = &context.connect()?;
            let item: NotificationItem = api
                .patch(&["v1", "users", "me", "notifications", &id, "read"], &())
                .await?;
            print(&item, output);
        }
        NotificationsCommand::ReadAll => {
            let api = &context.connect()?;
            api.patch_no_content(&["v1", "users", "me", "notifications", "read-all"])
                .await?;
        }
        NotificationsCommand::Settings {
            project,
            in_app,
            email,
        } => {
            // 綴り違いは送る前に弾く。サーバーの検証に任せると、どの値が未知なのかが
            // 応答から読み取りにくい（`review` の絞り込みと同じ流儀）
            let in_app = parse_event_types(in_app.as_deref(), "--in-app")?;
            let email = parse_event_types(email.as_deref(), "--email")?;
            let api = &context.connect()?;
            let project_id = resolve_project(api, &project).await?.id.to_string();
            let path = [
                "v1",
                "users",
                "me",
                "notification-settings",
                project_id.as_str(),
            ];
            let current: NotificationSettingsResponse = api.get(&path, &[]).await?;
            // 指定の無い側は現在値を保つ。片方だけ渡したときに、もう片方を空にしない
            let settings = if in_app.is_none() && email.is_none() {
                current
            } else {
                api.put(
                    &path,
                    &UpdateNotificationSettingsRequest {
                        email_events: email.unwrap_or(current.email_events),
                        in_app_events: in_app.unwrap_or(current.in_app_events),
                    },
                )
                .await?
            };
            print(&settings, output);
        }
    }
    Ok(0)
}

/// カンマ区切りの種別一覧。空文字は「全部外す」を意味するので空の一覧として通す。
fn parse_event_types(value: Option<&str>, flag: &str) -> Result<Option<Vec<String>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let mut events = Vec::new();
    for item in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if !KNOWN_EVENT_TYPES.contains(&item) {
            return Err(CliError::validation(format!(
                "Unknown event type for {flag}: {item} (accepted values: {})",
                KNOWN_EVENT_TYPES.join(", ")
            )));
        }
        events.push(item.to_string());
    }
    Ok(Some(events))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_given_order_and_accepts_an_empty_list() {
        assert_eq!(
            parse_event_types(Some("mentioned, assigned"), "--in-app").unwrap(),
            Some(vec!["mentioned".to_string(), "assigned".to_string()])
        );
        assert_eq!(
            parse_event_types(Some(""), "--email").unwrap(),
            Some(vec![])
        );
        assert_eq!(parse_event_types(None, "--email").unwrap(), None);
    }

    #[test]
    fn rejects_an_unknown_event_type_with_the_candidates() {
        let err = parse_event_types(Some("assigned,mentiond"), "--in-app").unwrap_err();
        assert_eq!(err.exit_code, 2);
        assert!(err.message.contains("mentiond"), "{}", err.message);
        assert!(err.message.contains("mentioned"), "{}", err.message);
    }
}
