// 統合テストは単一バイナリに集約する（リンク回数と tests/common の重複コンパイルを減らす）。
// 新しいテストファイルはこのディレクトリに置き、ここに mod 宣言を追加する。
#[path = "../common/mod.rs"]
mod common;

mod admin_project_scope_integration;
mod admin_tenants_integration;
mod admin_users_integration;
mod auth_2fa_integration;
mod custom_fields_integration;
mod dashboard_integration;
mod drive_file_content_integration;
mod drive_folder_boundary_integration;
mod drive_project_id_backfill_integration;
mod drive_public_share_integration;
mod drive_upload_acl_integration;
mod drive_usage_integration;
mod forge_commit_links_integration;
mod github_http_integration;
mod github_integration;
mod github_issue_sync_integration;
mod my_tasks_integration;
mod notification_email_integration;
mod oauth_integration;
mod password_reset_integration;
mod pat_revoke_all_integration;
mod pat_tenant_membership_integration;
mod personal_token_identity_integration;
mod personal_tokens_create_auth_integration;
mod personal_tokens_list_integration;
mod profile_update_integration;
mod project_guest_access_integration;
mod project_members_integration;
mod projects_integration;
mod register_integration;
mod review_notifications_integration;
mod review_summary_integration;
mod reviews_integration;
mod session_extractors_integration;
mod sprints_integration;
mod statuses_integration;
mod task_activities_integration;
mod task_assignable_users_integration;
mod task_atomic_update_integration;
mod task_comments_integration;
mod task_extensions_integration;
mod task_labels_integration;
mod task_list_integration;
mod task_notifications_integration;
mod task_update_concurrency_integration;
mod tenant_member_role_integration;
mod tenant_members_integration;
mod tenants_integration;
mod time_tracking_integration;
mod webauthn_integration;

#[test]
fn every_integration_file_is_declared() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/integration");
    let declared = include_str!("main.rs");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read integration tests") {
        let path = entry.expect("read integration test entry").path();
        if !path.is_file() || path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let name = path.file_stem().unwrap().to_str().expect("UTF-8 test name");
        if name != "main"
            && !declared
                .lines()
                .any(|line| line.trim() == format!("mod {name};"))
        {
            missing.push(name.to_owned());
        }
    }
    missing.sort();
    assert!(missing.is_empty(), "main.rs に mod 宣言が無い: {missing:?}");
}
