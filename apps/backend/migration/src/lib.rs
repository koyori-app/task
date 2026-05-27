pub use sea_orm_migration::prelude::*;

mod m20260524080400_create_project_members;
mod m20260524080500_add_icon_to_projects;
mod m20260524100000_add_role_check;
mod m20260524120000_add_pat_tenant_binding;
mod m20260525010000_add_drive_quota_to_tenants;
mod m20260525020000_create_drive_folders;
mod m20260525030000_create_drive_files;
mod m20260525040000_create_drive_folder_shares;

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
        ]
    }
}
