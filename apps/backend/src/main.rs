use backend::{AppState, server::run, utils::smtp::SmtpClient};

/// Application entry point that boots configuration, telemetry, storage, and network clients, then starts the server.
///
/// This initializes application settings, optionally configures Sentry when a DSN is provided, connects and syncs the database schema,
/// initializes the SMTP client, verifies Redis connectivity, constructs the shared application state, and launches the server runtime.
///
/// # Returns
///
/// `Ok(())` when the server starts successfully and runs until termination; any failure during configuration loading, Sentry DSN parsing,
/// database connection or schema sync, SMTP initialization, Redis ping, or server startup is propagated as an `Err`.
///
/// # Examples
///
/// ```no_run
/// // Typically run by the runtime; shown here for illustration only.
/// // `main()` initializes services and then runs the server until termination.
/// async fn run_app() {
///     let _ = crate::main().await;
/// }
/// ```
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = backend::settings::load_settings()?;
    let _guard = if let Some(ref sentry_dsn) = settings.sentry_dsn {
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

    let smtp_client = SmtpClient::new(
        &settings.smtp_host,
        settings.smtp_port,
        &settings.smtp_username,
        &settings.smtp_password,
         )
     .map_err(|err| {
         std::io::Error::other(format!(
             "SMTP client initialization failed. If email is required in this environment, check smtp_host/smtp_port/smtp_username/smtp_password. Underlying error: {err}"
         ))
     })?;
    let redis_client = backend::utils::redis::RedisConnection::new(&settings.redis_url);
    redis_client.ping().await?;
    let state = AppState {
        settings,
        db,
        redis_client,
        smtp_client,
    };
    run(state).await?;

    Ok(())
}
