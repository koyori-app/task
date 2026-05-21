
use apalis::layers::retry::RetryPolicy;
use apalis::layers::WorkerBuilderExt;
use apalis::prelude::WorkerBuilder;
use apalis_board::axum::{
    framework::{ApiBuilder, RegisterRoute},
    sse::{TracingBroadcaster, TracingSubscriber},
    ui::ServeUI,
};
use axum::{
    Extension, Router,
    body::Body,
    http::{HeaderValue, Method, Request},
    middleware,
    response::Redirect,
    routing::get,
};
use axum_session::{SameSite, SessionConfig, SessionLayer, SessionStore};
use axum_session_redispool::SessionRedisPool;
use sentry::integrations::tower::NewSentryLayer;
use tokio::sync::watch;
use tower::ServiceBuilder;
use tower_http::cors::{AllowHeaders, CorsLayer};
use tracing::{info, warn};
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};
use utoipa_scalar::{Scalar, Servable};

use crate::{
    AppState,
    jobs::verification_email::{self, MAX_RETRIES, QUEUE_NAME},
    middlewares::logging::logging_middleware,
};

pub async fn run(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let broadcaster = TracingBroadcaster::create();
    let board_tracing = TracingSubscriber::new(&broadcaster).layer().with_filter(
        tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,sqlx=warn".into()),
        ),
    );

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,sqlx=warn".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .with(board_tracing)
        .init();

    let is_prod = std::env::var("RUST_ENV").unwrap_or_default() == "production";
    let settings = &state.settings;

    let session_config = SessionConfig::default()
        .with_secure(is_prod)
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
        .merge(crate::routes::create_routes())
        .split_for_parts();

    crate::openapi::register_schemas(&mut openapi);

    let cors = CorsLayer::new()
        .allow_origin(settings.allow_origin.parse::<HeaderValue>()?)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(true);

    let email_storage = state.verification_email_storage.as_ref().clone();
    let board_api = ApiBuilder::new(Router::new())
        .register(email_storage)
        .build();

    let email_worker_storage = state.verification_email_storage.as_ref().clone();
    let worker_state = state.clone();
    let worker_concurrency = verification_email::worker_concurrency(settings);
    let email_worker = WorkerBuilder::new(format!("{QUEUE_NAME}-worker"))
        .backend(email_worker_storage)
        .retry(RetryPolicy::retries(MAX_RETRIES))
        .enable_tracing()
        .concurrency(worker_concurrency)
        .data(worker_state)
        .build(verification_email::process);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let worker_shutdown = shutdown_rx.clone();
    let worker_handle = tokio::spawn(async move {
        email_worker
            .run_until(wait_for_shutdown(worker_shutdown))
            .await
    });

    let api = router
        .merge(Scalar::with_url("/scalar", openapi.clone()))
        .with_state(state)
        .layer(cors)
        .layer(middleware::from_fn(logging_middleware))
        .layer(SessionLayer::new(session_store))
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top()));

    // apalis-board の UI はビルド時に API=/api/v1・静的ファイル=/ 直下を前提とする。
    // /jobs にネストすると JS/WASM が 404 になり真っ白になる。
    let app = Router::new()
        .merge(api)
        .nest("/api/v1", board_api)
        .route("/jobs", get(|| async { Redirect::permanent("/") }))
        .route("/jobs/", get(|| async { Redirect::permanent("/") }))
        .layer(Extension(broadcaster))
        .fallback_service(ServeUI::new());

    let addr = "0.0.0.0:3400";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on http://{addr}");
    info!("Apalis board: http://{addr}/");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal_inner().await;
            let _ = shutdown_tx.send(true);
            info!("shutting down HTTP server; Apalis workers finishing in-flight jobs");
        })
        .await?;

    match worker_handle.await {
        Ok(Ok(())) => info!("verification email worker stopped"),
        Ok(Err(e)) => warn!("verification email worker error: {e}"),
        Err(e) => warn!("verification email worker join error: {e}"),
    }

    Ok(())
}

async fn wait_for_shutdown(mut shutdown: watch::Receiver<bool>) -> Result<(), std::io::Error> {
    while !*shutdown.borrow() {
        if shutdown.changed().await.is_err() {
            break;
        }
    }
    Ok(())
}

async fn shutdown_signal_inner() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        sigterm.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => warn!("received Ctrl+C"),
        () = terminate => warn!("received SIGTERM"),
    }
}
