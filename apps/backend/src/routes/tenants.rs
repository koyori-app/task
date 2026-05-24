use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .routes(routes!(crate::handlers::tenants::list_tenants))
        .routes(routes!(crate::handlers::tenants::create_tenant))
        .routes(routes!(crate::handlers::tenants::get_tenant))
        .routes(routes!(crate::handlers::tenants::update_tenant))
        .routes(routes!(crate::handlers::tenants::delete_tenant))
        .nest(
            "/{tenant_id}/projects",
            OpenApiRouter::<AppState>::new()
                .routes(routes!(crate::handlers::projects::list_projects))
                .routes(routes!(crate::handlers::projects::create_project))
                .routes(routes!(crate::handlers::projects::get_project))
                .routes(routes!(crate::handlers::projects::update_project))
                .routes(routes!(crate::handlers::projects::delete_project)),
        )
}
