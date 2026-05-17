use backend::{AppState, server::run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = backend::settings::load_settings()?;
    let _guard = if let Some(sentry_dsn) = settings.sentry_dsn {
        Some(sentry::init(sentry::ClientOptions {
            dsn: Some(sentry_dsn.parse()?),
            release: sentry::release_name!(),
            // 本番既定は false。必要時のみ環境変数で opt-in 推奨   
            send_default_pii: false,
            ..Default::default()
        }))
    } else {
        None
    };
    let db = sea_orm::Database::connect(&settings.database_url).await?;
    db.get_schema_registry("backend::entities::*")
        .sync(&db)
        .await?;

    let redis_client = backend::utils::redis::RedisConnection::new(&settings.redis_url);
    redis_client.ping().await?;
    let state = AppState { db, redis_client };
    run(state).await;

    Ok(())
}
