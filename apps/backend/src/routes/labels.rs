use axum::routing::get;
use utoipa_axum::router::OpenApiRouter;

use crate::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new().route("/", get(crate::handlers::labels::get_labels))
}
