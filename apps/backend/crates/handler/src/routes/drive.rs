use tower_http::limit::RequestBodyLimitLayer;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use service::drive::DriveConfig;

pub fn tenant_folder_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .routes(routes!(crate::handlers::drive_folders::list_folders))
        .routes(routes!(crate::handlers::drive_folders::create_folder))
        .routes(routes!(crate::handlers::drive_folders::update_folder))
        .routes(routes!(crate::handlers::drive_folders::delete_folder))
        .routes(routes!(crate::handlers::drive_folders::list_shares))
        .routes(routes!(crate::handlers::drive_folders::create_share))
        .routes(routes!(crate::handlers::drive_folders::delete_share))
}

pub fn tenant_drive_routes() -> OpenApiRouter<AppState> {
    let upload_limit = DriveConfig::from_env().upload_max_bytes;

    OpenApiRouter::<AppState>::new()
        .routes(routes!(
            crate::handlers::drive_files::list_files,
            crate::handlers::drive_files::upload_file,
        ))
        .routes(routes!(crate::handlers::drive_files::get_drive_usage))
        .routes(routes!(crate::handlers::drive_files::update_drive_quota))
        .routes(routes!(crate::handlers::drive_files::get_file))
        .routes(routes!(crate::handlers::drive_files::update_file))
        .routes(routes!(crate::handlers::drive_files::delete_file))
        .layer(RequestBodyLimitLayer::new(upload_limit as usize))
}

pub fn public_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .routes(routes!(
            crate::handlers::drive_folders::get_public_share_folder
        ))
        .routes(routes!(
            crate::handlers::drive_folders::list_public_share_files
        ))
        .routes(routes!(crate::handlers::drive_files::get_file_content))
}
