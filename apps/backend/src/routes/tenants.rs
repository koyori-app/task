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
            "/{tenant_id}/drive/folders",
            crate::routes::drive::tenant_folder_routes(),
        )
        .nest(
            "/{tenant_id}/projects",
            OpenApiRouter::<AppState>::new()
                .routes(routes!(crate::handlers::projects::list_projects))
                .routes(routes!(crate::handlers::projects::create_project))
                .routes(routes!(crate::handlers::projects::get_project))
                .routes(routes!(crate::handlers::projects::update_project))
                .routes(routes!(crate::handlers::projects::delete_project))
                .nest(
                    "/{project_id}/members",
                    OpenApiRouter::<AppState>::new()
                        .routes(routes!(crate::handlers::project_members::list_members))
                        .routes(routes!(crate::handlers::project_members::add_member))
                        .routes(routes!(crate::handlers::project_members::update_member))
                        .routes(routes!(crate::handlers::project_members::remove_member)),
                ),
        )
        .nest(
            "/{tenant_id}/drive",
            crate::routes::drive::tenant_drive_routes(),
        )
}
