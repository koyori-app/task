pub use sea_orm_migration::prelude::*;

mod m20260524080400_create_project_members;
mod m20260524080500_add_icon_to_projects;
mod m20260524100000_add_role_check;
mod m20260524120000_add_pat_tenant_binding;
mod m20260525010000_add_drive_quota_to_tenants;
mod m20260525020000_create_drive_folders;
mod m20260525030000_create_drive_files;
mod m20260525040000_create_drive_folder_shares;
mod m20260527010000_add_key_to_projects;
mod m20260527020000_create_project_task_counters;
mod m20260527030000_create_project_statuses;
mod m20260527040000_create_milestones;
mod m20260527050000_create_tasks;
mod m20260527060000_create_task_assignees;
mod m20260527070000_create_task_relations;
mod m20260527080000_add_project_id_to_labels;
mod m20260527090000_create_task_labels;
mod m20260527100000_add_admin_fields_to_users;
mod m20260527110000_create_audit_logs;
mod m20260527120000_create_system_settings;
mod m20260528190000_add_drive_system_max_quota_mb;
mod m20260529100000_oauth_connections;
mod m20260601160000_add_sessions_revoked_at_to_users;
mod m20260603_add_unique_task_assignees;
mod m20260604_create_sprints;
mod m20260604_create_time_tracking;
mod m20260607_add_sprints_active_unique_index;
mod m20260607_add_tasks_completed_at;
mod m20260607_create_task_comments_and_activities;
mod m20260609_create_custom_fields;
mod m20260529010000_create_passkeys;
mod m20260529100000_auth_2fa;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260524080400_create_project_members::Migration),
            Box::new(m20260524080500_add_icon_to_projects::Migration),
            Box::new(m20260524100000_add_role_check::Migration),
            Box::new(m20260524120000_add_pat_tenant_binding::Migration),
            Box::new(m20260525010000_add_drive_quota_to_tenants::Migration),
            Box::new(m20260525020000_create_drive_folders::Migration),
            Box::new(m20260525030000_create_drive_files::Migration),
            Box::new(m20260525040000_create_drive_folder_shares::Migration),
            Box::new(m20260527010000_add_key_to_projects::Migration),
            Box::new(m20260527020000_create_project_task_counters::Migration),
            Box::new(m20260527030000_create_project_statuses::Migration),
            Box::new(m20260527040000_create_milestones::Migration),
            Box::new(m20260527050000_create_tasks::Migration),
            Box::new(m20260527060000_create_task_assignees::Migration),
            Box::new(m20260527070000_create_task_relations::Migration),
            Box::new(m20260527080000_add_project_id_to_labels::Migration),
            Box::new(m20260527090000_create_task_labels::Migration),
            Box::new(m20260527100000_add_admin_fields_to_users::Migration),
            Box::new(m20260527110000_create_audit_logs::Migration),
            Box::new(m20260527120000_create_system_settings::Migration),
            Box::new(m20260528190000_add_drive_system_max_quota_mb::Migration),
            Box::new(m20260529100000_oauth_connections::Migration),
            Box::new(m20260601160000_add_sessions_revoked_at_to_users::Migration),
            Box::new(m20260603_add_unique_task_assignees::Migration),
            Box::new(m20260604_create_sprints::Migration),
            Box::new(m20260604_create_time_tracking::Migration),
            Box::new(m20260607_add_sprints_active_unique_index::Migration),
            Box::new(m20260607_add_tasks_completed_at::Migration),
            Box::new(m20260607_create_task_comments_and_activities::Migration),
            Box::new(m20260609_create_custom_fields::Migration),
            Box::new(m20260529010000_create_passkeys::Migration),
            Box::new(m20260529100000_auth_2fa::Migration),
        ]
    }
}
