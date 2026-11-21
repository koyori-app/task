//! Drive の Markdown 本文を見出しごとの節に切って `drive_file_sections` に索引し、
//! プロジェクトの中で本文を検索する（docs/dev/features/drive/files.md の「本文の索引」「本文の検索」）。

use std::borrow::Cow;

use futures::StreamExt;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, EntityTrait,
    FromQueryResult, PaginatorTrait, QueryFilter, QuerySelect, Statement, TransactionTrait,
    prelude::{DateTimeWithTimeZone, Uuid},
};
use tracing::warn;

use crate::drive_task_links::is_markdown;
use crate::error::AppError;
use crate::storage::StorageBackend;
use entity::{drive_file_sections, drive_files};
use payload::documents::{DocumentSearchHit, DocumentSearchResponse};

/// 索引する本文の上限（1 MiB）。超えた本文は先頭だけを索引する
pub const MAX_INDEXED_BYTES: usize = 1 << 20;
/// 1 ファイルの節の上限。超えた後の見出しでは切らず、残りは最後の節に入れる
pub const MAX_SECTIONS_PER_FILE: usize = 2000;
/// 見出し 1 つの文字数の上限。超えた分は捨てる
pub const MAX_HEADING_CHARS: usize = 200;
/// 抜粋で、当たった語の前後に残す文字数
pub const EXCERPT_CONTEXT_CHARS: usize = 100;
/// 語が見つからなかったときに抜粋として返す先頭の文字数
const EXCERPT_FALLBACK_CHARS: usize = 200;
/// `insert_many` 1 回の行数（Postgres の束縛値の上限 65535 を越えない）
const INSERT_CHUNK: usize = 500;
/// backfill が 1 回に取る候補の数
const BACKFILL_BATCH: u64 = 100;

/// 見出しで切った本文の 1 節
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// 祖先から自分までの見出し。最初の見出しより前の節は空
    pub heading_path: Vec<String>,
    /// 見出しの行から次の節の手前までの元の文字列
    pub body: String,
}

/// backfill の結果の件数
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BackfillReport {
    pub indexed: u64,
    pub skipped: u64,
    pub failed: u64,
}

/// 索引に入れる形へ整える。NUL を除き（TEXT に入らない）、上限で文字の境界まで戻して切る
fn indexable(text: &str) -> Cow<'_, str> {
    fn cut(s: &str) -> usize {
        let mut end = s.len().min(MAX_INDEXED_BYTES);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        end
    }
    if text.contains('\0') {
        let mut cleaned = text.replace('\0', "");
        cleaned.truncate(cut(&cleaned));
        Cow::Owned(cleaned)
    } else {
        Cow::Borrowed(&text[..cut(text)])
    }
}

