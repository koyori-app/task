//! 通知メールの件名・本文の組み立て。
//!
//! 通知の行（`notifications`）と、それが指すタスク / プロジェクト / テナントだけから
//! 組み立てる。送信は `job::notification_email` の掃き出しループが行う。

use entity::{notifications, projects, tasks, tenants};

use common::notifications::{
    TYPE_ASSIGNED, TYPE_COMMENT_ADDED, TYPE_MENTIONED, TYPE_REVIEW_FINDING_CHANGED,
    TYPE_REVIEW_ROUND_CREATED, TYPE_STATUS_CHANGED,
};

/// 件名に載せるタスクタイトルの長さ。長い件名はクライアント側で切られるので、
/// こちらで char 境界を守って切る（バイト境界で切るとタイトルが壊れる）。
const TITLE_MAX_CHARS: usize = 60;

pub struct Mail {
    pub subject: String,
    pub text: String,
    pub html: String,
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn truncate(value: &str) -> String {
    let mut out: String = value.chars().take(TITLE_MAX_CHARS).collect();
    if value.chars().count() > TITLE_MAX_CHARS {
        out.push('…');
    }
    out
}

/// payload の文字列フィールド。無ければ空文字（本題は出す）。
fn field<'a>(notification: &'a notifications::Model, key: &str) -> &'a str {
    notification
        .payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn number(notification: &notifications::Model, key: &str) -> i64 {
    notification
        .payload
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0)
}

