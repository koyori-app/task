use axum::{
    body::Body, http::{HeaderValue, Method, Request}, middleware, routing::get
};
use axum_session::{SameSite, SessionConfig, SessionLayer, SessionStore};
use axum_session_redispool::SessionRedisPool;
use sentry::integrations::tower::NewSentryLayer;
use tower::ServiceBuilder;
use tower_http::cors::{AllowHeaders, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa_scalar::{Scalar, Servable};

use crate::{
    AppState, middlewares::logging::logging_middleware,
    utils::verification_email_outbox,
};



pub async fn run(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
        tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let is_prod = std::env::var("RUST_ENV").unwrap_or_default() == "production";
    let settings = &state.settings;

    let session_config = SessionConfig::default()
        .with_secure(is_prod) // 本番では secure=true にする
        .with_cookie_same_site(if is_prod {
            SameSite::None
        } else {
            SameSite::Lax
        });

    let session_store = SessionStore::<SessionRedisPool>::new(
        Some(state.redis_client.conn.clone().into()),
        session_config,
    )
    .await
    .unwrap();

    let (router, mut openapi) = utoipa_axum::router::OpenApiRouter::new()
        .route("/", get(|| async { "Hello, world!" }))
        .merge(crate::routes::create_routes())
        .split_for_parts();

    crate::openapi::register_schemas(&mut openapi);

    // Allow credentials and mirror the request origin/headers so we don't send
    // wildcard `*` which is disallowed when `Access-Control-Allow-Credentials` is true.
    let cors = CorsLayer::new()
        .allow_origin(settings.allow_origin.parse::<HeaderValue>()?)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(true);

    let worker_state = state.clone();
    tokio::spawn(async move {
        verification_email_outbox::run_worker(worker_state).await;
    });

    let app = router
        .merge(Scalar::with_url("/scalar", openapi.clone()))
        .with_state(state)
        .layer(cors)
        .layer(middleware::from_fn(logging_middleware))
        .layer(SessionLayer::new(session_store))
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top())); // Bind a new Hub per request, to ensure correct error <> request correlation

    let addr = "0.0.0.0:3400";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    println!("Shutting down...");
}
