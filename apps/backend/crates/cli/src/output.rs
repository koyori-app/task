//! 出力。`--json` は機械向け、既定は人間向けの 1 行表示。

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputOptions {
    pub json: bool,
}

impl OutputOptions {
    pub fn new(json: bool) -> Self {
        Self { json }
    }
}

/// `--json` なら型どおりの JSON、そうでなければ人間向けに畳んで出す。
pub fn print<T: Serialize>(value: &T, opts: OutputOptions) {
    println!("{}", render(value, opts));
}

pub fn render<T: Serialize>(value: &T, opts: OutputOptions) -> String {
    if opts.json {
        // 型の宣言順をそのまま出す。フィールド順が変わると差分の読み取りが荒れる
        return serde_json::to_string_pretty(value).unwrap_or_else(|err| err.to_string());
    }
    let value = serde_json::to_value(value).unwrap_or(Value::Null);
    let mut lines = Vec::new();
    render_human(&value, &mut lines);
    lines.join("\n")
}

/// 一覧はキーと名前だけの 1 行に畳み、畳めないものは JSON のまま見せる。
fn render_human(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Null => {}
        Value::Array(items) => {
            for item in items {
                render_human(item, out);
            }
        }
        Value::Object(map) => {
            if let Some(Value::Array(tasks)) = map.get("tasks") {
                for task in tasks {
                    render_human(task, out);
                }
                return;
            }
            if let Some(Value::Array(notifications)) = map.get("notifications") {
                for notification in notifications {
                    render_human(notification, out);
                }
                return;
            }
            if map.contains_key("notification_type") && map.contains_key("created_at") {
                out.push(format_notification(map));
                return;
            }
            if map.contains_key("seq_key") && map.contains_key("title") {
                // `seq_key` が null の行もあるので、その場合は id へ落とす
                let key = map
                    .get("seq_key")
                    .filter(|value| !value.is_null())
                    .or_else(|| map.get("id"));
                out.push(format!("{}\t{}", as_text(key), as_text(map.get("title"))));
                return;
            }
            if map.contains_key("key") && map.contains_key("name") {
                out.push(format!(
                    "{}\t{}",
                    as_text(map.get("key")),
                    as_text(map.get("name"))
                ));
                return;
            }
            out.push(serde_json::to_string_pretty(value).unwrap_or_default());
        }
        other => out.push(as_text(Some(other))),
    }
}

/// 通知 1 件を 1 行にする: `{ID}\t{未読|既読}\t{日時}\t{種別}\t{対象}\t{要約}`。
///
/// 対象と要約は種別ごとに形が違う（タスクの通知はタスク、レビューの通知は PR）。
/// 畳めない種別は payload をそのまま見せて、内容を落とさない。
fn format_notification(map: &serde_json::Map<String, Value>) -> String {
    let read = match map.get("read_at") {
        None | Some(Value::Null) => "未読",
        Some(_) => "既読",
    };
    let kind = as_text(map.get("notification_type"));
    let payload = map.get("payload").and_then(Value::as_object);
    format!(
        "{}\t{read}\t{}\t{kind}\t{}\t{}",
        as_text(map.get("id")),
        as_text(map.get("created_at")),
        notification_target(map, payload),
        notification_summary(&kind, payload),
    )
}

/// 何についての通知か。タスクの通知はタスク、レビューの通知は PR を指す。
fn notification_target(
    map: &serde_json::Map<String, Value>,
    payload: Option<&serde_json::Map<String, Value>>,
) -> String {
    if let Some(task) = map.get("task").and_then(Value::as_object) {
        return format!(
            "#{} {}",
            as_text(task.get("seq_id")),
            as_text(task.get("title"))
        );
    }
    let Some(payload) = payload else {
        return String::new();
    };
    let pr = match payload.get("pr_number") {
        None | Some(Value::Null) => return String::new(),
        Some(pr) => as_text(Some(pr)),
    };
    let mut target = format!("PR #{pr}");
    let repo = as_text(payload.get("repo"));
    if !repo.is_empty() {
        target.push_str(&format!(" @ {repo}"));
    }
    let round = as_text(payload.get("round"));
    if !round.is_empty() {
        target.push_str(&format!(" R{round}"));
    }
    target
}

fn notification_summary(kind: &str, payload: Option<&serde_json::Map<String, Value>>) -> String {
    let Some(payload) = payload else {
        return String::new();
    };
    match kind {
        "review_round_created" => {
            let counts = payload.get("counts").and_then(Value::as_object);
            let count = |severity: &str| match counts {
                Some(counts) => as_text(counts.get(severity)),
                None => "0".into(),
            };
            format!(
                "{} high={} medium={} low={} nit={}",
                as_text(payload.get("reviewer")),
                count("high"),
                count("medium"),
                count("low"),
                count("nit"),
            )
        }
        "review_finding_changed" => format!(
            "{} {}→{}",
            as_text(payload.get("title")),
            as_text(payload.get("from")),
            as_text(payload.get("to")),
        ),
        "assigned" => as_text(payload.get("assigned_by")),
        // 畳み方を決めていない種別は、payload をそのまま 1 行で出す
        _ => serde_json::to_string(payload).unwrap_or_default(),
    }
}

