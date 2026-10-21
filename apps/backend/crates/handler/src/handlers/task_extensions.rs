//! 全文検索・バルク操作・保存済みビュー・ファイル添付。

use crate::AppState;
use crate::auth_helpers::{is_tenant_owner, require_project_access};
use crate::error::{AppError, ServerError};
use crate::extractors::AuthUser;
use crate::handlers::tasks::resolve_task;
use crate::openapi::CrudErrors;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use axum_valid::Valid;
use chrono::Utc;
use entity::{
    drive_files, labels, project_statuses, project_task_views, sprints, task_assignees,
    task_attachments, task_labels, tasks,
};
use payload::drive_files::content_url;
use payload::task_extensions::*;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Statement, TransactionTrait,
    prelude::Uuid,
};
use service::db::is_postgres_unique_violation;
use service::task_activities::{
    record_activity, record_label_diff, status_name, task_label_entries,
};
use std::collections::HashSet;

const BULK_MAX_TASKS: usize = 100;

fn use_pg_bigm_search() -> bool {
    matches!(
        std::env::var("USE_PG_BIGM").as_deref(),
        Ok("1") | Ok("true") | Ok("True") | Ok("TRUE")
    )
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/search",
    tag = "Tasks",
    summary = "タスク全文検索",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        SearchTasksQuery,
    ),
    responses(
        (status = 200, description = "検索結果", body = SearchTasksResponse),
        CrudErrors,
    )
)]
pub async fn search_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Query(q): Query<SearchTasksQuery>,
) -> Result<Json<SearchTasksResponse>, AppError> {
    auth.require_scope(entity::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;

    let query = q.q.trim();
    if query.is_empty() {
        return Err(AppError::BadRequest);
    }

    let limit = q.limit.min(100);
    let offset = q.offset;
    let mentions = mention_search_terms(&state, project_id, query).await?;

    // pg_bigm は LIKE + gin_bigm_ops で使う拡張であり tsvector 設定名ではない。
    // USE_PG_BIGM=true の場合は ILIKE + gin_bigm_ops index 経路を使う。
    if use_pg_bigm_search() {
        search_tasks_ilike(&state, project_id, query, mentions.as_ref(), limit, offset).await
    } else {
        search_tasks_tsvector(&state, project_id, query, mentions.as_ref(), limit, offset).await
    }
}

/// 検索語のうち、`@名` を印のメンションにも当てる形に引いた物。
///
/// 説明文のメンションは本文に `<@利用者ID>` の印で入り、名は入らぬ。ゆえに `@名` の語は、
/// 検索の時に名を利用者 id へ引き（`service::mentions::resolve_names`。名を返す境と同じ）、
/// その者の印を含む説明文にも当てる。名は検索の時に引くゆえ、改名の後は今の名で当たり、
/// 前の名では当たらぬ。`@` だけの語と、境の中の誰の名でもない語は、今どおり文字として当てる。
struct MentionSearch {
    /// `@名` の語ごとに、その語の文字と、印で当てる利用者 id（境の中に誰もいなければ空）
    terms: Vec<(String, Vec<Uuid>)>,
    /// `@名` の語を除いた残りの語（空なら無い）
    rest: String,
}

/// `@名` の語（`@` の後に空白までの一続きの字。`@` だけの語は含めない）。
fn mention_names(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter_map(|word| word.strip_prefix('@'))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

async fn mention_search_terms(
    state: &AppState,
    project_id: Uuid,
    query: &str,
) -> Result<Option<MentionSearch>, AppError> {
    let names = mention_names(query);
    if names.is_empty() {
        return Ok(None);
    }
    let resolved = service::mentions::resolve_names(&state.db, project_id, &names).await?;
    if resolved.is_empty() {
        // 誰の名でもない。今どおり、検索語の文字だけで当てる
        return Ok(None);
    }
    let terms = names
        .into_iter()
        .map(|name| {
            let ids = resolved
                .get(&name.to_lowercase())
                .cloned()
                .unwrap_or_default();
            (name, ids)
        })
        .collect();
    let rest = query
        .split_whitespace()
        .filter(|word| !(word.starts_with('@') && word.len() > 1))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(Some(MentionSearch { terms, rest }))
}

/// 説明文がその者の印を含むかを LIKE で見る形（id は 16 進と `-` だけで、LIKE の特殊字を含まぬ）。
fn marker_pattern(id: &Uuid) -> String {
    format!("%<@{id}>%")
}

/// 検索語を、前後に `%` を付けた ILIKE のパターンにする。
/// 語の中の `%` `_` `\` は、共通の `common::db::escape_like` で字義どおりに当てる。
fn ilike_pattern(text: &str) -> String {
    format!("%{}%", common::db::escape_like(text))
}

fn ilike_text(text: &str) -> sea_orm::sea_query::SimpleExpr {
    let pattern = ilike_pattern(text);
    Expr::cust_with_values(
        "(COALESCE(title, '') ILIKE $1 ESCAPE '\\' OR COALESCE(description, '') ILIKE $1 ESCAPE '\\')",
        [pattern],
    )
}

async fn search_tasks_ilike(
    state: &AppState,
    project_id: Uuid,
    query: &str,
    mentions: Option<&MentionSearch>,
    limit: u64,
    offset: u64,
) -> Result<Json<SearchTasksResponse>, AppError> {
    // 今どおりの「検索語の全体を一続きの文字として含む」に、`@名` を印にも当てる形を OR で足す
    let mut matched = Condition::any().add(ilike_text(query));
    if let Some(mentions) = mentions {
        let mut all = Condition::all();
        for (name, ids) in &mentions.terms {
            let mut term = Condition::any().add(ilike_text(&format!("@{name}")));
            for id in ids {
                term = term.add(Expr::cust_with_values(
                    "COALESCE(description, '') LIKE $1",
                    [marker_pattern(id)],
                ));
            }
            all = all.add(term);
        }
        if !mentions.rest.is_empty() {
            all = all.add(ilike_text(&mentions.rest));
        }
        matched = matched.add(all);
    }
    let base = tasks::Entity::find()
        .filter(tasks::Column::ProjectId.eq(project_id))
        .filter(tasks::Column::DeletedAt.is_null())
        .filter(matched);

    let total = base.clone().count(&state.db).await?;
    let rows = base
        .order_by_desc(tasks::Column::UpdatedAt)
        // 同じ更新時刻が並ぶと offset のページ境界で行が重複・欠落する
        .order_by_asc(tasks::Column::Id)
        .limit(limit)
        .offset(offset)
        .all(&state.db)
        .await?;

    // 説明文のメンションの印は id を含むので、一覧の抜粋に出す前に `@今の名` に戻す
    let mention_map = service::mentions::resolve_in_bodies(
        &state.db,
        project_id,
        rows.iter().filter_map(|t| t.description.as_deref()),
    )
    .await?;
    let hits = rows
        .into_iter()
        .map(|t| SearchTaskHit {
            id: t.id,
            seq_id: t.seq_id,
            title: t.title.clone(),
            highlight: highlight_ilike(
                &t.title,
                t.description
                    .as_deref()
                    .map(|d| payload::mentions::to_plain(d, &mention_map))
                    .as_deref(),
                query,
            ),
            score: 1.0,
        })
        .collect();

    Ok(Json(SearchTasksResponse { tasks: hits, total }))
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn chars_eq_ignore_case(a: char, b: char) -> bool {
    a.to_lowercase().eq(b.to_lowercase())
}

fn find_match_char_range(text: &str, query: &str) -> Option<(usize, usize)> {
    let query_chars: Vec<char> = query.chars().collect();
    if query_chars.is_empty() {
        return None;
    }
    let text_chars: Vec<char> = text.chars().collect();
    let q_len = query_chars.len();
    // 近似一致（`<%`）や空白の連続する検索語では、対象より検索語の方が長いことがある
    if q_len > text_chars.len() {
        return None;
    }
    for start in 0..=text_chars.len().saturating_sub(q_len) {
        if text_chars[start..start + q_len]
            .iter()
            .zip(query_chars.iter())
            .all(|(a, b)| chars_eq_ignore_case(*a, *b))
        {
            return Some((start, start + q_len));
        }
    }
    None
}

fn chars_slice(s: &str, start: usize, end: usize) -> String {
    s.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

fn highlight_ilike(title: &str, description: Option<&str>, query: &str) -> String {
    let text = format!("{} {}", title, description.unwrap_or_default());
    // 検索語の全体が見つからねば、語ごと（`@名` の語は印を戻した `@今の名` に当たる）に探す
    let found = find_match_char_range(&text, query).or_else(|| {
        query
            .split_whitespace()
            .find_map(|word| find_match_char_range(&text, word))
    });
    if let Some((start_char, end_char)) = found {
        let total_chars = text.chars().count();
        let display_start = start_char.saturating_sub(60);
        let display_end = (end_char + 60).min(total_chars);
        let prefix_marker = if display_start > 0 { "…" } else { "" };
        let suffix_marker = if display_end < total_chars { "…" } else { "" };
        format!(
            "{prefix_marker}{}<em>{}</em>{}{suffix_marker}",
            html_escape(&chars_slice(&text, display_start, start_char)),
            html_escape(&chars_slice(&text, start_char, end_char)),
            html_escape(&chars_slice(&text, end_char, display_end)),
        )
    } else {
        html_escape(&text.chars().take(120).collect::<String>())
    }
}

/// tsvector の道の FROM 句（`$2` は検索語の全体、`tsq` はその全体の tsquery）。
const TSVECTOR_FROM: &str = "FROM tasks CROSS JOIN plainto_tsquery('pg_catalog.simple', $2) AS tsq";

/// tsvector の道の WHERE の当て方と、その値（`$1` は project_id、`$2` は検索語の全体、`$3` はその ILIKE のパターン）。
///
/// 今どおりの「検索語の全体を tsvector、部分一致（ILIKE）、表記ゆれ（`<%`）のどれかで当てる」に、
/// `@名` の語ごとに「その語の文字か、境の中のその名の者の印を含む」を AND で束ね、
/// 残りの語の当てと合わせた形を OR で足す。
/// 語の文字の当て方は、検索語の全体と同じ三つの OR に揃える（語の当て方だけが狭いと、
/// メンションを含む検索だけ当たり方が変わるため）。
fn tsvector_match(
    project_id: Uuid,
    query: &str,
    mentions: Option<&MentionSearch>,
) -> (String, Vec<sea_orm::Value>) {
    fn placeholder(value: sea_orm::Value, values: &mut Vec<sea_orm::Value>) -> String {
        values.push(value);
        format!("${}", values.len())
    }
    /// 一語を、tsvector、部分一致、表記ゆれのどれかで当てる形（値を二つ使う）。
    fn text_match(text: &str, values: &mut Vec<sea_orm::Value>) -> String {
        let t = placeholder(text.into(), values);
        let p = placeholder(ilike_pattern(text).into(), values);
        format!(
            "(search_vector @@ plainto_tsquery('pg_catalog.simple', {t}) \
             OR title ILIKE {p} ESCAPE '\\' \
             OR COALESCE(description, '') ILIKE {p} ESCAPE '\\' \
             OR {t} <% title)"
        )
    }
    let mut values: Vec<sea_orm::Value> =
        vec![project_id.into(), query.into(), ilike_pattern(query).into()];
    let mut matched = "(search_vector @@ tsq \
         OR title ILIKE $3 ESCAPE '\\' \
         OR COALESCE(description, '') ILIKE $3 ESCAPE '\\' \
         OR $2 <% title)"
        .to_string();
    if let Some(mentions) = mentions {
        let mut all = Vec::new();
        for (name, ids) in &mentions.terms {
            let mut any = vec![text_match(&format!("@{name}"), &mut values)];
            for id in ids {
                let marker = placeholder(marker_pattern(id).into(), &mut values);
                any.push(format!("COALESCE(description, '') LIKE {marker}"));
            }
            all.push(format!("({})", any.join(" OR ")));
        }
        if !mentions.rest.is_empty() {
            all.push(text_match(&mentions.rest, &mut values));
        }
        matched = format!("({matched} OR ({}))", all.join(" AND "));
    }
    (
        format!("project_id = $1 AND deleted_at IS NULL AND {matched}"),
        values,
    )
}

async fn search_tasks_tsvector(
    state: &AppState,
    project_id: Uuid,
    query: &str,
    mentions: Option<&MentionSearch>,
    limit: u64,
    offset: u64,
) -> Result<Json<SearchTasksResponse>, AppError> {
    let (where_clause, values) = tsvector_match(project_id, query, mentions);
    let count_sql = format!("SELECT COUNT(*)::bigint AS cnt {TSVECTOR_FROM} WHERE {where_clause}");
    let count_result = state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            count_sql,
            values.clone(),
        ))
        .await?;
    let total: i64 = count_result
        .and_then(|r| r.try_get_by_index(0).ok())
        .unwrap_or(0);

    let mut values = values;
    values.push((limit as i64).into());
    let limit_at = values.len();
    values.push((offset as i64).into());
    let offset_at = values.len();
    // ts_rank（0.1 前後）と word_similarity（0.6〜1.0）は値域が揃わないので、
    // タイトル一致を第 1 キーにして大小比較を同じ一致種別の中に閉じる。
    // score は同値が頻発する。並びが一意でないと offset のページ境界で行が重複・欠落する
    let search_sql = format!(
        "SELECT id, seq_id, title, description, \
                GREATEST(ts_rank(search_vector, tsq), word_similarity($2, title))::real AS score \
         {TSVECTOR_FROM} WHERE {where_clause} \
         ORDER BY (title ILIKE $3 ESCAPE '\\' OR $2 <% title) DESC, score DESC, id ASC \
         LIMIT ${limit_at} OFFSET ${offset_at}"
    );
    let rows = state
        .db
        .query_all_raw(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            search_sql,
            values,
        ))
        .await?;
    let mention_map = service::mentions::resolve_in_bodies(
        &state.db,
        project_id,
        rows.iter()
            .filter_map(|row| row.try_get_by_index::<Option<String>>(3).ok().flatten())
            .collect::<Vec<_>>()
            .iter()
            .map(String::as_str),
    )
    .await?;
    let hits = rows
        .into_iter()
        .filter_map(|row| {
            let title: String = row.try_get_by_index(2).ok()?;
            let description: Option<String> = row
                .try_get_by_index::<Option<String>>(3)
                .ok()
                .flatten()
                .map(|d| payload::mentions::to_plain(&d, &mention_map));
            let highlight = highlight_ilike(&title, description.as_deref(), query);
            Some(SearchTaskHit {
                id: row.try_get_by_index(0).ok()?,
                seq_id: row.try_get_by_index(1).ok()?,
                title,
                score: row.try_get_by_index(4).ok()?,
                highlight,
            })
        })
        .collect();

    Ok(Json(SearchTasksResponse {
        tasks: hits,
        total: total as u64,
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/bulk",
    tag = "Tasks",
    summary = "タスク一括更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = BulkUpdateRequest,
    responses(
        (status = 200, description = "一括更新結果", body = BulkUpdateResponse),
        CrudErrors,
    )
)]
pub async fn bulk_update_tasks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<BulkUpdateRequest>>,
) -> Result<Json<BulkUpdateResponse>, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;

    if payload.task_ids.len() > BULK_MAX_TASKS {
        return Err(AppError::BadRequest);
    }
    if let (Some(add), Some(remove)) = (
        &payload.update.add_label_ids,
        &payload.update.remove_label_ids,
    ) {
        let remove: HashSet<&Uuid> = remove.iter().collect();
        if add.iter().any(|id| remove.contains(id)) {
            return Err(AppError::BadRequest);
        }
    }

    let mut unique_ids = payload.task_ids.clone();
    unique_ids.sort();
    unique_ids.dedup();

    let mut updated = 0u32;
    let mut failed = Vec::new();

    for task_id in unique_ids {
        match apply_bulk_update(
            &state,
            tenant_id,
            project_id,
            task_id,
            auth.user_id,
            &payload.update,
        )
        .await
        {
            Ok(()) => updated += 1,
            Err(e) => failed.push(BulkFailure {
                task_id,
                reason: bulk_error_reason(&e),
            }),
        }
    }

    Ok(Json(BulkUpdateResponse { updated, failed }))
}

fn bulk_error_reason(err: &AppError) -> String {
    match err {
        AppError::NotFound => "not-found".into(),
        AppError::Forbidden => "forbidden".into(),
        AppError::Conflict => "conflict".into(),
        AppError::BadRequest => "bad-request".into(),
        AppError::BadRequestDetail(d) => d.clone(),
        _ => "error".into(),
    }
}

async fn apply_bulk_update(
    state: &AppState,
    tenant_id: Uuid,
    project_id: Uuid,
    task_id: Uuid,
    user_id: Uuid,
    update: &BulkUpdateFields,
) -> Result<(), AppError> {
    let task = tasks::Entity::find_by_id(task_id)
        .filter(tasks::Column::ProjectId.eq(project_id))
        .filter(tasks::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let txn = state.db.begin().await?;
    let snapshot = task.clone();
    let mut active: tasks::ActiveModel = task.into();

    if let Some(status_id) = update.status_id {
        // ステータスが変わらない場合は completed_at もアクティビティも更新しない
        if status_id != snapshot.status_id {
            let status = project_statuses::Entity::find_by_id(status_id)
                .filter(project_statuses::Column::ProjectId.eq(project_id))
                .one(&txn)
                .await?
                .ok_or(AppError::NotFound)?;
            active.status_id = Set(status_id);
            // 単体更新 API と同じルール: done 状態なら既存 completed_at を保持、なければ now()
            active.completed_at = if status.is_done_state {
                Set(Some(
                    snapshot
                        .completed_at
                        .unwrap_or_else(|| chrono::Utc::now().into()),
                ))
            } else {
                Set(None)
            };
            let from = status_name(&txn, snapshot.status_id).await?;
            let to = status_name(&txn, status_id).await?;
            record_activity(
                &txn,
                task_id,
                Some(user_id),
                "status_changed",
                serde_json::json!({ "from": from, "to": to }),
            )
            .await?;
        }
    }

    if update.clear_sprint_id {
        active.sprint_id = Set(None);
    } else if let Some(sprint_id) = update.sprint_id {
        sprints::Entity::find_by_id(sprint_id)
            .filter(sprints::Column::ProjectId.eq(project_id))
            .one(&txn)
            .await?
            .ok_or(AppError::NotFound)?;
        active.sprint_id = Set(Some(sprint_id));
    }

    active.updated_at = Set(chrono::Utc::now().into());
    active.update(&txn).await?;

    if let Some(assignee_id) = update.assignee_id {
        require_project_access(&txn, tenant_id, project_id, assignee_id).await?;
        let exists = task_assignees::Entity::find()
            .filter(task_assignees::Column::TaskId.eq(task_id))
            .filter(task_assignees::Column::UserId.eq(assignee_id))
            .one(&txn)
            .await?
            .is_some();
        if !exists {
            task_assignees::ActiveModel {
                id: Set(Uuid::new_v4()),
                task_id: Set(task_id),
                user_id: Set(assignee_id),
                role: Set("assignee".into()),
                assigned_at: Set(chrono::Utc::now().into()),
            }
            .insert(&txn)
            .await?;
            record_activity(
                &txn,
                task_id,
                Some(user_id),
                "assignee_added",
                serde_json::json!({ "user_id": assignee_id }),
            )
            .await?;
        }
    }

    // ラベル変更の前後スナップショット。実際に集合が変わったときだけ記録する。
    // ラベルを変える入力が増えたら、この判定式に足すこと。
    // ここを忘れると記録だけが静かに欠ける
    let labels_will_change = update.add_label_ids.is_some() || update.remove_label_ids.is_some();
    let before_labels = if labels_will_change {
        Some(task_label_entries(&txn, task_id).await?)
    } else {
        None
    };

    if let Some(ref label_ids) = update.add_label_ids {
        let mut unique = label_ids.clone();
        unique.sort();
        unique.dedup();
        if !unique.is_empty() {
            let in_project = labels::Entity::find()
                .filter(labels::Column::Id.is_in(unique.clone()))
                .filter(labels::Column::ProjectId.eq(project_id))
                .all(&txn)
                .await?;
            if in_project.len() != unique.len() {
                return Err(AppError::BadRequest);
            }
            for lid in unique {
                let exists = task_labels::Entity::find()
                    .filter(task_labels::Column::TaskId.eq(task_id))
                    .filter(task_labels::Column::LabelId.eq(lid))
                    .one(&txn)
                    .await?
                    .is_some();
                if !exists {
                    task_labels::ActiveModel {
                        task_id: Set(task_id),
                        label_id: Set(lid),
                    }
                    .insert(&txn)
                    .await?;
                }
            }
        }
    }

    if let Some(ref label_ids) = update.remove_label_ids {
        let mut unique = label_ids.clone();
        unique.sort();
        unique.dedup();
        if !unique.is_empty() {
            task_labels::Entity::delete_many()
                .filter(task_labels::Column::TaskId.eq(task_id))
                .filter(task_labels::Column::LabelId.is_in(unique))
                .exec(&txn)
                .await?;
        }
    }

    if let Some(before_labels) = before_labels {
        let after_labels = task_label_entries(&txn, task_id).await?;
        record_label_diff(&txn, task_id, Some(user_id), &before_labels, &after_labels).await?;
    }

    let linked = service::github::sync::mark_pending_push(&txn, task_id).await?;
    txn.commit().await?;
    if linked {
        crate::handlers::github::enqueue_issue_push(state, task_id).await;
    }
    Ok(())
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/",
    tag = "TaskViews",
    summary = "保存済みビュー一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    responses(
        (status = 200, description = "ビュー一覧", body = TaskViewListResponse),
        CrudErrors,
    )
)]
pub async fn list_task_views(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<TaskViewListResponse>, AppError> {
    auth.require_scope(entity::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;

    let views = project_task_views::Entity::find()
        .filter(project_task_views::Column::ProjectId.eq(project_id))
        .filter(
            Condition::any()
                .add(project_task_views::Column::CreatedBy.eq(auth.user_id))
                .add(project_task_views::Column::IsShared.eq(true)),
        )
        .order_by_asc(project_task_views::Column::Name)
        .all(&state.db)
        .await?;

    Ok(Json(TaskViewListResponse {
        views: views.into_iter().map(Into::into).collect(),
    }))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/",
    tag = "TaskViews",
    summary = "保存済みビュー作成",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
    ),
    request_body = CreateTaskViewRequest,
    responses(
        (status = 201, description = "作成されたビュー", body = ProjectTaskViewResponse),
        CrudErrors,
    )
)]
pub async fn create_task_view(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id)): Path<(Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<CreateTaskViewRequest>>,
) -> Result<(StatusCode, Json<ProjectTaskViewResponse>), AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;

    let model = project_task_views::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        created_by: Set(auth.user_id),
        name: Set(payload.name),
        is_shared: Set(payload.is_shared),
        filters: Set(payload.filters),
        sort: Set(payload.sort),
        view_type: Set(payload.view_type),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
    }
    .insert(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(model.into())))
}

