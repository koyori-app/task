use axum::{
    middleware::{Next},
    response::IntoResponse,
    http::Request,
    body::Body,
};

pub async fn logging_middleware(req: Request<Body>, next: Next) -> impl IntoResponse {
    let path = req.uri().path().to_owned();
    let method = req.method().clone();

    tracing::info!("Received request: {} {}", method, path);

    next.run(req).await
}