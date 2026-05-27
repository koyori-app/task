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
        ]
    }
}
