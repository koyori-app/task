use axum::routing::get;
use utoipa_scalar::{Scalar, Servable};

use crate::AppState;

pub async fn run(state: AppState) {
    let (mut router, openapi) = utoipa_axum::router::OpenApiRouter::new()
        .route("/", get(|| async { "Hello, world!" }))
        .merge(crate::routes::create_routes())
        .split_for_parts();

    let app = router
        .merge(Scalar::with_url("/scalar", openapi.clone()))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3400").await.unwrap();

    println!("Listening on http://0.0.0.0:3400");
    axum::serve(listener, app).await.unwrap();
}
