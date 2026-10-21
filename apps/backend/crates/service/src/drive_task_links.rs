//! Drive の Markdown 本文に書かれた `KEY-N` を `task_attachments`（`source = 'body'`）へ
//! 結び付ける（docs/features/drive.md §8.5）。

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use sea_orm::sea_query::{OnConflict, Query, SimpleExpr};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect,
    prelude::Uuid,
};

use crate::access::visible_project_ids;
use crate::drive::is_tenant_owner;
use crate::error::AppError;
use crate::forge::task_refs::{extract_limited, resolve};
use entity::{drive_files, task_attachments, tasks};

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
/// 消すのも保存者が入れるプロジェクト（と削除済みタスク）の `body` 行だけで、入れない
/// プロジェクトへの行は本文を判断できないので残す（保存した人で結び付きが消えたり戻ったり
/// しないため）。プロジェクトファイルはそのプロジェクトのタスクにだけ結ぶ（手動添付と同じ範囲）。
/// プロジェクトファイルから別プロジェクトのタスクへの `body` 行は、保存者に見えなくても消す。
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

    let refs = extract_limited(&strip_code(body), MAX_LINKS_PER_FILE);
    let mut resolved = resolve(db, file.tenant_id, &refs).await?;
    // 結び付け先は手動添付と同じ範囲。プロジェクトファイルを別プロジェクトのタスクへ結ぶと、
    // その添付一覧からファイル名が元のプロジェクトに入れない人へ見えてしまう
    if let Some(file_project) = file.project_id {
        resolved.retain(|r| r.project_id == file_project);
    }

    // 今ある body 行とそのタスクのプロジェクト（削除済み・消えたタスクは None）
    let current: Vec<Uuid> = task_attachments::Entity::find()
        .filter(task_attachments::Column::DriveFileId.eq(file.id))
        .filter(task_attachments::Column::Source.eq(task_attachments::SOURCE_BODY))
        .select_only()
        .column(task_attachments::Column::TaskId)
        .into_tuple::<Uuid>()
        .all(db)
        .await?;
    let current_projects: HashMap<Uuid, Uuid> = if current.is_empty() {
        HashMap::new()
    } else {
        tasks::Entity::find()
            .filter(tasks::Column::Id.is_in(current.iter().copied()))
            .filter(tasks::Column::DeletedAt.is_null())
            .select_only()
            .column(tasks::Column::Id)
            .column(tasks::Column::ProjectId)
            .into_tuple::<(Uuid, Uuid)>()
            .all(db)
            .await?
            .into_iter()
            .collect()
    };

    let project_ids: HashSet<Uuid> = resolved
        .iter()
        .map(|r| r.project_id)
        .chain(current_projects.values().copied())
        .collect();
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

    let stale: Vec<Uuid> = current
        .into_iter()
        .filter(|task_id| match current_projects.get(task_id) {
            None => true,
            // 別プロジェクトのタスクへの行は、保存者に見えなくても外す（移動前に付いた行など）
            Some(project_id) => {
                file.project_id.is_some_and(|p| p != *project_id)
                    || (visible.contains(project_id) && !wanted.contains(task_id))
            }
        })
        .collect();
    if !stale.is_empty() {
        task_attachments::Entity::delete_many()
            .filter(task_attachments::Column::DriveFileId.eq(file.id))
            .filter(task_attachments::Column::Source.eq(task_attachments::SOURCE_BODY))
            .filter(task_attachments::Column::TaskId.is_in(stale))
            .exec(db)
            .await?;
    }

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

/// `files`（`drive_files` への条件）に当たるファイルから、プロジェクト `project_id` の外の
/// タスクへの `body` 行を外す。`manual` 行には触れない。
///
/// ファイルをプロジェクトへ移した時に呼ぶ。プロジェクトファイルはそのプロジェクトのタスクに
/// だけ結ぶので、移動前に付いた行を残すと、移動元のプロジェクトのタスクの添付一覧から
/// 移動先にしか置かれていないファイルの名前が見え続ける
pub async fn unlink_body_outside_project<C: ConnectionTrait>(
    db: &C,
    files: SimpleExpr,
    project_id: Uuid,
) -> Result<(), AppError> {
    task_attachments::Entity::delete_many()
        .filter(task_attachments::Column::Source.eq(task_attachments::SOURCE_BODY))
        .filter(
            task_attachments::Column::DriveFileId.in_subquery(
                Query::select()
                    .column(drive_files::Column::Id)
                    .from(drive_files::Entity)
                    .and_where(files)
                    .to_owned(),
            ),
        )
        .filter(
            task_attachments::Column::TaskId.not_in_subquery(
                Query::select()
                    .column(tasks::Column::Id)
                    .from(tasks::Entity)
                    .and_where(tasks::Column::ProjectId.eq(project_id))
                    .to_owned(),
            ),
        )
        .exec(db)
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
///
/// 列ごとに残りの行から閉じを探すと、長さ違いの閉じない列を並べた行で行の長さの 1.5 乗に
/// なる（本文の保存はテナントのロックの内側で走る）。列を一度集め、「同じ長さの次の列」を
/// 後ろから一度なめて引けるようにして、行の長さに対して線形にする
fn strip_inline_code(line: &str, out: &mut String) {
    let bytes = line.as_bytes();
    // バッククォート列の (開始位置, 長さ)
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let n = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            runs.push((i, n));
            i += n;
        } else {
            i += 1;
        }
    }
    let mut next_same: Vec<Option<usize>> = vec![None; runs.len()];
    let mut last_by_len: HashMap<usize, usize> = HashMap::new();
    for (k, &(_, n)) in runs.iter().enumerate().rev() {
        next_same[k] = last_by_len.insert(n, k);
    }

    let mut copied = 0;
    let mut k = 0;
    while k < runs.len() {
        match next_same[k] {
            Some(close) => {
                out.push_str(&line[copied..runs[k].0]);
                out.push(' ');
                let (start, n) = runs[close];
                copied = start + n;
                k = close + 1;
            }
            None => k += 1,
        }
    }
    out.push_str(&line[copied..]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::task_refs::{TaskRef, extract};

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

    /// 長さを 1 ずつ増やした閉じない列を 1 行に 4 MiB 並べても線形で終わる。
    /// 列ごとに残りの行を舐める実装では行の長さの 1.5 乗になり、ここで数十秒かかる
    #[test]
    fn unmatched_runs_of_growing_length_stay_linear() {
        let mut line = String::new();
        let mut n = 1;
        while line.len() < 4 << 20 {
            line.push_str(&"`".repeat(n));
            line.push('a');
            n += 1;
        }
        let started = std::time::Instant::now();
        let stripped = strip_code(&line);
        let elapsed = started.elapsed();
        assert_eq!(stripped, format!("{line}\n"), "閉じない列はそのまま残す");
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "4 MiB の行に {elapsed:?} かかった"
        );
    }

    #[test]
    fn markdown_mime() {
        assert!(is_markdown("text/markdown"));
        assert!(is_markdown("Text/Markdown; charset=utf-8"));
        assert!(!is_markdown("text/plain"));
    }
}
