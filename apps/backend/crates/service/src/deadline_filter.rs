//! 「今日 / 今週 / 期限超過」の判定。ホーム（dashboard）と My Tasks で共有する。
//!
//! 期限は `COALESCE(soft_deadline, hard_deadline)`（UTC 0 時の暦日として保存）。
//! 「今日」「今週」は閲覧者のタイムゾーンの暦日で、週は月曜始まり。

/// `tasks` テーブル（別名 `alias`）の期限の暦日を返す SQL 式。
pub fn due_date_sql(alias: &str) -> String {
    format!("(COALESCE({alias}.soft_deadline, {alias}.hard_deadline) AT TIME ZONE 'UTC')::date")
}

/// タイムゾーンのプレースホルダ（`$3` 等）から今日の暦日を返す SQL 式。
pub fn today_sql(tz: &str) -> String {
    format!("(now() AT TIME ZONE {tz})::date")
}

/// タイムゾーンのプレースホルダから今週の月曜の暦日を返す SQL 式。
pub fn week_start_sql(tz: &str) -> String {
    format!("date_trunc('week', now() AT TIME ZONE {tz})::date")
}

/// フィルター名に対応する期限の条件（SQL 断片）。未知の名前は `None`。
/// `today` / `week_start` は暦日を返す SQL 式。
pub fn predicate(filter: &str, due: &str, today: &str, week_start: &str) -> Option<String> {
    match filter {
        "today" => Some(format!("{due} = {today}")),
        "week" => Some(format!(
            "{due} >= {week_start} AND {due} < {week_start} + 7"
        )),
        "overdue" => Some(format!("{due} < {today}")),
        "all" => Some("TRUE".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicates_are_shared_shapes() {
        let p = |f| predicate(f, "d", "t", "w");
        assert_eq!(p("today").unwrap(), "d = t");
        assert_eq!(p("week").unwrap(), "d >= w AND d < w + 7");
        assert_eq!(p("overdue").unwrap(), "d < t");
        assert_eq!(p("all").unwrap(), "TRUE");
        assert!(p("bogus").is_none());
    }
}