/// 本文を見出しごとの節に切る。
///
/// 節の境界は、引用やリストの外に書かれた見出しだけ。コードブロックの中の `#` と
/// frontmatter の中身は見出しにしない。節の本文は元の文字列をそのまま持つ
/// （見出しの行もコードブロックも残す）。必ず 1 つ以上の節を返す。
pub fn split_sections(text: &str) -> Vec<Section> {
    let text = indexable(text);
    let text: &str = &text;

    // 節の境界（見出しの開始位置と、その見出しまでの経路）
    let mut boundaries: Vec<(usize, Vec<String>)> = Vec::new();
    // 最初の見出しより前に、空白でない本文があるか（最初の境界で決まる）
    let mut has_preface = false;
    let mut stack: Vec<(HeadingLevel, String)> = Vec::new();
    let mut depth = 0usize;
    // 読んでいる途中の見出し（level、開始位置、題）
    let mut current: Option<(HeadingLevel, usize, String)> = None;

    let parser =
        Parser::new_ext(text, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS).into_offset_iter();
    for (event, range) in parser {
        match event {
            Event::Start(tag) => {
                if depth == 0
                    && let Tag::Heading { level, .. } = tag
                {
                    if boundaries.is_empty() {
                        has_preface = !text[..range.start].trim().is_empty();
                    }
                    if usize::from(has_preface) + boundaries.len() < MAX_SECTIONS_PER_FILE {
                        current = Some((level, range.start, String::new()));
                    }
                }
                depth += 1;
            }
            Event::End(tag_end) => {
                depth = depth.saturating_sub(1);
                if depth == 0
                    && matches!(tag_end, TagEnd::Heading(_))
                    && let Some((level, start, title)) = current.take()
                {
                    let title: String = title.trim().chars().take(MAX_HEADING_CHARS).collect();
                    while stack.last().is_some_and(|(top, _)| *top >= level) {
                        stack.pop();
                    }
                    stack.push((level, title));
                    boundaries.push((start, stack.iter().map(|(_, t)| t.clone()).collect()));
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, _, title)) = current.as_mut() {
                    title.push_str(&t);
                }
            }
            _ => {}
        }
    }

    let Some(first_start) = boundaries.first().map(|(start, _)| *start) else {
        return vec![Section {
            heading_path: Vec::new(),
            body: text.to_owned(),
        }];
    };
    let mut sections = Vec::with_capacity(boundaries.len() + 1);
    if has_preface {
        sections.push(Section {
            heading_path: Vec::new(),
            body: text[..first_start].to_owned(),
        });
    }
    let mut ends: Vec<usize> = boundaries.iter().skip(1).map(|(start, _)| *start).collect();
    ends.push(text.len());
    for ((start, heading_path), end) in boundaries.into_iter().zip(ends) {
        sections.push(Section {
            heading_path,
            body: text[start..end].to_owned(),
        });
    }
    sections
}

fn chars_eq_ignore_case(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// `haystack` の中で `needle` が最初に現れる位置（文字単位）。大文字小文字は無視する
fn find_chars(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(a, b)| chars_eq_ignore_case(*a, *b))
    })
}

