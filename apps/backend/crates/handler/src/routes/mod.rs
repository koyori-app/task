use utoipa_axum::router::OpenApiRouter;

use crate::AppState;

pub mod admin;
pub mod auth;
pub mod drive;
pub mod github;
pub mod personal_tokens;
pub mod tenants;
pub mod users;

pub fn create_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().nest(
        "/v1",
        OpenApiRouter::new()
            .nest("/admin", crate::routes::admin::routes())
            .nest("/auth", crate::routes::auth::routes())
            .nest("/personal_tokens", crate::routes::personal_tokens::routes())
            .nest("/users", crate::routes::users::routes())
            .nest("/tenants", crate::routes::tenants::routes())
            .nest("/github", crate::routes::github::public_github_routes())
            .nest("/drive", crate::routes::drive::public_routes()),
    )
}
