//! コミットメッセージ・PR タイトル・ブランチ名からの `KEY-N` 抽出と解決
//! （docs/features/tasks/9.github-tasks.md §4）。

use std::sync::LazyLock;

use regex::Regex;
use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, prelude::Uuid};

use entity::{projects, tasks};

/// `KEY-N` と、その直前のクローズキーワード（任意）。
///
/// 境界は ASCII の `\b` で取る。Unicode の `\b` だと日本語の直後（`修正TASK-1`）が
/// 語の途中とみなされて取りこぼす。KEY は大小文字不問で拾い、照合前に大文字へ揃える。
static TASK_REF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:(?-u:\b)(?i:(close[sd]?|fix(?:e[sd])?|resolve[sd]?|implement(?:s|ed)?))\s*:?\s+)?(?-u:\b)([A-Za-z][A-Za-z0-9]{1,9})-([0-9]+)(?-u:\b)",
    )
    .expect("task ref regex")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRef {
    /// 大文字に正規化したプロジェクトキー
    pub key: String,
    pub seq_id: i32,
    /// クローズキーワード付きで書かれていたか
    pub closes: bool,
}

/// テキスト中の参照を初出順に返す。同じ参照が複数回出たら 1 つにまとめ、
/// どれか 1 つでもクローズキーワード付きなら `closes` にする。
///
/// `SHA256-1` のようなキーでないものも拾うが、[`resolve`] で解決できずに捨てられる。
pub fn extract(text: &str) -> Vec<TaskRef> {
    let mut refs: Vec<TaskRef> = Vec::new();
    for cap in TASK_REF_RE.captures_iter(text) {
        // i32 に収まらない番号のタスクは存在しない
        let Ok(seq_id) = cap[3].parse::<i32>() else {
            continue;
        };
        let key = cap[2].to_ascii_uppercase();
        let closes = cap.get(1).is_some();
        match refs.iter_mut().find(|r| r.key == key && r.seq_id == seq_id) {
            Some(existing) => existing.closes |= closes,
            None => refs.push(TaskRef {
                key,
                seq_id,
                closes,
            }),
        }
    }
    refs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedTaskRef {
    pub task_id: Uuid,
    pub project_id: Uuid,
    pub closes: bool,
}

/// 参照をテナント内のタスクへ解決する。キーやタスクが見つからない参照は黙って捨てる。
///
/// 探すのは `tenant_id` のプロジェクトだけ。別テナントに同じキーがあっても解決しない
/// （webhook を送れるだけの外部から、他テナントのタスクへ行を足させない）。
pub async fn resolve<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    refs: &[TaskRef],
) -> Result<Vec<ResolvedTaskRef>, DbErr> {
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let projects = projects::Entity::find()
        .filter(projects::Column::TenantId.eq(tenant_id))
        .filter(projects::Column::Key.is_in(refs.iter().map(|r| r.key.as_str())))
        .all(db)
        .await?;

    let mut resolved = Vec::new();
    for project in projects {
        let wanted: Vec<&TaskRef> = refs.iter().filter(|r| r.key == project.key).collect();
        let found = tasks::Entity::find()
            .filter(tasks::Column::ProjectId.eq(project.id))
            .filter(tasks::Column::SeqId.is_in(wanted.iter().map(|r| r.seq_id)))
            .filter(tasks::Column::DeletedAt.is_null())
            .all(db)
            .await?;
        for task in found {
            let closes = wanted.iter().any(|r| r.seq_id == task.seq_id && r.closes);
            resolved.push(ResolvedTaskRef {
                task_id: task.id,
                project_id: project.id,
                closes,
            });
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(key: &str, seq_id: i32, closes: bool) -> TaskRef {
        TaskRef {
            key: key.to_string(),
            seq_id,
            closes,
        }
    }

    #[test]
    fn plain_reference() {
        assert_eq!(extract("TASK-140"), vec![r("TASK", 140, false)]);
    }

    #[test]
    fn lowercase_key_is_normalized() {
        assert_eq!(extract("task-140"), vec![r("TASK", 140, false)]);
    }

    #[test]
    fn bracketed_reference() {
        assert_eq!(
            extract("[TASK-140] ログイン修正"),
            vec![r("TASK", 140, false)]
        );
    }

    #[test]
    fn branch_name() {
        assert_eq!(extract("feat/TASK-140-foo"), vec![r("TASK", 140, false)]);
    }

    #[test]
    fn directly_after_japanese_text() {
        assert_eq!(
            extract("ログイン修正TASK-140を対応"),
            vec![r("TASK", 140, false)]
        );
    }

    #[test]
    fn multiple_references_in_order() {
        assert_eq!(
            extract("feat: OAuth TASK-42 TASK-43"),
            vec![r("TASK", 42, false), r("TASK", 43, false)]
        );
    }

    #[test]
    fn references_to_different_projects() {
        assert_eq!(
            extract("feat: OAuth ENG-42 BACK-7"),
            vec![r("ENG", 42, false), r("BACK", 7, false)]
        );
    }

    #[test]
    fn longer_number_is_not_a_shorter_reference() {
        assert_eq!(extract("TASK-1400"), vec![r("TASK", 1400, false)]);
    }

    #[test]
    fn digits_or_letters_glued_after_number_do_not_match() {
        assert_eq!(extract("TASK-140a"), vec![]);
        assert_eq!(extract("ATASK-140"), vec![r("ATASK", 140, false)]);
    }

    #[test]
    fn key_longer_than_ten_characters_does_not_match() {
        assert_eq!(extract("ABCDEFGHIJK-1"), vec![]);
    }

    /// キーに見えるだけのものも拾う。解決できなければ捨てるので許容する。
    #[test]
    fn false_positive_is_extracted_and_left_to_resolution() {
        assert_eq!(extract("SHA256-1"), vec![r("SHA256", 1, false)]);
    }

    #[test]
    fn number_overflowing_i32_is_skipped() {
        assert_eq!(extract("TASK-99999999999"), vec![]);
    }

    #[test]
    fn close_keywords_set_closes() {
        for text in [
            "Closes TASK-1",
            "close TASK-1",
            "Closed TASK-1",
            "Fix TASK-1",
            "fixes TASK-1",
            "FIXED TASK-1",
            "Resolve TASK-1",
            "resolves TASK-1",
            "Resolved TASK-1",
            "Implement TASK-1",
            "implements TASK-1",
            "Implemented TASK-1",
            "Closes: TASK-1",
        ] {
            assert_eq!(extract(text), vec![r("TASK", 1, true)], "{text}");
        }
    }

    #[test]
    fn keyword_inside_another_word_does_not_close() {
        assert_eq!(extract("prefixes TASK-1"), vec![r("TASK", 1, false)]);
        assert_eq!(extract("Closes the TASK-1"), vec![r("TASK", 1, false)]);
    }

    #[test]
    fn keyword_applies_only_to_the_following_reference() {
        assert_eq!(
            extract("Closes TASK-1 TASK-2"),
            vec![r("TASK", 1, true), r("TASK", 2, false)]
        );
    }

    #[test]
    fn duplicate_references_merge_and_keep_closes() {
        assert_eq!(extract("TASK-1 と Fixes task-1"), vec![r("TASK", 1, true)]);
    }
}