/// 節の本文から、語の前後を切り出した抜粋を作る。
///
/// 語を順に探し、最初に見つかった語の前後 [`EXCERPT_CONTEXT_CHARS`] 文字を返す。
/// どの語も見つからなければ先頭を返す。切った側に `…` を付ける。
/// 素の文字列で返す（HTML にしない）。
pub fn excerpt(body: &str, terms: &[String]) -> String {
    let chars: Vec<char> = body.chars().collect();
    let found = terms.iter().find_map(|term| {
        let needle: Vec<char> = term.chars().collect();
        find_chars(&chars, &needle).map(|at| (at, at + needle.len()))
    });
    let (start, end) = match found {
        Some((at, until)) => (
            at.saturating_sub(EXCERPT_CONTEXT_CHARS),
            (until + EXCERPT_CONTEXT_CHARS).min(chars.len()),
        ),
        None => (0, EXCERPT_FALLBACK_CHARS.min(chars.len())),
    };
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// 保存した本文に合わせて、そのファイルの節の索引を作り直す。Markdown 以外では何もしない。
///
/// 呼び出し側のトランザクションの中で呼ぶこと。同じファイルへの並行する書き込みは、
/// 呼び出し側が取る `drive_files` の行ロックで直列になる。
pub async fn reindex<C: ConnectionTrait>(
    db: &C,
    file: &drive_files::Model,
    body: &str,
) -> Result<(), AppError> {
    if !is_markdown(&file.mime_type) {
        return Ok(());
    }
    drive_file_sections::Entity::delete_many()
        .filter(drive_file_sections::Column::DriveFileId.eq(file.id))
        .exec(db)
        .await?;
    let mut rows = split_sections(body)
        .into_iter()
        .enumerate()
        .map(|(position, section)| drive_file_sections::ActiveModel {
            id: Set(Uuid::new_v4()),
            drive_file_id: Set(file.id),
            position: Set(position as i32),
            heading_path: Set(section.heading_path),
            body: Set(section.body),
        })
        .peekable();
    while rows.peek().is_some() {
        let chunk: Vec<_> = rows.by_ref().take(INSERT_CHUNK).collect();
        drive_file_sections::Entity::insert_many(chunk)
            .exec_without_returning(db)
            .await?;
    }
    Ok(())
}

#[derive(Debug, FromQueryResult)]
struct BackfillCandidate {
    id: Uuid,
    mime_type: String,
    storage_key: String,
}

/// 索引の無い Markdown ファイルの本文をストレージから読み、節の索引を作る。
///
/// 1 ファイルの失敗（ストレージに実体が無い、DB の失敗）は記録して次へ進む。
/// 失敗したファイルは次に呼ばれたときにもう一度試す。
pub async fn backfill_missing(
    db: &DatabaseConnection,
    storage: &dyn StorageBackend,
) -> Result<BackfillReport, AppError> {
    let mut report = BackfillReport::default();
    let mut last_id: Option<Uuid> = None;
    loop {
        let candidates = BackfillCandidate::find_by_statement(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT f.id, f.mime_type, f.storage_key FROM drive_files f \
             WHERE f.mime_type ILIKE 'text/markdown%' \
               AND ($1::uuid IS NULL OR f.id > $1::uuid) \
               AND NOT EXISTS (SELECT 1 FROM drive_file_sections s WHERE s.drive_file_id = f.id) \
             ORDER BY f.id ASC LIMIT $2",
            [last_id.into(), (BACKFILL_BATCH as i64).into()],
        ))
        .all(db)
        .await?;
        let Some(last) = candidates.last() else {
            break;
        };
        last_id = Some(last.id);
        let fetched = candidates.len() as u64;
        for candidate in candidates {
            if !is_markdown(&candidate.mime_type) {
                continue;
            }
            match backfill_one(db, storage, &candidate).await {
                Ok(true) => report.indexed += 1,
                Ok(false) => report.skipped += 1,
                Err(e) => {
                    warn!(file_id = %candidate.id, error = %e, "drive section index backfill failed for a file");
                    report.failed += 1;
                }
            }
        }
        if fetched < BACKFILL_BATCH {
            break;
        }
    }
    Ok(report)
}

/// 1 ファイルを索引する。索引したら `true`、別の書き込みが先に済ませていて飛ばしたら `false`
async fn backfill_one(
    db: &DatabaseConnection,
    storage: &dyn StorageBackend,
    candidate: &BackfillCandidate,
) -> Result<bool, anyhow::Error> {
    let mut stream = storage.get_stream(&candidate.storage_key).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk?);
        // 索引するのは先頭だけ。上限を越えたら残りは読まない
        if bytes.len() > MAX_INDEXED_BYTES {
            break;
        }
    }
    drop(stream);
    let text = String::from_utf8_lossy(&bytes);

    // 本文の更新と同じ行ロックを取り、読んだ本文がまだ今の本文かを確かめる。
    // 読んだ後に本文が更新されていれば、その保存が索引を作っているので上書きしない
    let txn = db.begin().await?;
    let Some(file) = drive_files::Entity::find_by_id(candidate.id)
        .lock_exclusive()
        .one(&txn)
        .await?
    else {
        return Ok(false);
    };
    if file.storage_key != candidate.storage_key {
        return Ok(false);
    }
    let existing = drive_file_sections::Entity::find()
        .filter(drive_file_sections::Column::DriveFileId.eq(file.id))
        .count(&txn)
        .await?;
    if existing > 0 {
        return Ok(false);
    }
    reindex(&txn, &file, &text).await?;
    txn.commit().await?;
    Ok(true)
}

#[derive(Debug, FromQueryResult)]
struct SearchRow {
    file_id: Uuid,
    file_name: String,
    folder_id: Option<Uuid>,
    heading_path: Vec<String>,
    position: i32,
    body: String,
    updated_at: DateTimeWithTimeZone,
}

