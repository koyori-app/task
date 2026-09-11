pub use sea_orm_migration::prelude::*;

mod m20260520000000_initial_schema;
mod m20260818000000_tenant_members;
mod m20260818010000_github_issue_links;
mod m20260826000000_review_findings;
mod m20260904000000_drive_project_id_backfill;
mod m20260905000000_tenant_icon_emoji;
mod m20260907000000_default_done_status;
mod m20260911000000_forge_commit_links;
mod m20260911010000_oauth_provider_login;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260520000000_initial_schema::Migration),
            Box::new(m20260818000000_tenant_members::Migration),
            Box::new(m20260818010000_github_issue_links::Migration),
            Box::new(m20260826000000_review_findings::Migration),
            Box::new(m20260904000000_drive_project_id_backfill::Migration),
            Box::new(m20260905000000_tenant_icon_emoji::Migration),
            Box::new(m20260907000000_default_done_status::Migration),
            Box::new(m20260911000000_forge_commit_links::Migration),
            Box::new(m20260911010000_oauth_provider_login::Migration),
        ]
    }
}
