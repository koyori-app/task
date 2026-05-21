use axum::{Json, extract::State};
use sea_orm::EntityTrait;

use crate::error::AppError;
use crate::openapi::InternalOnlyError;
use crate::{AppState, entities};

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    summary = "ラベル一覧",
    responses(
        (
            status = 200,
            description = "すべてのラベル",
            body = [entities::labels::Model]
        ),
        InternalOnlyError,
    )
)]
pub async fn get_labels(
    State(state): State<AppState>,
) -> Result<Json<Vec<entities::labels::Model>>, AppError> {
    // DB 障害時は 500 を返す
    let labels = entities::labels::Entity::find().all(&state.db).await?;
    Ok(Json(labels))
}
