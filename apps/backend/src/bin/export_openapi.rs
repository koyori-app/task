use axum::routing::get;
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (_, openapi) = utoipa_axum::router::OpenApiRouter::<backend::AppState>::new()
        .route("/", get(|| async { "Hello, world!" }))
        .merge(backend::routes::create_routes())
        .split_for_parts();

    let json = serde_json::to_string_pretty(&openapi)?;

    if let Some(path) = env::args().nth(1) {
        fs::write(path, json)?;
    } else {
        println!("{json}");
    }

    Ok(())
}
