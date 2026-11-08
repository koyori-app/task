use crate::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .routes(routes!(
            crate::handlers::desktop_auth::create_desktop_auth_code
        ))
        .routes(routes!(
            crate::handlers::desktop_auth::exchange_desktop_auth_token
        ))
}
