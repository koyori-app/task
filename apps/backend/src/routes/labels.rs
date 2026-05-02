pub fn routes() -> axum::Router {
    axum::Router::new().route("/labels", axum::routing::get(crate::handlers::labels::get_labels))
}