pub mod admin_audit;
pub mod admin_audit_logs;
pub mod admin_settings;
pub mod admin_tenants;
pub mod admin_users;
pub mod auth;
pub mod auth_2fa;
pub mod avatars;
pub mod bots;
pub mod custom_fields;
pub mod dashboard;
pub mod desktop_auth;
pub mod drive_files;
pub mod drive_folders;
pub mod favicons;
pub mod forge;
pub mod forge_merge_effects;
pub mod github;
pub mod labels;
pub mod milestones;
pub mod my_tasks;
pub mod oauth;
pub mod passkeys;
pub mod password_reset;
pub mod personal_tokens;
pub mod project_events;
pub mod project_members;
pub mod projects;
pub mod reactions;
pub mod reviews;
pub mod schedule;
pub mod sprints;
pub mod statuses;
pub mod task_comments;
pub mod task_extensions;
pub mod task_notifications;
pub mod tasks;
pub mod tenant_invitations;
pub mod tenant_members;
pub mod tenants;
pub mod time_tracking;
pub mod webhooks;

/// レート制限の接続元キー。プロキシが付ける値を使う（監査ログと同じ取り方）。
pub(crate) fn client_key(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
