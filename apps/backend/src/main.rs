use backend::{AppState, server::run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = backend::settings::load_settings();
    let db = sea_orm::Database::connect(&settings.database_url).await?;
    db.get_schema_registry("backend::entities::*")
        .sync(&db)
        .await?;

    let state = AppState { db };
    run(state).await;

    Ok(())
}
