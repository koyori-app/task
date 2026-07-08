use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .nest("/users", admin_users_routes())
        .nest("/tenants", admin_tenants_routes())
        .nest("/audit-logs", admin_audit_logs_routes())
        .nest("/system/settings", admin_settings_routes())
}

fn admin_users_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .routes(routes!(crate::handlers::admin_users::list_users))
        .routes(routes!(crate::handlers::admin_users::create_user))
        .routes(routes!(crate::handlers::admin_users::update_user))
        .routes(routes!(crate::handlers::admin_users::delete_user))
        .routes(routes!(crate::handlers::admin_users::password_reset))
        .routes(routes!(crate::handlers::admin_users::reset_2fa))
        .routes(routes!(crate::handlers::admin_users::delete_passkey))
        .routes(routes!(crate::handlers::admin_users::delete_oauth))
}

fn admin_tenants_routes() -> OpenApiRouter<AppState> {
    // ハンドラーの #[utoipa::path] が nest 位置からの相対パスを持つため、
    // ここで追加の nest を挟むとパスが二重連結されて 404 になる（実際になっていた）。
    OpenApiRouter::<AppState>::new()
        .routes(routes!(crate::handlers::admin_tenants::list_tenants))
        .routes(routes!(crate::handlers::admin_tenants::get_tenant))
        .routes(routes!(crate::handlers::admin_tenants::delete_tenant))
        .routes(routes!(
            crate::handlers::admin_tenants::list_tenant_projects
        ))
        .routes(routes!(
            crate::handlers::admin_tenants::list_tenant_project_tasks
        ))
}

fn admin_audit_logs_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .routes(routes!(crate::handlers::admin_audit_logs::list_audit_logs))
}

fn admin_settings_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .routes(routes!(
            crate::handlers::admin_settings::get_system_settings
        ))
        .routes(routes!(
            crate::handlers::admin_settings::update_system_settings
        ))
}
