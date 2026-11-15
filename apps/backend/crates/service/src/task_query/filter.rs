use super::{
    lex::Key,
    parse::{DueValue, FilterValue, SearchScope},
    resolve::{ResolvedTaskQuery, ResolvedText, ResolvedValue},
};
use entity::tasks;
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{ColumnTrait, Condition, Value};

/// 本文の `<@利用者ID>` の印を取り除いた式。語の ILIKE は印の中の UUID に当てない（`42`・`add` のような
/// 16 進の文字だけの語が、メンションの UUID に当たって塗りも抜粋もない行を出さないため）。
/// `@名` の検索は別経路（下の `mention_ids`）で印そのものに当てる。
pub fn strip_mentions(column: &str) -> String {
    format!("regexp_replace({column}, '<@[0-9a-fA-F-]{{36}}>', '', 'g')")
}
pub fn ilike_pattern(text: &str) -> String {
    format!("%{}%", common::db::escape_like(text))
}
pub fn text_condition(text: &ResolvedText, scope: SearchScope) -> Condition {
    let mut any = Condition::any();
    let pattern = ilike_pattern(&text.text);
    for (enabled, sql) in [
        (scope.title, "tasks.title ILIKE $1 ESCAPE '\\'".to_string()),
        (
            scope.description,
            format!(
                "{} ILIKE $1 ESCAPE '\\'",
                strip_mentions("COALESCE(tasks.description, '')")
            ),
        ),
        (
            scope.comment,
            format!(
                "EXISTS (SELECT 1 FROM task_comments c WHERE c.task_id = tasks.id AND c.deleted_at IS NULL AND {} ILIKE $1 ESCAPE '\\')",
                strip_mentions("c.body")
            ),
        ),
    ] {
        if enabled {
            any = any.add(Expr::cust_with_values(sql, [pattern.clone()]));
        }
    }
    for id in &text.mention_ids {
        let pattern = format!("%<@{id}>%");
        if scope.description {
            any = any.add(Expr::cust_with_values(
                "COALESCE(tasks.description, '') LIKE $1",
                [pattern.clone()],
            ));
        }
        if scope.comment {
            any = any.add(Expr::cust_with_values("EXISTS (SELECT 1 FROM task_comments c WHERE c.task_id = tasks.id AND c.deleted_at IS NULL AND c.body LIKE $1)", [pattern]));
        }
    }
    any
}
pub fn condition(resolved: &ResolvedTaskQuery, timezone: &str) -> Condition {
    let mut all = Condition::all();
    for t in &resolved.texts {
        let term = text_condition(t, resolved.scope);
        all = all.add(if t.negated { term.not() } else { term });
    }
    for f in &resolved.filters {
        let term = match &f.value {
            ResolvedValue::Ids(ids) => match f.key {
                Key::Status => Condition::all().add(tasks::Column::StatusId.is_in(ids.clone())),
                Key::Author => Condition::all().add(tasks::Column::CreatedBy.is_in(ids.clone())),
                Key::Label | Key::Assignee => {
                    let (table, column) = if f.key == Key::Label {
                        ("task_labels", "label_id")
                    } else {
                        ("task_assignees", "user_id")
                    };
                    let mut any = Condition::any();
                    for id in ids {
                        any = any.add(Expr::cust_with_values(format!("EXISTS (SELECT 1 FROM {table} x WHERE x.task_id = tasks.id AND x.{column} = $1)"), [*id]));
                    }
                    any
                }
                _ => unreachable!("only name conditions resolve to ids"),
            },
            ResolvedValue::Other(value) => match value {
                FilterValue::Priorities(values) => {
                    Condition::all().add(tasks::Column::Priority.is_in(values.clone()))
                }
                FilterValue::NoAssignee => Condition::all().add(Expr::cust(
                    "NOT EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = tasks.id)",
                )),
                FilterValue::NoLabel => Condition::all().add(Expr::cust(
                    "NOT EXISTS (SELECT 1 FROM task_labels l WHERE l.task_id = tasks.id)",
                )),
                FilterValue::Subtask => {
                    Condition::all().add(tasks::Column::ParentTaskId.is_not_null())
                }
                FilterValue::Due(values) => {
                    let due = crate::deadline_filter::due_date_sql("tasks");
                    let today = crate::deadline_filter::today_sql("$1");
                    let mut any = Condition::any();
                    for value in values {
                        any = any.add(match value {
                            DueValue::Overdue => Expr::cust_with_values(format!("{due} < {today} AND EXISTS (SELECT 1 FROM project_statuses s WHERE s.id = tasks.status_id AND s.is_done_state = false)"), vec![Value::from(timezone)]),
                            DueValue::Within(n) => Expr::cust_with_values(format!("{due} >= {today} AND {due} < {today} + $2::integer"), vec![Value::from(timezone), Value::from(i32::from(*n))]),
                        });
                    }
                    // NULL の期限は false に寄せる。否定（-due:overdue など）では期限なしのタスクが残る。
                    Condition::all().add(Expr::expr(any).if_null(false))
                }
                FilterValue::Names(_) => unreachable!("names are resolved first"),
            },
        };
        all = all.add(if f.negated { term.not() } else { term });
    }
    all
}
