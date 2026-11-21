use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .routes(routes!(crate::handlers::oauth_grants::list_oauth_grants))
        .routes(routes!(
            crate::handlers::oauth_grants::preview_authorization
        ))
        .routes(routes!(
            crate::handlers::oauth_grants::approve_authorization
        ))
        .routes(routes!(crate::handlers::oauth_grants::deny_authorization))
        .routes(routes!(crate::handlers::oauth_grants::revoke_oauth_grant))
}
