use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        // routes!マクロは一つのエンドポイントのメソッドをまとめてルーティングするためのマクロっぽい...?同じメソッドを複数定義しようとするとエラーになる。
        .routes(routes!(crate::handlers::auth::login))
        .routes(routes!(crate::handlers::auth::register))
        .routes(routes!(crate::handlers::auth::verify_email))
        .routes(routes!(crate::handlers::auth::resend_verification_email))
        .routes(routes!(crate::handlers::auth::logout))
        .routes(routes!(crate::handlers::auth::me))
        .routes(routes!(crate::handlers::auth_2fa::totp_setup))
        .routes(routes!(crate::handlers::auth_2fa::totp_verify_setup))
        .routes(routes!(crate::handlers::auth_2fa::verify_2fa))
        .routes(routes!(crate::handlers::auth_2fa::delete_totp))
        .routes(routes!(crate::handlers::auth_2fa::regenerate_recovery_codes))
}
