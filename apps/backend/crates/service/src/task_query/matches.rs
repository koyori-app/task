use super::{filter::ilike_pattern, resolve::ResolvedTaskQuery};
use common::error::AppError;
use entity::{task_comments, users};
use payload::{task_query::*, tasks::TaskResponse, users::UserSummary};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult, QueryFilter, Statement,
};
use std::collections::HashMap;
use uuid::Uuid;

pub const SNIPPET_BEFORE: usize = 24;
pub const SNIPPET_AFTER: usize = 96;
fn hit_mask(text: &[char], terms: &[&str]) -> Vec<bool> {
    let mut mask = vec![false; text.len()];
    for term in terms {
        let chars: Vec<char> = term.chars().collect();
        if chars.is_empty() || chars.len() > text.len() {
            continue;
        }
        for i in 0..=text.len() - chars.len() {
            if text[i..i + chars.len()]
                .iter()
                .zip(&chars)
                .all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()))
            {
                mask[i..i + chars.len()].fill(true);
            }
        }
    }
    mask
}
fn segments(chars: &[char], mask: &[bool]) -> Vec<MatchSegment> {
    let mut result: Vec<MatchSegment> = Vec::new();
    for (c, hit) in chars.iter().zip(mask) {
        if let Some(last) = result.last_mut()
            && last.hit == *hit
        {
            last.text.push(*c);
            continue;
        }
        result.push(MatchSegment {
            text: c.to_string(),
            hit: *hit,
        });
    }
    result
}
pub fn title_segments(text: &str, terms: &[&str]) -> Option<Vec<MatchSegment>> {
    let chars: Vec<char> = text.chars().collect();
    let mask = hit_mask(&chars, terms);
    mask.iter().any(|hit| *hit).then(|| segments(&chars, &mask))
}
pub fn snippet(text: &str, terms: &[&str]) -> Option<(Vec<MatchSegment>, bool, bool)> {
    let chars: Vec<char> = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let mask = hit_mask(&chars, terms);
    let first = mask.iter().position(|hit| *hit)?;
    let start = first.saturating_sub(SNIPPET_BEFORE);
    let end = (first + SNIPPET_AFTER).min(chars.len());
    Some((
        segments(&chars[start..end], &mask[start..end]),
        start > 0,
        end < chars.len(),
    ))
}
pub async fn for_tasks<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    resolved: &ResolvedTaskQuery,
    tasks: &[TaskResponse],
) -> Result<HashMap<Uuid, TaskSearchMatch>, AppError> {
    let terms: Vec<&str> = resolved
        .texts
        .iter()
        .filter(|t| !t.negated)
        .map(|t| t.text.as_str())
        .collect();
    let mut result = HashMap::new();
    if terms.is_empty() || tasks.is_empty() {
        return Ok(result);
    }
    let comments = if resolved.scope.comment {
        let mut values: Vec<sea_orm::Value> = tasks.iter().map(|t| t.id.into()).collect();
        let ids = (1..=values.len())
            .map(|i| format!("${i}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut predicates = Vec::new();
        for term in resolved.texts.iter().filter(|t| !t.negated) {
            values.push(ilike_pattern(&term.text).into());
            predicates.push(format!("c.body ILIKE ${} ESCAPE '\\'", values.len()));
            for id in &term.mention_ids {
                values.push(format!("%<@{id}>%").into());
                predicates.push(format!("c.body LIKE ${}", values.len()));
            }
        }
        task_comments::Model::find_by_statement(Statement::from_sql_and_values(DbBackend::Postgres, format!("SELECT DISTINCT ON (c.task_id) c.* FROM task_comments c WHERE c.deleted_at IS NULL AND c.task_id IN ({ids}) AND ({}) ORDER BY c.task_id, c.created_at DESC, c.id DESC", predicates.join(" OR ")), values)).all(db).await?
    } else {
        Vec::new()
    };
    let mentions = crate::mentions::resolve_in_bodies(
        db,
        project_id,
        tasks
            .iter()
            .filter_map(|t| t.description.as_deref())
            .chain(comments.iter().map(|c| c.body.as_str())),
    )
    .await?;
    let authors: HashMap<Uuid, UserSummary> = users::Entity::find()
        .filter(users::Column::Id.is_in(comments.iter().map(|c| c.user_id)))
        .all(db)
        .await?
        .into_iter()
        .map(|u| (u.id, u.into()))
        .collect();
    for task in tasks {
        let title = if resolved.scope.title {
            title_segments(&task.title, &terms)
        } else {
            None
        };
        let mut snippets = Vec::new();
        if resolved.scope.description
            && let Some(body) = &task.description
            && let Some((segments, truncated_start, truncated_end)) =
                snippet(&payload::mentions::to_plain(body, &mentions), &terms)
        {
            snippets.push(MatchSnippet {
                source: MatchSource::Description.into(),
                segments,
                truncated_start,
                truncated_end,
                comment: None,
            });
        }
        if let Some(c) = comments.iter().find(|c| c.task_id == task.id)
            && let Some((segments, truncated_start, truncated_end)) =
                snippet(&payload::mentions::to_plain(&c.body, &mentions), &terms)
        {
            snippets.push(MatchSnippet {
                source: MatchSource::Comment.into(),
                segments,
                truncated_start,
                truncated_end,
                comment: Some(MatchComment {
                    id: c.id,
                    author: authors.get(&c.user_id).cloned(),
                    created_at: c.created_at.into(),
                }),
            });
        }
        if title.is_some() || !snippets.is_empty() {
            result.insert(task.id, TaskSearchMatch { title, snippets });
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_and_unicode_matches() {
        assert_eq!(
            title_segments("Ababa🙂", &["aba", "BAB"]).unwrap(),
            vec![
                MatchSegment {
                    text: "Ababa".into(),
                    hit: true
                },
                MatchSegment {
                    text: "🙂".into(),
                    hit: false
                }
            ]
        );
        assert!(title_segments("abc", &["no"]).is_none());
        let text = format!("{}通知\n{}", "🙂".repeat(30), "か".repeat(120));
        let (s, before, after) = snippet(&text, &["通知"]).unwrap();
        assert!(before && after);
        assert!(s.iter().any(|s| s.hit));
        assert!(!s.iter().any(|s| s.text.contains('\n')));
    }
}