#[axum::debug_handler]
#[utoipa::path(
    patch,
    path = "/{view_id}",
    tag = "TaskViews",
    summary = "保存済みビュー更新",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("view_id" = Uuid, Path, description = "ビューID"),
    ),
    request_body = UpdateTaskViewRequest,
    responses(
        (status = 200, description = "更新されたビュー", body = ProjectTaskViewResponse),
        CrudErrors,
    )
)]
pub async fn update_task_view(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, view_id)): Path<(Uuid, Uuid, Uuid)>,
    Valid(Json(payload)): Valid<Json<UpdateTaskViewRequest>>,
) -> Result<Json<ProjectTaskViewResponse>, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;

    let view = project_task_views::Entity::find_by_id(view_id)
        .filter(project_task_views::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let owner = is_tenant_owner(&state.db, tenant_id, auth.user_id).await?;
    if view.created_by != auth.user_id && !owner {
        return Err(AppError::Forbidden);
    }

    let mut active: project_task_views::ActiveModel = view.into();
    if let Some(name) = payload.name {
        active.name = Set(name);
    }
    if let Some(is_shared) = payload.is_shared {
        active.is_shared = Set(is_shared);
    }
    if let Some(filters) = payload.filters {
        active.filters = Set(filters);
    }
    if let Some(sort) = payload.sort {
        active.sort = Set(sort);
    }
    if let Some(view_type) = payload.view_type {
        active.view_type = Set(view_type);
    }
    active.updated_at = Set(chrono::Utc::now().into());

    let updated = active.update(&state.db).await?;
    Ok(Json(updated.into()))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{view_id}",
    tag = "TaskViews",
    summary = "保存済みビュー削除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("view_id" = Uuid, Path, description = "ビューID"),
    ),
    responses(
        (status = 204, description = "削除成功"),
        CrudErrors,
    )
)]
pub async fn delete_task_view(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, view_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;

    let view = project_task_views::Entity::find_by_id(view_id)
        .filter(project_task_views::Column::ProjectId.eq(project_id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    let owner = is_tenant_owner(&state.db, tenant_id, auth.user_id).await?;
    if view.created_by != auth.user_id && !owner {
        return Err(AppError::Forbidden);
    }

    project_task_views::Entity::delete_by_id(view_id)
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}/attachments",
    tag = "Tasks",
    summary = "タスク添付ファイル一覧",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID（UUID または連番ID）"),
    ),
    responses(
        (status = 200, description = "添付ファイル一覧", body = TaskAttachmentListResponse),
        CrudErrors,
    )
)]
pub async fn list_task_attachments(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<TaskAttachmentListResponse>, AppError> {
    auth.require_scope(entity::scopes::Scope::ReadTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    let rows = task_attachments::Entity::find()
        .filter(task_attachments::Column::TaskId.eq(task.id))
        .order_by_desc(task_attachments::Column::CreatedAt)
        .find_also_related(drive_files::Entity)
        .all(&state.db)
        .await?;

    let attachments = rows
        .into_iter()
        .map(|(row, file)| {
            let file = file.ok_or(AppError::NotFound)?;
            Ok(TaskAttachmentResponse {
                id: row.id,
                drive_file_id: row.drive_file_id,
                name: file.name,
                mime_type: file.mime_type,
                size: file.size,
                url: content_url(file.id),
                created_at: row.created_at.with_timezone(&Utc),
                source: row.source,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    Ok(Json(TaskAttachmentListResponse { attachments }))
}

#[axum::debug_handler]
#[utoipa::path(
    post,
    path = "/{id}/attachments",
    tag = "Tasks",
    summary = "タスクにファイルを添付",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID（UUID または連番ID）"),
    ),
    request_body = AttachFileRequest,
    responses(
        (status = 201, description = "添付完了", body = TaskAttachmentResponse),
        (
            status = 200,
            description = "本文から付いていた添付（`source = body`）を手動へ昇格した",
            body = TaskAttachmentResponse
        ),
        CrudErrors,
    )
)]
pub async fn attach_task_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id)): Path<(Uuid, Uuid, String)>,
    Valid(Json(payload)): Valid<Json<AttachFileRequest>>,
) -> Result<(StatusCode, Json<TaskAttachmentResponse>), AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    let file = drive_files::Entity::find_by_id(payload.drive_file_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if file.tenant_id != tenant_id {
        return Err(AppError::Forbidden);
    }
    if let Some(pid) = file.project_id
        && pid != project_id
    {
        return Err(AppError::Forbidden);
    }

    let existing = task_attachments::Entity::find()
        .filter(task_attachments::Column::TaskId.eq(task.id))
        .filter(task_attachments::Column::DriveFileId.eq(payload.drive_file_id))
        .one(&state.db)
        .await?;

    let (status, model) = match existing {
        // 本文から付いた行は手動へ昇格する（本文から参照が消えても残したい意思を優先）。
        // 解除できるのは昇格させた人になる
        Some(row) if row.source == task_attachments::SOURCE_BODY => {
            let promoted = task_attachments::Entity::update_many()
                .col_expr(
                    task_attachments::Column::Source,
                    Expr::value(task_attachments::SOURCE_MANUAL),
                )
                .col_expr(
                    task_attachments::Column::CreatedBy,
                    Expr::value(auth.user_id),
                )
                .filter(task_attachments::Column::Id.eq(row.id))
                .filter(task_attachments::Column::Source.eq(task_attachments::SOURCE_BODY))
                .exec(&state.db)
                .await?;
            // 本文の保存と競合して行が消えた・先に昇格された
            if promoted.rows_affected == 0 {
                return Err(AppError::Conflict);
            }
            let model = task_attachments::Model {
                source: task_attachments::SOURCE_MANUAL.to_string(),
                created_by: auth.user_id,
                ..row
            };
            (StatusCode::OK, model)
        }
        Some(_) => return Err(AppError::Conflict),
        None => {
            let model = match (task_attachments::ActiveModel {
                id: Set(Uuid::new_v4()),
                task_id: Set(task.id),
                drive_file_id: Set(payload.drive_file_id),
                created_by: Set(auth.user_id),
                created_at: Set(chrono::Utc::now().into()),
                source: Set(task_attachments::SOURCE_MANUAL.to_string()),
            })
            .insert(&state.db)
            .await
            {
                Ok(model) => model,
                Err(e) if is_postgres_unique_violation(&e) => return Err(AppError::Conflict),
                Err(e) => return Err(e.into()),
            };
            (StatusCode::CREATED, model)
        }
    };

    Ok((
        status,
        Json(TaskAttachmentResponse {
            id: model.id,
            drive_file_id: file.id,
            name: file.name,
            mime_type: file.mime_type,
            size: file.size,
            url: content_url(file.id),
            created_at: model.created_at.with_timezone(&Utc),
            source: model.source,
        }),
    ))
}

