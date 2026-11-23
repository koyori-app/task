use axum::{
    Json,
    extract::{Path, Query, State},
};
use sea_orm::prelude::Uuid;

use crate::AppState;
use crate::error::AppError;
use crate::extractors::AuthUser;
use crate::openapi::CrudErrors;
use entity::scopes::Scope;
use payload::documents::{DocumentSearchResponse, SearchDocumentsQuery};
use service::drive::can_access_project;
use service::drive_sections::MAX_QUERY_CHARS;

/// 検索語の語数の上限。語ごとに ILIKE を 1 つ足すので抑える
const MAX_QUERY_TERMS: usize = 10;
const MAX_SEARCH_LIMIT: u64 = 100;

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/search",
    tag = "Drive Files",
    summary = "プロジェクトのドキュメント本文検索",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        SearchDocumentsQuery,
    ),
    responses(
        (status = 200, description = "検索結果", body = DocumentSearchResponse),
        CrudErrors,
    )
)]
pub async fn search_documents(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Query(q): Query<SearchDocumentsQuery>,
) -> Result<Json<DocumentSearchResponse>, AppError> {
    auth.require_scope(Scope::ReadDrive)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    // ensure_tenant_access は、テナントに所属せずプロジェクトにだけ明示指定された利用者も通す。
    // Drive はその利用者を通さないので、ここで止める。フォルダの利用者指定の共有と
    // 公開リンクのトークンは見ない（検索はプロジェクトに入れる人だけ）
    if !can_access_project(&state.db, tenant_id, project_id, auth.user_id).await? {
        return Err(AppError::Forbidden);
    }

    let query = q.q.trim();
    // NUL は Postgres の text に束縛できず、DB のエラー（500）になる。索引の本文にも残らない
    if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS || query.contains('\0') {
        return Err(AppError::BadRequest);
    }
    let terms: Vec<String> = query.split_whitespace().map(str::to_owned).collect();
    if terms.len() > MAX_QUERY_TERMS {
        return Err(AppError::BadRequest);
    }
    let limit = q.limit.clamp(1, MAX_SEARCH_LIMIT);

    let response =
        service::drive_sections::search(&state.db, tenant_id, project_id, &terms, limit, q.offset)
            .await?;
    Ok(Json(response))
}