/// JSON の値を「表示用の文字列」にする。文字列は引用符を外す。
fn as_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn human(value: serde_json::Value) -> String {
        render(&value, OutputOptions::new(false))
    }

    #[test]
    fn prints_pretty_json_when_asked_for_json() {
        assert_eq!(
            render(&json!({ "b": 2 }), OutputOptions::new(true)),
            "{\n  \"b\": 2\n}"
        );
    }

    #[test]
    fn folds_a_task_into_key_and_title() {
        assert_eq!(
            human(json!({ "seq_key": "APP-7", "title": "Ship it" })),
            "APP-7\tShip it"
        );
    }

    #[test]
    fn falls_back_to_the_id_when_a_task_has_no_sequence_key() {
        assert_eq!(
            human(json!({ "seq_key": null, "id": "task-1", "title": "T" })),
            "task-1\tT"
        );
    }

    #[test]
    fn folds_a_named_resource_into_key_and_name() {
        assert_eq!(human(json!({ "key": "APP", "name": "App" })), "APP\tApp");
    }

    #[test]
    fn unwraps_a_task_list_into_one_line_per_task() {
        let listing = json!({
            "tasks": [
                { "seq_key": "APP-1", "title": "First" },
                { "seq_key": "APP-2", "title": "Second" },
            ],
            "total": 2,
        });
        assert_eq!(human(listing), "APP-1\tFirst\nAPP-2\tSecond");
    }

    /// `projects statuses` の人間向け出力。名前の配列を渡すと 1 行ずつになる。
    /// オブジェクトで包むと畳めず（特別扱いは `tasks` だけ）、id や日時まで出てしまう。
    #[test]
    fn renders_a_string_list_as_one_line_each() {
        let names = json!(["Backlog", "Todo", "In Progress"]);
        assert_eq!(human(names), "Backlog\nTodo\nIn Progress");
    }

    /// 畳めないオブジェクトで包むと用途に合わない出力になることの裏取り。
    /// これが `projects statuses` の通常出力で起きていた。
    #[test]
    fn does_not_fold_a_statuses_object() {
        let listing = json!({ "statuses": [{ "name": "Todo", "position": 0 }] });
        assert!(
            human(listing).starts_with('{'),
            "オブジェクトは JSON のまま出る"
        );
    }

    /// 通知の一覧は 1 件 1 行に畳む。タスクの通知は `#{seq_id} {title}` を指す。
    #[test]
    fn folds_a_task_notification_into_one_line() {
        let listing = json!({
            "unread_count": 1,
            "notifications": [{
                "id": "ca4c4e72-189d-456c-844d-b62eaa978ec5",
                "notification_type": "assigned",
                "project_id": "p-1",
                "task": { "id": "t-1", "seq_id": 42, "title": "OAuth 対応" },
                "payload": { "assigned_by": "yupix", "role": "primary" },
                "read_at": null,
                "created_at": "2026-05-27T10:00:00Z",
            }],
        });
        assert_eq!(
            human(listing),
            "ca4c4e72-189d-456c-844d-b62eaa978ec5\t未読\t2026-05-27T10:00:00Z\tassigned\t#42 OAuth 対応\tyupix"
        );
    }

    /// レビューの通知はタスクに紐づかないので、対象は payload の PR から組む。
    #[test]
    fn folds_a_review_notification_into_the_pull_request_it_is_about() {
        let listing = json!({
            "unread_count": 0,
            "notifications": [{
                "id": "n-2",
                "notification_type": "review_round_created",
                "project_id": "p-1",
                "task": null,
                "payload": {
                    "repo": "koyori-app/task",
                    "pr_number": 618,
                    "round": 2,
                    "reviewer": "yupix",
                    "counts": { "high": 1, "medium": 2, "low": 0, "nit": 0 },
                },
                "read_at": "2026-05-27T11:00:00Z",
                "created_at": "2026-05-27T10:00:00Z",
            }],
        });
        assert_eq!(
            human(listing),
            "n-2\t既読\t2026-05-27T10:00:00Z\treview_round_created\tPR #618 @ koyori-app/task R2\tyupix high=1 medium=2 low=0 nit=0"
        );
    }

    /// 畳み方を決めていない種別でも、payload を落とさず 1 行に収める。
    #[test]
    fn keeps_the_payload_of_a_notification_type_without_a_summary() {
        let line = human(json!({
            "id": "n-3",
            "notification_type": "pr_merged",
            "task": null,
            "payload": { "pr_number": null, "merged_by": "yupix" },
            "read_at": null,
            "created_at": "2026-05-27T10:00:00Z",
        }));
        // payload の項目の並びは serde_json の持ち方次第なので、行の形と中身だけ見る
        assert!(
            line.starts_with("n-3\t未読\t2026-05-27T10:00:00Z\tpr_merged\t\t{"),
            "{line}"
        );
        assert!(line.contains("\"merged_by\":\"yupix\""), "{line}");
    }

    #[test]
    fn keeps_json_for_objects_that_do_not_fold() {
        assert_eq!(
            human(json!({ "username": "yupix" })),
            "{\n  \"username\": \"yupix\"\n}"
        );
    }

    #[test]
    fn prints_an_empty_listing_as_nothing() {
        assert_eq!(human(json!({ "tasks": [], "total": 0 })), "");
        assert_eq!(human(json!([])), "");
    }

    #[test]
    fn prints_scalars_without_quoting_them() {
        assert_eq!(human(json!("done")), "done");
        assert_eq!(human(json!(7)), "7");
    }
}
