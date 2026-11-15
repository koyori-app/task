use super::{
    lex::Key,
    parse::{self, FilterValue, ParsedTaskQuery, SearchScope, SortSpec},
};
use common::error::AppError;
use entity::{labels, project_statuses};
use payload::task_query::{TaskQueryError, TaskQueryErrorCode};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use std::collections::HashMap;
use uuid::Uuid;

pub struct ResolveContext {
    pub project_id: Uuid,
    pub viewer_id: Uuid,
}
pub struct ResolvedText {
    pub text: String,
    pub negated: bool,
    pub mention_ids: Vec<Uuid>,
}
pub enum ResolvedValue {
    Ids(Vec<Uuid>),
    Other(FilterValue),
}
pub struct ResolvedFilter {
    pub key: Key,
    pub value: ResolvedValue,
    pub negated: bool,
}
pub struct ResolvedTaskQuery {
    pub texts: Vec<ResolvedText>,
    pub filters: Vec<ResolvedFilter>,
    pub scope: SearchScope,
    pub sort: Option<SortSpec>,
    pub archived: Option<bool>,
}
impl ResolvedTaskQuery {
    pub fn has_positive_text(&self) -> bool {
        self.texts.iter().any(|t| !t.negated)
    }
}
pub async fn resolve<C: ConnectionTrait>(
    db: &C,
    ctx: &ResolveContext,
    parsed: ParsedTaskQuery,
) -> Result<Result<ResolvedTaskQuery, Vec<TaskQueryError>>, AppError> {
    let mut statuses: HashMap<String, Vec<Uuid>> = HashMap::new();
    let mut label_names: HashMap<String, Vec<Uuid>> = HashMap::new();
    if parsed.filters.iter().any(|f| f.key == Key::Status) {
        for s in project_statuses::Entity::find()
            .filter(project_statuses::Column::ProjectId.eq(ctx.project_id))
            .all(db)
            .await?
        {
            statuses
                .entry(s.name.to_lowercase())
                .or_default()
                .push(s.id);
        }
    }
    if parsed.filters.iter().any(|f| f.key == Key::Label) {
        for l in labels::Entity::find()
            .filter(labels::Column::ProjectId.eq(ctx.project_id))
            .all(db)
            .await?
        {
            label_names
                .entry(l.name.to_lowercase())
                .or_default()
                .push(l.id);
        }
    }
    let mut names = Vec::new();
    for f in &parsed.filters {
        if matches!(f.key, Key::Assignee | Key::Author)
            && let FilterValue::Names(values) = &f.value
        {
            names.extend(
                values
                    .iter()
                    .map(|v| v.strip_prefix('@').unwrap_or(v).to_string()),
            );
        }
    }
    for t in &parsed.texts {
        if let Some(name) = t.text.strip_prefix('@') {
            names.push(name.to_string());
        }
    }
    let users = crate::mentions::resolve_names(db, ctx.project_id, &names).await?;
    let mut errors = Vec::new();
    let mut filters = Vec::new();
    for f in parsed.filters {
        let value = if let FilterValue::Names(values) = f.value {
            let mut ids = Vec::new();
            for value in values {
                let name = if matches!(f.key, Key::Assignee | Key::Author) {
                    value.strip_prefix('@').unwrap_or(&value)
                } else {
                    &value
                };
                let found = if matches!(f.key, Key::Assignee | Key::Author)
                    && name.eq_ignore_ascii_case("me")
                    && value.starts_with('@')
                {
                    Some(vec![ctx.viewer_id])
                } else {
                    let map = match f.key {
                        Key::Status => &statuses,
                        Key::Label => &label_names,
                        _ => &users,
                    };
                    map.get(&name.to_lowercase()).cloned()
                };
                if let Some(found) = found {
                    ids.extend(found);
                } else {
                    errors.push(parse::error(
                        TaskQueryErrorCode::UnknownValue,
                        Some(f.key),
                        Some(value),
                        f.raw.clone(),
                    ));
                }
            }
            ResolvedValue::Ids(ids)
        } else {
            ResolvedValue::Other(f.value)
        };
        filters.push(ResolvedFilter {
            key: f.key,
            value,
            negated: f.negated,
        });
    }
    let texts = parsed
        .texts
        .into_iter()
        .map(|t| {
            let mention_ids = t
                .text
                .strip_prefix('@')
                .and_then(|n| users.get(&n.to_lowercase()))
                .cloned()
                .unwrap_or_default();
            ResolvedText {
                text: t.text,
                negated: t.negated,
                mention_ids,
            }
        })
        .collect();
    if !errors.is_empty() {
        return Ok(Err(errors));
    }
    Ok(Ok(ResolvedTaskQuery {
        texts,
        filters,
        scope: parsed.scope,
        sort: parsed.sort,
        archived: parsed.archived,
    }))
}
