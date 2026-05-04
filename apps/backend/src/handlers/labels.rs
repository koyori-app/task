use axum::{Json, extract::State};
use sea_orm::EntityTrait;

use crate::{AppState, entities};

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    responses(
        (status = 200, description = "Labels list", body = [entities::labels::Model])
    )
)]
pub async fn get_labels(State(state): State<AppState>) -> Json<Vec<entities::labels::Model>> {
    let labels = entities::labels::Entity::find()
        .all(&state.db)
        .await
        .unwrap_or_default();
    Json(labels)
}
