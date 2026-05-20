use axum::{Json, extract::State};
use sea_orm::EntityTrait;

use crate::openapi::InternalOnlyError;
use crate::utils::auth::AuthError;
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
) -> Result<Json<Vec<entities::labels::Model>>, AuthError> {
    let labels = entities::labels::Entity::find()
        .all(&state.db)
        .await
        .map_err(AuthError::from)?;
    Ok(Json(labels))
}
