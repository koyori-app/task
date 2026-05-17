use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        // routes!マクロは一つのエンドポイントのメソッドをまとめてルーティングするためのマクロっぽい...?同じメソッドを複数定義しようとするとエラーになる。
        .routes(routes!(
            crate::handlers::personal_tokens::create_personal_token
        ))
        .routes(routes!(
            crate::handlers::personal_tokens::get_personal_token
        ))
        .routes(routes!(
            crate::handlers::personal_tokens::revoke_personal_token
        ))
        .routes(routes!(
            crate::handlers::personal_tokens::revoke_all_personal_tokens
        ))
}
