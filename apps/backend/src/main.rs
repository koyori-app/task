use backend::{AppState, server::run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = backend::settings::load_settings();
    let _guard = sentry::init(sentry::ClientOptions {
        dsn: Some(settings.sentry_dsn.parse()?),
        release: sentry::release_name!(),
        // Capture user IPs and potentially sensitive headers when using HTTP server integrations
        // see https://docs.sentry.io/platforms/rust/data-management/data-collected for more info
        send_default_pii: true,
        ..Default::default()
    });
    let db = sea_orm::Database::connect(&settings.database_url).await?;
    db.get_schema_registry("backend::entities::*")
        .sync(&db)
        .await?;

    let state = AppState { db };
    run(state).await?;

    Ok(())
}
