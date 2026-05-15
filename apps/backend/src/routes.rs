use utoipa_axum::router::OpenApiRouter;

use crate::AppState;

pub mod labels;
pub mod auth;
pub mod personal_tokens;

pub fn create_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().nest(
        "/v1",
        OpenApiRouter::new()
            .nest("/labels", crate::routes::labels::routes())
            .nest("/auth", crate::routes::auth::routes())
            .nest("/personal_tokens", crate::routes::personal_tokens::routes())
    )
}
