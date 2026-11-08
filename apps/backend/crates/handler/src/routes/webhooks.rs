use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

/// `/{project_id}/webhooks` 配下。
pub fn webhook_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        .routes(routes!(crate::handlers::webhooks::list_webhooks))
        .routes(routes!(crate::handlers::webhooks::create_webhook))
        .routes(routes!(crate::handlers::webhooks::update_webhook))
        .routes(routes!(crate::handlers::webhooks::delete_webhook))
        .routes(routes!(crate::handlers::webhooks::list_webhook_deliveries))
        .routes(routes!(
            crate::handlers::webhooks::redeliver_webhook_delivery
        ))
}
