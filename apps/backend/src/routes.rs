use utoipa_axum::router::OpenApiRouter;

use crate::AppState;

pub mod labels;

pub fn create_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().nest("/labels", crate::routes::labels::routes())
}
