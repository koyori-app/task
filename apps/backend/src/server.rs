use axum::{body::Body, http::Request, routing::get};
use sentry::integrations::tower::NewSentryLayer;
use tower::ServiceBuilder;
use axum::routing::get;
use axum_session::{SameSite, SessionConfig, SessionLayer, SessionStore};
use axum_session_redispool::SessionRedisPool;
use tower_http::cors::{Any, CorsLayer};
use utoipa_scalar::{Scalar, Servable};

use crate::AppState;

pub async fn run(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let session_config = SessionConfig::default()
        .with_secure(false) // 開発環境ではセキュアクッキーを無効にする
        .with_cookie_same_site(SameSite::None); // SameSite属性をNoneに設定

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

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

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