fn severity_counts(notification: &notifications::Model) -> String {
    let counts = notification.payload.get("counts");
    let count = |key: &str| {
        counts
            .and_then(|c| c.get(key))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    format!(
        "high {} / medium {} / low {} / nit {}",
        count("high"),
        count("medium"),
        count("low"),
        count("nit")
    )
}

/// タスクの表示名（`#12 タイトル`）。タスクが引けない通知では種別だけを出す。
fn task_label(task: Option<&tasks::Model>) -> String {
    match task {
        Some(task) => format!("#{} {}", task.seq_id, truncate(&task.title)),
        None => String::new(),
    }
}

/// 画面の URL。タスク系はタスク詳細、レビュー系は指摘一覧
/// （フロントの `taskDetailHref` / 要約コメントのリンクと同じ形）。
fn link(
    notification: &notifications::Model,
    task: Option<&tasks::Model>,
    project: &projects::Model,
    tenant: &tenants::Model,
    app_url: &str,
) -> Option<String> {
    let base = app_url.trim_end_matches('/');
    if base.is_empty() {
        return None;
    }
    match notification.notification_type.as_str() {
        TYPE_REVIEW_ROUND_CREATED | TYPE_REVIEW_FINDING_CHANGED => Some(format!(
            "{base}/{}/projects/{}/reviews?pr={}",
            tenant.display_id,
            project.key,
            number(notification, "pr_number")
        )),
        _ => task.map(|task| {
            format!(
                "{base}/{}/projects/{}/tasks/{}-{}",
                tenant.display_id, project.key, project.key, task.seq_id
            )
        }),
    }
}

/// 件名と本文 1 行目。未知の種別は種別名と payload をそのまま出す
/// （送らないより、何が起きたかを伝える方がよい）。
fn subject_and_line(
    notification: &notifications::Model,
    task: Option<&tasks::Model>,
) -> (String, String) {
    let label = task_label(task);
    match notification.notification_type.as_str() {
        TYPE_ASSIGNED => (
            format!("[Koyori] {label} の担当になりました"),
            format!(
                "{} があなたを {label} の担当（{}）に追加しました。",
                field(notification, "assigned_by"),
                field(notification, "role")
            ),
        ),
        TYPE_MENTIONED => (
            format!("[Koyori] {label} でメンションされました"),
            format!(
                "{} が {label} のコメントであなたにメンションしました。",
                field(notification, "author")
            ),
        ),
        TYPE_COMMENT_ADDED => (
            format!("[Koyori] {label} にコメントが追加されました"),
            format!(
                "{} が {label} にコメントしました。",
                field(notification, "author")
            ),
        ),
        TYPE_STATUS_CHANGED => (
            format!(
                "[Koyori] {label} のステータスが {} → {}",
                field(notification, "from"),
                field(notification, "to")
            ),
            format!(
                "{} が {label} のステータスを {} から {} へ変更しました。",
                field(notification, "changed_by"),
                field(notification, "from"),
                field(notification, "to")
            ),
        ),
        TYPE_REVIEW_ROUND_CREATED => (
            format!(
                "[Koyori] PR #{} のレビュー R{}（{}）",
                number(notification, "pr_number"),
                number(notification, "round"),
                severity_counts(notification)
            ),
            format!(
                "{} が PR #{} のレビュー R{} を起票しました。\n{}",
                field(notification, "reviewer"),
                number(notification, "pr_number"),
                number(notification, "round"),
                field(notification, "summary_excerpt")
            ),
        ),
        TYPE_REVIEW_FINDING_CHANGED => (
            format!(
                "[Koyori] 指摘「{}」が {} → {}",
                truncate(field(notification, "title")),
                field(notification, "from"),
                field(notification, "to")
            ),
            format!(
                "{} が PR #{} の指摘「{}」を {} から {} へ変更しました。",
                field(notification, "actor"),
                number(notification, "pr_number"),
                truncate(field(notification, "title")),
                field(notification, "from"),
                field(notification, "to")
            ),
        ),
        other => (
            format!("[Koyori] {other}"),
            format!("{other}: {}", notification.payload),
        ),
    }
}

pub fn render(
    notification: &notifications::Model,
    task: Option<&tasks::Model>,
    project: &projects::Model,
    tenant: &tenants::Model,
    app_url: &str,
) -> Mail {
    let (subject, line) = subject_and_line(notification, task);
    let url = link(notification, task, project, tenant, app_url);

    let text = match &url {
        Some(url) => format!("{line}\n{url}\n"),
        None => format!("{line}\n"),
    };
    let html = match &url {
        Some(url) => format!(
            "<p>{}</p><p><a href=\"{}\">{}</a></p>",
            escape(&line),
            escape(url),
            escape(url)
        ),
        None => format!("<p>{}</p>", escape(&line)),
    };
    Mail {
        subject,
        text,
        html,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::prelude::Uuid;

    fn notification(notification_type: &str, payload: serde_json::Value) -> notifications::Model {
        notifications::Model {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            task_id: None,
            project_id: Some(Uuid::new_v4()),
            notification_type: notification_type.into(),
            payload,
            read_at: None,
            created_at: chrono::Utc::now().into(),
            email_queued_at: Some(chrono::Utc::now().into()),
            emailed_at: None,
            email_attempts: 0,
        }
    }

    fn task(title: &str) -> tasks::Model {
        tasks::Model {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            seq_id: 12,
            title: title.into(),
            description: None,
            status_id: Uuid::new_v4(),
            priority: entity::tasks::TaskPriority::Medium,
            progress_pct: 0,
            parent_task_id: None,
            milestone_id: None,
            soft_deadline: None,
            hard_deadline: None,
            estimated_minutes: None,
            is_archived: false,
            created_by: Uuid::new_v4(),
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
            deleted_at: None,
            sprint_id: None,
            completed_at: None,
            search_vector: None,
        }
    }

    fn project() -> projects::Model {
        projects::Model {
            id: Uuid::new_v4(),
            name: "Koyori".into(),
            description: String::new(),
            tenant_id: Uuid::new_v4(),
            icon_emoji: None,
            icon_url: None,
            key: "KOY".into(),
            is_personal: false,
            personal_owner_id: None,
        }
    }

    fn tenant() -> tenants::Model {
        tenants::Model {
            id: Uuid::new_v4(),
            display_id: "acme".into(),
            name: "Acme".into(),
            description: String::new(),
            icon_url: String::new(),
            icon_emoji: None,
            owner_id: Uuid::new_v4(),
            drive_quota_bytes: None,
            require_2fa: false,
        }
    }

    #[test]
    fn task_types_carry_seq_id_and_detail_url() {
        let task = task("OAuth 対応");
        let mail = render(
            &notification(
                TYPE_ASSIGNED,
                serde_json::json!({"assigned_by": "田中", "role": "primary"}),
            ),
            Some(&task),
            &project(),
            &tenant(),
            "http://localhost:3000/",
        );
        assert_eq!(mail.subject, "[Koyori] #12 OAuth 対応 の担当になりました");
        assert!(
            mail.text
                .contains("http://localhost:3000/acme/projects/KOY/tasks/KOY-12"),
            "text: {}",
            mail.text
        );
    }

    #[test]
    fn review_types_carry_pr_number() {
        let mail = render(
            &notification(
                TYPE_REVIEW_ROUND_CREATED,
                serde_json::json!({"pr_number": 618, "round": 2, "counts": {"high": 1, "medium": 2}}),
            ),
            None,
            &project(),
            &tenant(),
            "http://localhost:3000",
        );
        assert_eq!(
            mail.subject,
            "[Koyori] PR #618 のレビュー R2（high 1 / medium 2 / low 0 / nit 0）"
        );
        assert!(mail.text.contains("/projects/KOY/reviews?pr=618"));
    }

    /// payload が空でも、種別ごとの件名は出して落ちない。
    #[test]
    fn empty_payload_does_not_panic() {
        for notification_type in [
            TYPE_ASSIGNED,
            TYPE_MENTIONED,
            TYPE_COMMENT_ADDED,
            TYPE_STATUS_CHANGED,
            TYPE_REVIEW_ROUND_CREATED,
            TYPE_REVIEW_FINDING_CHANGED,
            "unknown_type",
        ] {
            let mail = render(
                &notification(notification_type, serde_json::json!({})),
                None,
                &project(),
                &tenant(),
                "",
            );
            assert!(mail.subject.starts_with("[Koyori]"));
            assert!(!mail.text.is_empty());
        }
    }

    /// 件名は char 境界で切る（マルチバイトのタイトルで panic しない）。
    #[test]
    fn long_multibyte_title_is_truncated_on_char_boundary() {
        let task = task(&"あ".repeat(200));
        let mail = render(
            &notification(TYPE_ASSIGNED, serde_json::json!({})),
            Some(&task),
            &project(),
            &tenant(),
            "http://localhost:3000",
        );
        assert!(mail.subject.contains(&"あ".repeat(TITLE_MAX_CHARS)));
        assert!(mail.subject.contains('…'));
    }

    /// HTML 本文に生のタグを通さない。
    #[test]
    fn html_escapes_payload() {
        let mail = render(
            &notification(
                TYPE_COMMENT_ADDED,
                serde_json::json!({"author": "<script>alert(1)</script>"}),
            ),
            Some(&task("t")),
            &project(),
            &tenant(),
            "http://localhost:3000",
        );
        assert!(!mail.html.contains("<script>"), "html: {}", mail.html);
        assert!(mail.html.contains("&lt;script&gt;"));
    }
}
