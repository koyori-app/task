pub use sea_orm_migration::prelude::*;

mod m20260524080400_create_project_members;
mod m20260524080500_add_icon_to_projects;
mod m20260524100000_add_role_check;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260524080400_create_project_members::Migration),
            Box::new(m20260524080500_add_icon_to_projects::Migration),
            Box::new(m20260524100000_add_role_check::Migration),
        ]
    }
}
