use crate::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .routes(routes!(
            crate::handlers::task_notifications::list_notifications
        ))
        .routes(routes!(
            crate::handlers::task_notifications::mark_notification_read
        ))
        .routes(routes!(
            crate::handlers::task_notifications::mark_all_notifications_read
        ))
        .routes(routes!(
            crate::handlers::task_notifications::get_notification_settings
        ))
        .routes(routes!(
            crate::handlers::task_notifications::update_notification_settings
        ))
}