/// プロジェクトのファイルの節を、全部の語を含むもの（ILIKE の部分一致の AND）で引く。
///
/// 認可は呼び出し側で済ませること。`total` は当たった節の数（`limit` / `offset` を掛ける前）。
pub async fn search<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
    project_id: Uuid,
    terms: &[String],
    limit: u64,
    offset: u64,
) -> Result<DocumentSearchResponse, AppError> {
    let mut values: Vec<sea_orm::Value> = vec![tenant_id.into(), project_id.into()];
    let mut conditions = String::from("f.tenant_id = $1 AND f.project_id = $2");
    for term in terms {
        values.push(format!("%{}%", common::db::escape_like(term)).into());
        conditions.push_str(&format!(" AND s.body ILIKE ${} ESCAPE '\\'", values.len()));
    }
    let from = "FROM drive_file_sections s JOIN drive_files f ON f.id = s.drive_file_id";

    let total: i64 = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            format!("SELECT COUNT(*)::bigint {from} WHERE {conditions}"),
            values.clone(),
        ))
        .await?
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("document search count returned no row"))
        })?
        .try_get_by_index(0)?;

    values.push((limit as i64).into());
    let limit_at = values.len();
    values.push((offset.min(i64::MAX as u64) as i64).into());
    let offset_at = values.len();
    // 並びを一意にする。一意でないと offset のページの境で行が重複・欠落する
    let rows = SearchRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        format!(
            "SELECT f.id AS file_id, f.name AS file_name, f.folder_id, s.heading_path, \
                    s.position, s.body, f.updated_at \
             {from} WHERE {conditions} \
             ORDER BY f.name ASC, f.id ASC, s.position ASC \
             LIMIT ${limit_at} OFFSET ${offset_at}"
        ),
        values,
    ))
    .all(db)
    .await?;

    Ok(DocumentSearchResponse {
        hits: rows
            .into_iter()
            .map(|row| DocumentSearchHit {
                file_id: row.file_id,
                file_name: row.file_name,
                folder_id: row.folder_id,
                heading_path: row.heading_path,
                position: row.position,
                excerpt: excerpt(&row.body, terms),
                updated_at: row.updated_at.into(),
            })
            .collect(),
        total: total.max(0) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(sections: &[Section]) -> Vec<Vec<&str>> {
        sections
            .iter()
            .map(|s| s.heading_path.iter().map(String::as_str).collect())
            .collect()
    }

    fn joined(sections: &[Section]) -> String {
        sections.iter().map(|s| s.body.as_str()).collect()
    }

    fn terms(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn body_without_heading_is_one_section() {
        let sections = split_sections("ただの本文\n二行目\n");
        assert_eq!(paths(&sections), vec![Vec::<&str>::new()]);
        assert_eq!(sections[0].body, "ただの本文\n二行目\n");

        let empty = split_sections("");
        assert_eq!(empty.len(), 1);
        assert!(empty[0].heading_path.is_empty());
        assert_eq!(empty[0].body, "");
    }

    #[test]
    fn heading_path_follows_levels() {
        let text = "# A\na\n## B\nb\n### C\nc\n## D\nd\n";
        let sections = split_sections(text);
        assert_eq!(
            paths(&sections),
            vec![
                vec!["A"],
                vec!["A", "B"],
                vec!["A", "B", "C"],
                vec!["A", "D"]
            ]
        );
        assert_eq!(sections[1].body, "## B\nb\n");
        assert_eq!(joined(&sections), text);
    }

    #[test]
    fn heading_path_with_skipped_and_shallower_levels() {
        // level を飛ばす
        let skipped = split_sections("# A\n### C\n## B\n");
        assert_eq!(
            paths(&skipped),
            vec![vec!["A"], vec!["A", "C"], vec!["A", "B"]]
        );
        // 浅い見出しから始まらない
        let shallower = split_sections("## B\n# A\n## C\n");
        assert_eq!(
            paths(&shallower),
            vec![vec!["B"], vec!["A"], vec!["A", "C"]]
        );
    }

    #[test]
    fn setext_heading_is_a_boundary() {
        let text = "前置き\n\n題\n===\n本文\n\n小題\n---\n続き\n";
        let sections = split_sections(text);
        assert_eq!(
            paths(&sections),
            vec![vec![], vec!["題"], vec!["題", "小題"]]
        );
        assert_eq!(sections[1].body, "題\n===\n本文\n\n");
        assert_eq!(joined(&sections), text);
    }

    #[test]
    fn hash_in_code_block_is_not_a_boundary_and_stays_in_body() {
        let text = "# A\n\n```sh\n# x\necho 1\n```\n\n## B\n";
        let sections = split_sections(text);
        assert_eq!(paths(&sections), vec![vec!["A"], vec!["A", "B"]]);
        assert!(sections[0].body.contains("# x\necho 1"));
        assert_eq!(joined(&sections), text);
    }

    #[test]
    fn headings_in_quote_and_list_are_not_boundaries() {
        let text = "# A\n\n> # 引用の中\n\n- # リストの中\n\n## B\n";
        let sections = split_sections(text);
        assert_eq!(paths(&sections), vec![vec!["A"], vec!["A", "B"]]);
        assert!(sections[0].body.contains("> # 引用の中"));
        assert!(sections[0].body.contains("- # リストの中"));
    }

    #[test]
    fn frontmatter_is_not_a_heading() {
        let text = "---\ntitle: 仕様\nstatus: draft\n---\n\n# A\n本文\n";
        let sections = split_sections(text);
        assert_eq!(paths(&sections), vec![vec![], vec!["A"]]);
        assert_eq!(sections[0].body, "---\ntitle: 仕様\nstatus: draft\n---\n\n");
        assert_eq!(joined(&sections), text);
    }

    #[test]
    fn inline_markup_in_heading_is_flattened() {
        let sections = split_sections("# A `c` *b*\n");
        assert_eq!(paths(&sections), vec![vec!["A c b"]]);
    }

    #[test]
    fn bodies_concatenate_back_to_the_input() {
        for text in [
            "前置き\n# 認証\n本文です\n## トークン\n更新の手順\n",
            "intro\r\n# A\r\nbody\r\n## B\r\nmore\r\n",
            "# A\n末尾に改行なし",
            "見出しなし、改行なし",
        ] {
            assert_eq!(joined(&split_sections(text)), text, "{text:?}");
        }
        // 空白だけの前置きは節にしない
        let sections = split_sections("\n\n# A\nbody\n");
        assert_eq!(paths(&sections), vec![vec!["A"]]);
        assert_eq!(joined(&sections), "# A\nbody\n");
    }

    #[test]
    fn nul_is_removed() {
        let text = "pre\0face\n# A\0B\nbo\0dy\n";
        let sections = split_sections(text);
        assert!(sections.iter().all(|s| !s.body.contains('\0')));
        assert!(
            sections
                .iter()
                .all(|s| s.heading_path.iter().all(|h| !h.contains('\0')))
        );
        assert_eq!(joined(&sections), text.replace('\0', ""));
    }

    #[test]
    fn section_count_is_capped() {
        let at_limit: String = (0..MAX_SECTIONS_PER_FILE)
            .map(|i| format!("# h{i}\n"))
            .collect();
        assert_eq!(split_sections(&at_limit).len(), MAX_SECTIONS_PER_FILE);

        let over = format!("{at_limit}# 余り\n残りの本文\n");
        let sections = split_sections(&over);
        assert_eq!(sections.len(), MAX_SECTIONS_PER_FILE);
        let last = sections.last().expect("last section");
        assert_eq!(
            last.heading_path,
            vec![format!("h{}", MAX_SECTIONS_PER_FILE - 1)]
        );
        assert!(last.body.ends_with("# 余り\n残りの本文\n"));
        assert_eq!(joined(&sections), over);

        // 前置きの節も上限に数える
        let with_preface = format!("前置き\n{over}");
        assert_eq!(split_sections(&with_preface).len(), MAX_SECTIONS_PER_FILE);
    }

    #[test]
    fn indexed_bytes_are_capped_on_a_char_boundary() {
        let exact = "a".repeat(MAX_INDEXED_BYTES);
        assert_eq!(joined(&split_sections(&exact)).len(), MAX_INDEXED_BYTES);

        let over = "a".repeat(MAX_INDEXED_BYTES + 1);
        assert_eq!(joined(&split_sections(&over)).len(), MAX_INDEXED_BYTES);

        // 3 バイトの文字が上限をまたぐ。途中では切らず、その文字の手前まで戻す
        let multibyte = format!("{}あ", "a".repeat(MAX_INDEXED_BYTES - 1));
        let body = joined(&split_sections(&multibyte));
        assert_eq!(body.len(), MAX_INDEXED_BYTES - 1);
        assert!(body.chars().all(|c| c == 'a'));
    }

    #[test]
    fn heading_is_capped() {
        let text = format!("# {}\n", "あ".repeat(MAX_HEADING_CHARS + 1));
        let sections = split_sections(&text);
        assert_eq!(
            sections[0].heading_path[0].chars().count(),
            MAX_HEADING_CHARS
        );
        let exact = format!("# {}\n", "あ".repeat(MAX_HEADING_CHARS));
        assert_eq!(
            split_sections(&exact)[0].heading_path[0].chars().count(),
            MAX_HEADING_CHARS
        );
    }

    #[test]
    fn excerpt_cuts_around_the_term() {
        let body = format!("{}needle{}", "a".repeat(150), "b".repeat(150));
        let out = excerpt(&body, &terms(&["needle"]));
        assert_eq!(
            out,
            format!("…{}needle{}…", "a".repeat(100), "b".repeat(100))
        );

        // 切らなかった側には付かない
        let head = format!("needle{}", "b".repeat(150));
        assert_eq!(
            excerpt(&head, &terms(&["needle"])),
            format!("needle{}…", "b".repeat(100))
        );
        let tail = format!("{}needle", "a".repeat(150));
        assert_eq!(
            excerpt(&tail, &terms(&["needle"])),
            format!("…{}needle", "a".repeat(100))
        );
        // 前後がちょうど 100 文字なら切らない
        let exact = format!("{}needle{}", "a".repeat(100), "b".repeat(100));
        assert_eq!(excerpt(&exact, &terms(&["needle"])), exact);
    }

    #[test]
    fn excerpt_ignores_case_and_counts_chars() {
        assert_eq!(
            excerpt("The Refresh_Token", &terms(&["refresh_token"])),
            "The Refresh_Token"
        );

        let body = format!("{}認証トークン{}", "あ".repeat(120), "い".repeat(120));
        assert_eq!(
            excerpt(&body, &terms(&["認証トークン"])),
            format!("…{}認証トークン{}…", "あ".repeat(100), "い".repeat(100))
        );
    }

    #[test]
    fn excerpt_falls_back_to_later_terms_then_to_the_head() {
        let body = format!("{}second{}", "a".repeat(150), "b".repeat(10));
        assert_eq!(
            excerpt(&body, &terms(&["missing", "second"])),
            format!("…{}second{}", "a".repeat(100), "b".repeat(10))
        );

        let long = "x".repeat(201);
        assert_eq!(
            excerpt(&long, &terms(&["missing"])),
            format!("{}…", "x".repeat(200))
        );
        assert_eq!(excerpt("short", &terms(&["missing"])), "short");
    }

    #[test]
    fn excerpt_is_plain_text() {
        let body = "before <b>bold</b> & needle after";
        assert_eq!(excerpt(body, &terms(&["needle"])), body);
    }
}
