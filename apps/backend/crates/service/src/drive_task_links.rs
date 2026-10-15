//! Drive の Markdown 本文に書かれた `KEY-N` を `task_attachments`（`source = 'body'`）へ
//! 結び付ける（docs/features/drive.md §8.5）。

use std::collections::HashSet;

use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect,
    prelude::Uuid,
};

use crate::access::visible_project_ids;
use crate::drive::is_tenant_owner;
use crate::error::AppError;
use crate::forge::task_refs::{extract, resolve};
use entity::{drive_files, task_attachments};

/// 1 ファイルから結び付ける参照の上限。本文に現れた順で数え、超えた分は捨てる
pub const MAX_LINKS_PER_FILE: usize = 200;

/// 本文から参照を拾う対象の MIME か（パラメータ付きの値も受ける）
pub fn is_markdown(mime: &str) -> bool {
    mime.split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .eq_ignore_ascii_case("text/markdown")
}

/// 保存した本文に合わせて、そのファイルの `body` 行を作り直す。`manual` 行には触れない。
///
/// 結び付けるのは `user_id`（保存した利用者）が入れるプロジェクトのタスクだけ。
/// プロジェクトファイルはそのプロジェクトのタスクにだけ結ぶ（手動添付と同じ範囲）。
/// 呼び出し側はファイルの書き込み権限とテナントへの所属を確認済みであること。
/// Markdown 以外のファイルでは何もしない。
pub async fn sync_body_links<C: ConnectionTrait>(
    db: &C,
    file: &drive_files::Model,
    user_id: Uuid,
    body: &str,
) -> Result<(), AppError> {
    if !is_markdown(&file.mime_type) {
        return Ok(());
    }

    let mut refs = extract(&strip_code(body));
    refs.truncate(MAX_LINKS_PER_FILE);
    let mut resolved = resolve(db, file.tenant_id, &refs).await?;
    // 結び付け先は手動添付と同じ範囲。プロジェクトファイルを別プロジェクトのタスクへ結ぶと、
    // その添付一覧からファイル名が元のプロジェクトに入れない人へ見えてしまう
    if let Some(file_project) = file.project_id {
        resolved.retain(|r| r.project_id == file_project);
    }

    let project_ids: HashSet<Uuid> = resolved.iter().map(|r| r.project_id).collect();
    let visible = if is_tenant_owner(db, file.tenant_id, user_id).await? {
        project_ids
    } else {
        visible_project_ids(db, project_ids.into_iter().collect(), user_id).await?
    };
    let wanted: HashSet<Uuid> = resolved
        .iter()
        .filter(|r| visible.contains(&r.project_id))
        .map(|r| r.task_id)
        .collect();

    let mut stale = task_attachments::Entity::delete_many()
        .filter(task_attachments::Column::DriveFileId.eq(file.id))
        .filter(task_attachments::Column::Source.eq(task_attachments::SOURCE_BODY));
    if !wanted.is_empty() {
        stale = stale.filter(task_attachments::Column::TaskId.is_not_in(wanted.iter().copied()));
    }
    stale.exec(db).await?;

    let existing: HashSet<Uuid> = task_attachments::Entity::find()
        .filter(task_attachments::Column::DriveFileId.eq(file.id))
        .select_only()
        .column(task_attachments::Column::TaskId)
        .into_tuple::<Uuid>()
        .all(db)
        .await?
        .into_iter()
        .collect();

    let now = Utc::now();
    let new_rows: Vec<task_attachments::ActiveModel> = wanted
        .difference(&existing)
        .map(|&task_id| task_attachments::ActiveModel {
            id: Set(Uuid::new_v4()),
            task_id: Set(task_id),
            drive_file_id: Set(file.id),
            created_by: Set(user_id),
            created_at: Set(now.into()),
            source: Set(task_attachments::SOURCE_BODY.to_string()),
        })
        .collect();
    if new_rows.is_empty() {
        return Ok(());
    }
    // 並行する手動添付が先に入っていたら、そちら（manual）を残す
    task_attachments::Entity::insert_many(new_rows)
        .on_conflict(
            OnConflict::columns([
                task_attachments::Column::TaskId,
                task_attachments::Column::DriveFileId,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// フェンスコードブロック（```` ``` ```` / `~~~`）とインラインコードの中身を落とす。
/// 閉じられていないフェンスは末尾まで落とす。行の区切りは残す。
///
/// shortcut: インラインコードは行内でしか対にしない（行を跨ぐコードスパンは拾ってしまう）。
/// 誤って結び付くのが問題になったら Markdown パーサーに置き換える
pub fn strip_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut fence: Option<(u8, usize)> = None;
    for line in text.lines() {
        match fence {
            Some((ch, len)) => {
                let trimmed = line.trim();
                if fence_marker(line).is_some_and(|(c, n)| c == ch && n >= len)
                    && trimmed.bytes().all(|b| b == ch)
                {
                    fence = None;
                }
            }
            None => match fence_marker(line) {
                Some(marker) => fence = Some(marker),
                None => strip_inline_code(line, &mut out),
            },
        }
        out.push('\n');
    }
    out
}

/// 行頭（字下げ可）の 3 個以上の ``` ` ``` か `~` の連なり
fn fence_marker(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start();
    let ch = *trimmed.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = trimmed.bytes().take_while(|&b| b == ch).count();
    (len >= 3).then_some((ch, len))
}

/// 同じ長さのバッククォート列で閉じたコードスパンを空白 1 つに置き換える。
/// 閉じられていないバッククォートはそのまま残す（CommonMark と同じ）
fn strip_inline_code(line: &str, out: &mut String) {
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        out.push_str(&rest[..start]);
        let run = &rest[start..];
        let len = run.len() - run.trim_start_matches('`').len();
        let after = &run[len..];
        match closing_run(after, len) {
            Some(end) => {
                out.push(' ');
                rest = &after[end + len..];
            }
            None => {
                out.push_str(&run[..len]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
}

/// ちょうど `len` 個のバッククォート列の開始位置
fn closing_run(s: &str, len: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let n = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            if n == len {
                return Some(i);
            }
            i += n;
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::task_refs::TaskRef;

    fn keys(text: &str) -> Vec<String> {
        extract(&strip_code(text))
            .into_iter()
            .map(|TaskRef { key, seq_id, .. }| format!("{key}-{seq_id}"))
            .collect()
    }

    #[test]
    fn backtick_fence_is_removed() {
        let text = "TASK-1\n```rust\nTASK-2\n```\nTASK-3";
        assert_eq!(keys(text), ["TASK-1", "TASK-3"]);
    }

    #[test]
    fn tilde_fence_is_removed() {
        let text = "~~~\nTASK-2\n~~~\nTASK-3";
        assert_eq!(keys(text), ["TASK-3"]);
    }

    #[test]
    fn fence_closes_only_with_same_char_and_enough_length() {
        // ``` の中の ~~~ や短いフェンスでは閉じない
        let text = "````\n~~~\n```\nTASK-2\n````\nTASK-3";
        assert_eq!(keys(text), ["TASK-3"]);
    }

    #[test]
    fn unclosed_fence_strips_to_end() {
        let text = "TASK-1\n```\nTASK-2\n\nTASK-3";
        assert_eq!(keys(text), ["TASK-1"]);
    }

    #[test]
    fn inline_code_is_removed() {
        let text = "see `TASK-2` and TASK-3, ``a ` TASK-4`` then TASK-5";
        assert_eq!(keys(text), ["TASK-3", "TASK-5"]);
    }

    #[test]
    fn unmatched_backtick_keeps_text() {
        assert_eq!(keys("a ` TASK-1"), ["TASK-1"]);
        // 長さの違う列では閉じない
        assert_eq!(keys("``TASK-1` TASK-2"), ["TASK-1", "TASK-2"]);
    }

    #[test]
    fn markdown_mime() {
        assert!(is_markdown("text/markdown"));
        assert!(is_markdown("Text/Markdown; charset=utf-8"));
        assert!(!is_markdown("text/plain"));
    }
}
