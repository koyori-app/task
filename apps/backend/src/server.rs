use axum::{
    body::Body,
    http::Request,
    http::{HeaderValue, Method},
    routing::get,
};
use axum_session::{SameSite, SessionConfig, SessionLayer, SessionStore};
use axum_session_redispool::SessionRedisPool;
use sentry::integrations::tower::NewSentryLayer;
use tower::ServiceBuilder;
use tower_http::cors::{AllowHeaders, CorsLayer};
use utoipa_scalar::{Scalar, Servable};

use crate::{AppState, settings};

pub async fn run(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let is_prod = std::env::var("RUST_ENV").unwrap_or_default() == "production";
    let settings = settings::load_settings()?;

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

    let (mut router, openapi) = utoipa_axum::router::OpenApiRouter::new()
        .route("/", get(|| async { "Hello, world!" }))
        .merge(crate::routes::create_routes())
        .split_for_parts();

    // Allow credentials and mirror the request origin/headers so we don't send
    // wildcard `*` which is disallowed when `Access-Control-Allow-Credentials` is true.
    let cors = CorsLayer::new()
        .allow_origin(settings.allow_origin.parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(true);

    let app = router
        .merge(Scalar::with_url("/scalar", openapi.clone()))
        .layer(SessionLayer::new(session_store))
        .with_state(state)
        .layer(cors)
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top())); // Bind a new Hub per request, to ensure correct error <> request correlation

    println!("Listening on http://0.0.0.0:3400");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3400").await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
