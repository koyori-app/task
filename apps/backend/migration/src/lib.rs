pub use sea_orm_migration::prelude::*;

mod m20260524080400_create_project_members;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260524080400_create_project_members::Migration),
        ]
    }
}
