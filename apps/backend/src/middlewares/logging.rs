use axum::{
    middleware::{Next},
    response::IntoResponse,
    http::Request,
    body::Body,
};

pub async fn logging_middleware(req: Request<Body>, next: Next) -> impl IntoResponse {
    // Log path only — never the query string (password-reset verify carries ?token=).
    let path = req.uri().path().to_owned();
    let method = req.method().clone();

    tracing::info!(%method, %path, "received request");

    next.run(req).await
}