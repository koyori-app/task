use utoipa_axum::router::OpenApiRouter;

use crate::AppState;

pub mod admin;
pub mod auth;
pub mod drive;
pub mod personal_tokens;
pub mod tenants;

pub fn create_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().nest(
        "/v1",
        OpenApiRouter::new()
<<<<<<< HEAD
=======
<<<<<<< HEAD
            .nest("/labels", crate::routes::labels::routes())
=======
            .nest("/admin", crate::routes::admin::routes())
>>>>>>> 591de1bf (feat(admin): implement Phase A admin management API)
>>>>>>> 7886cd5c (feat(admin): implement Phase A admin management API)
            .nest("/auth", crate::routes::auth::routes())
            .nest("/personal_tokens", crate::routes::personal_tokens::routes())
            .nest("/tenants", crate::routes::tenants::routes())
            .nest("/drive", crate::routes::drive::public_routes()),
    )
}