#[axum::debug_handler]
#[utoipa::path(
    delete,
    path = "/{id}/attachments/{attachment_id}",
    tag = "Tasks",
    summary = "タスク添付を解除",
    params(
        ("tenant_id" = Uuid, Path, description = "テナントID"),
        ("project_id" = Uuid, Path, description = "プロジェクトID"),
        ("id" = String, Path, description = "タスクID（UUID または連番ID）"),
        ("attachment_id" = Uuid, Path, description = "添付ID"),
    ),
    responses(
        (status = 204, description = "解除成功"),
        (
            status = 409,
            description = "本文から抽出した参照（`source = body`）は本文を編集して外す",
            body = ServerError
        ),
        CrudErrors,
    )
)]
pub async fn detach_task_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((tenant_id, project_id, id, attachment_id)): Path<(Uuid, Uuid, String, Uuid)>,
) -> Result<StatusCode, AppError> {
    auth.require_scope(entity::scopes::Scope::WriteTask)?;
    auth.ensure_tenant_access(&state, tenant_id, Some(project_id))
        .await?;
    let task = resolve_task(&state, tenant_id, project_id, &id).await?;

    let attachment = task_attachments::Entity::find()
        .filter(task_attachments::Column::Id.eq(attachment_id))
        .filter(task_attachments::Column::TaskId.eq(task.id))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // ここで消しても次の本文保存で付き直すので、本文側で外させる
    if attachment.source == task_attachments::SOURCE_BODY {
        return Err(AppError::ConflictDetail(
            "本文から抽出した参照は解除できません。Drive ファイルの本文を編集して参照を外してください"
                .into(),
        ));
    }

    let owner = is_tenant_owner(&state.db, tenant_id, auth.user_id).await?;
    if attachment.created_by != auth.user_id && !owner {
        return Err(AppError::Forbidden);
    }

    task_attachments::Entity::delete_by_id(attachment.id)
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ilike_pattern_matches_wildcards_and_backslashes_literally() {
        assert_eq!(ilike_pattern("50%_\\"), "%50\\%\\_\\\\%");
        assert_eq!(ilike_pattern("@alice 仕様"), "%@alice 仕様%");
    }

    #[test]
    fn mention_names_takes_words_after_at_sign() {
        assert_eq!(
            mention_names("@alice 仕様 @Bob @ メール@x"),
            vec!["alice".to_string(), "Bob".to_string()]
        );
        assert!(mention_names("@ 仕様").is_empty());
    }

    #[test]
    fn tsvector_match_numbers_values_after_query() {
        let id = Uuid::nil();
        let mentions = MentionSearch {
            terms: vec![("alice".into(), vec![id])],
            rest: "仕様".into(),
        };
        let (clause, values) = tsvector_match(Uuid::nil(), "@alice 仕様", Some(&mentions));
        // $1 project_id、$2 検索語の全体、$3 そのパターン、$4 $5 `@alice` の語とそのパターン、
        // $6 印、$7 $8 残りの語とそのパターン
        assert_eq!(values.len(), 8);
        assert!(clause.contains("plainto_tsquery('pg_catalog.simple', $4)"));
        assert!(clause.contains("title ILIKE $5"));
        assert!(clause.contains("$4 <% title"));
        assert!(clause.contains("LIKE $6"));
        assert!(clause.contains("plainto_tsquery('pg_catalog.simple', $7)"));
        assert_eq!(values[4], sea_orm::Value::from("%@alice%".to_string()));
        assert_eq!(values[5], sea_orm::Value::from(format!("%<@{id}>%")));
        assert_eq!(values[7], sea_orm::Value::from("%仕様%".to_string()));
    }

    #[test]
    fn tsvector_match_without_mentions_keeps_plain_query() {
        let (clause, values) = tsvector_match(Uuid::nil(), "仕様", None);
        assert_eq!(values.len(), 3);
        assert_eq!(values[2], sea_orm::Value::from("%仕様%".to_string()));
        assert!(!clause.contains("description, '') LIKE"));
        assert!(clause.contains("search_vector @@ tsq"));
    }

    #[test]
    fn highlight_falls_back_to_each_word() {
        let highlight = highlight_ilike("題", Some("@alice さんに頼む"), "@alice 仕様");
        assert!(highlight.contains("@alice"), "{highlight}");
    }

    #[test]
    fn find_match_char_range_is_none_when_the_query_is_longer_than_the_text() {
        assert_eq!(find_match_char_range("T @alice", "@alice @alice"), None);
        assert_eq!(find_match_char_range("", "a"), None);
        assert_eq!(find_match_char_range("あ", "あい"), None);
    }

    #[test]
    fn find_match_char_range_still_finds_a_query_as_long_as_the_text() {
        assert_eq!(find_match_char_range("@alice", "@ALICE"), Some((0, 6)));
        assert_eq!(find_match_char_range("T @alice", "@alice"), Some((2, 8)));
    }

    #[test]
    fn highlight_does_not_panic_when_the_whole_query_is_longer_than_the_text() {
        // 説明文が印だけのタスクを `@alice @alice` で探した時。印を戻した表示本文 `T @alice`
        // は 8 文字で、検索語の全体は 13 文字ある。語ごとの探しへ進めること。
        let highlight = highlight_ilike("T", Some("@alice"), "@alice @alice");
        assert!(highlight.contains("<em>@alice</em>"), "{highlight}");
    }

    #[test]
    fn highlight_returns_the_plain_text_when_a_long_query_matches_nothing() {
        let highlight = highlight_ilike("題", Some("説明"), "どこにも無い長い検索語");
        assert_eq!(highlight, "題 説明");
    }

    #[test]
    fn find_match_char_range_returns_none_when_query_is_longer_than_text() {
        // 範囲外スライスで panic していた（検索語 8 文字 > 対象 4 文字）
        assert_eq!(find_match_char_range("a b ", "a      b"), None);
        assert_eq!(find_match_char_range("", "a"), None);
    }

    #[test]
    fn find_match_char_range_matches_by_char_index() {
        assert_eq!(
            find_match_char_range("認証ログイン画面", "ログイン"),
            Some((2, 6))
        );
        assert_eq!(find_match_char_range("OAuth", "oauth"), Some((0, 5)));
        assert_eq!(find_match_char_range("abc", "abc"), Some((0, 3)));
        assert_eq!(find_match_char_range("abc", "abd"), None);
    }
}
