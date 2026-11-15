use super::lex::{Key, Token, lex};
use entity::tasks::TaskPriority;
use payload::task_query::{TaskQueryError, TaskQueryErrorCode as Code};

pub const MAX_QUERY_CHARS: usize = 1000;
pub const MAX_QUERY_TERMS: usize = 30;
#[derive(Debug)]
pub struct TextTerm {
    pub text: String,
    pub negated: bool,
}
#[derive(Debug, Clone, Copy)]
pub struct SearchScope {
    pub title: bool,
    pub description: bool,
    pub comment: bool,
}
#[derive(Debug, Clone, Copy)]
pub enum SortField {
    Created,
    Updated,
    Title,
    Assignee,
    Priority,
    Due,
    Key,
    Status,
}
#[derive(Debug, Clone, Copy)]
pub struct SortSpec {
    pub field: SortField,
    pub desc: bool,
}
#[derive(Debug, Clone, Copy)]
pub enum DueValue {
    Overdue,
    Within(u16),
}
#[derive(Debug)]
pub enum FilterValue {
    Names(Vec<String>),
    Priorities(Vec<TaskPriority>),
    Due(Vec<DueValue>),
    NoAssignee,
    NoLabel,
    Subtask,
}
#[derive(Debug)]
pub struct FilterTerm {
    pub key: Key,
    pub value: FilterValue,
    pub negated: bool,
    pub raw: String,
}
#[derive(Debug)]
pub struct ParsedTaskQuery {
    pub texts: Vec<TextTerm>,
    pub filters: Vec<FilterTerm>,
    pub scope: SearchScope,
    pub sort: Option<SortSpec>,
    pub archived: Option<bool>,
}
pub fn error(code: Code, key: Option<Key>, value: Option<String>, term: String) -> TaskQueryError {
    TaskQueryError {
        code: code.into(),
        key: key.map(|k| format!("{k:?}").to_ascii_lowercase()),
        value,
        term,
    }
}
pub fn sort_spec(value: &str) -> Option<SortSpec> {
    let (field, explicit) = if let Some(f) = value.strip_suffix("-asc") {
        (f, Some(false))
    } else if let Some(f) = value.strip_suffix("-desc") {
        (f, Some(true))
    } else {
        (value, None)
    };
    let field = match field {
        "created" => SortField::Created,
        "updated" => SortField::Updated,
        "title" => SortField::Title,
        "assignee" => SortField::Assignee,
        "priority" => SortField::Priority,
        "due" => SortField::Due,
        "key" => SortField::Key,
        "status" => SortField::Status,
        _ => return None,
    };
    let time = matches!(field, SortField::Created | SortField::Updated);
    if time && explicit == Some(false) {
        return None;
    }
    Some(SortSpec {
        field,
        desc: explicit.unwrap_or(time),
    })
}
pub fn parse(input: &str) -> Result<ParsedTaskQuery, Vec<TaskQueryError>> {
    let tokens = lex(input);
    let mut errors = Vec::new();
    if input.chars().count() > MAX_QUERY_CHARS || tokens.len() > MAX_QUERY_TERMS {
        errors.push(error(Code::TooLong, None, None, input.into()));
    }
    let mut parsed = ParsedTaskQuery {
        texts: vec![],
        filters: vec![],
        scope: SearchScope {
            title: true,
            description: true,
            comment: true,
        },
        sort: None,
        archived: None,
    };
    let mut scoped = false;
    let mut sorts = 0;
    for token in tokens {
        let Token::Qualifier {
            key,
            values,
            negated,
            raw,
        } = token
        else {
            let (Token::Word { text, negated } | Token::Phrase { text, negated }) = token else {
                unreachable!()
            };
            if !text.is_empty() {
                parsed.texts.push(TextTerm { text, negated });
            }
            continue;
        };
        if matches!(key, Key::In | Key::Sort) && negated {
            errors.push(error(Code::NotNegatable, Some(key), None, raw.clone()));
        }
        if key == Key::Sort {
            sorts += 1;
            if sorts > 1 {
                errors.push(error(Code::DuplicateSort, Some(key), None, raw.clone()));
            }
        }
        if matches!(key, Key::No | Key::Is | Key::Sort) && values.len() != 1 {
            errors.push(error(
                Code::InvalidValue,
                Some(key),
                Some(values.join(",")),
                raw.clone(),
            ));
        }
        let mut priorities = Vec::new();
        let mut dues = Vec::new();
        for value in &values {
            if value.is_empty() {
                errors.push(error(
                    Code::EmptyValue,
                    Some(key),
                    Some(value.clone()),
                    raw.clone(),
                ));
                continue;
            }
            let lower = value.to_ascii_lowercase();
            let valid = match key {
                Key::Status | Key::Assignee | Key::Label | Key::Author => true,
                Key::Priority => {
                    if let Some(p) = match lower.as_str() {
                        "critical_fire" => Some(TaskPriority::CriticalFire),
                        "critical" => Some(TaskPriority::Critical),
                        "high" => Some(TaskPriority::High),
                        "medium" => Some(TaskPriority::Medium),
                        "low" => Some(TaskPriority::Low),
                        "trivial" => Some(TaskPriority::Trivial),
                        _ => None,
                    } {
                        priorities.push(p);
                        true
                    } else {
                        false
                    }
                }
                Key::Due => {
                    if lower == "overdue" {
                        dues.push(DueValue::Overdue);
                        true
                    } else if let Some(n) = lower
                        .strip_prefix('<')
                        .and_then(|s| s.strip_suffix('d'))
                        .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
                        .and_then(|s| s.parse::<u16>().ok())
                        .filter(|n| (1..=365).contains(n))
                    {
                        dues.push(DueValue::Within(n));
                        true
                    } else {
                        false
                    }
                }
                Key::No => matches!(lower.as_str(), "assignee" | "label"),
                Key::Is => matches!(lower.as_str(), "archived" | "subtask"),
                Key::In => matches!(lower.as_str(), "title" | "comment"),
                Key::Sort => sort_spec(&lower).is_some(),
            };
            if !valid {
                errors.push(error(
                    Code::InvalidValue,
                    Some(key),
                    Some(value.clone()),
                    raw.clone(),
                ));
            }
        }
        let value = match key {
            Key::Status | Key::Assignee | Key::Label | Key::Author => FilterValue::Names(values),
            Key::Priority => FilterValue::Priorities(priorities),
            Key::Due => FilterValue::Due(dues),
            Key::No => {
                if values[0].eq_ignore_ascii_case("assignee") {
                    FilterValue::NoAssignee
                } else {
                    FilterValue::NoLabel
                }
            }
            Key::Is => {
                if values[0].eq_ignore_ascii_case("archived") {
                    parsed.archived = Some(!negated);
                    continue;
                }
                FilterValue::Subtask
            }
            Key::In => {
                if !scoped {
                    parsed.scope = SearchScope {
                        title: false,
                        description: false,
                        comment: false,
                    };
                    scoped = true;
                }
                for v in values {
                    parsed.scope.title |= v.eq_ignore_ascii_case("title");
                    parsed.scope.comment |= v.eq_ignore_ascii_case("comment");
                }
                continue;
            }
            Key::Sort => {
                parsed.sort = sort_spec(&values[0].to_ascii_lowercase());
                continue;
            }
        };
        parsed.filters.push(FilterTerm {
            key,
            value,
            negated,
            raw,
        });
    }
    if errors.is_empty() {
        Ok(parsed)
    } else {
        Err(errors)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_and_invalid_values_and_boundaries() {
        for q in [
            "status:\"In Progress\" -label:保留 通知",
            "priority:HIGH,low",
            "due:<365d",
            "in:title,comment",
            "sort:key-desc",
            "sort:status",
            "author:@me",
            "is:archived",
        ] {
            assert!(parse(q).is_ok(), "{q}");
        }
        for q in [
            "priority:hgih",
            "due:<0d",
            "due:<366d",
            "due:<+1d",
            "no:x",
            "is:archived,subtask",
            "in:description",
            "sort:created-asc",
            "sort:updated-asc",
            "label:",
            "-in:title",
            "-sort:title",
            "sort:title sort:due",
        ] {
            assert!(parse(q).is_err(), "{q}");
        }
        assert!(parse(&"あ".repeat(1000)).is_ok());
        assert!(parse(&"あ".repeat(1001)).is_err());
        assert!(parse(&vec!["a"; 30].join(" ")).is_ok());
        assert!(parse(&vec!["a"; 31].join(" ")).is_err());
        assert_eq!(
            parse("label: priority:hgih -sort:title sort:due")
                .unwrap_err()
                .len(),
            4
        );
    }
}
