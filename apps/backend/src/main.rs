use axum::routing::get;
use utoipa_scalar::{Scalar, Servable};


#[tokio::main]
async fn main() {
    let (mut router, openapi) =
        utoipa_axum::router::OpenApiRouter::new().route("/", get(|| async { "Hello, world!" })).split_for_parts();

    router = router.merge(Scalar::with_url("/scalar", openapi.clone()));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();

    println!("Listening on http://0.0.0.0:3000");
    axum::serve(listener, router).await.unwrap();
}
