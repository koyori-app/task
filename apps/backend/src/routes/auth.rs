use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::<AppState>::new()
        // routes!マクロは一つのエンドポイントのメソッドをまとめてルーティングするためのマクロっぽい...?同じメソッドを複数定義しようとするとエラーになる。
        .routes(routes!(crate::handlers::auth::login))
        .routes(routes!(crate::handlers::auth::register))
        .routes(routes!(crate::handlers::auth::logout))
}
