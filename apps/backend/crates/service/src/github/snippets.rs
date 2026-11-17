//! 連携リポジトリのファイルから、パーマリンクが指す行を切り出す。
//!
//! 本文に貼った GitHub のパーマリンク（コミット SHA と行範囲つきの blob URL）を、画面が
//! コードの抜粋として出すための取得口。GitHub App の Contents: Read で読み、SHA 指定で
//! 中身が変わらないので Redis に置く。Installation Access Token の取得は 1 回につき
//! GitHub へ 2 リクエスト出るため、閲覧のたびには取りに行かない。
//!
//! 誰に見せてよいかはここでは決めない。呼び出し側（handler）が、連携行のプロジェクトに
//! 入れる利用者かを確かめてから [`snippet_for`] を呼ぶ。

use std::time::Duration;

use base64::Engine;
use reqwest::{Client, Method, StatusCode};
use sea_orm::sea_query::{Expr, ExprTrait, Func};
use sea_orm::{
    DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect, prelude::Uuid,
};

use common::cache::redis::RedisConnection;
use common::settings::GithubAppSettings;
use entity::{github_integrations, projects};
use payload::github::{GithubSnippetQuery, GithubSnippetResponse};

use super::client::api_base;
use super::commits::request;
use super::sync::installation_token;

/// 1 つの抜粋で返す行数の上限。
pub const MAX_SNIPPET_LINES: usize = 200;
/// 1 つの抜粋で返す文字数（UTF-16 の符号単位、行ごとの改行 1 つを含む）の上限。
/// 画面は抜粋を着色の口（`/internal/render-description`、本文 65,536 文字まで）へ送るので、
/// その上限に収まる値にする。向こうが数えるのは JavaScript の `length` なので単位を合わせる。
pub const MAX_SNIPPET_CHARS: usize = 48_000;
/// Contents API が本文を返すファイルの大きさの上限（これを超えると `encoding: "none"`）。
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// GitHub の応答として読む量の上限（1 MB の本文の base64 と JSON の分）。
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
pub const CACHE_KEY_PREFIX: &str = "github:snippet:v1:";
pub const CACHE_TTL_SECS: u64 = 86_400;
pub const RATE_LIMIT_KEY_PREFIX: &str = "github:snippet:rl:";
/// キャッシュに無い抜粋を 1 利用者が取りに行ける回数（[`FETCH_RATE_WINDOW_SECS`] あたり）。
pub const FETCH_RATE_LIMIT: i64 = 60;
pub const FETCH_RATE_WINDOW_SECS: i64 = 600;

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// 同じリポジトリを連携しているプロジェクトのうち、認可を試す件数の上限。
const MAX_CANDIDATES: u64 = 20;
const MAX_PATH_BYTES: usize = 1024;
const MAX_REPO_LEN: usize = 100;
const MAX_OWNER_LEN: usize = 39;

/// 検査を通った抜粋の要求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetRequest {
    pub owner: String,
    pub repo: String,
    /// 小文字の 40 桁
    pub sha: String,
    pub path: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SnippetOutcome {
    Found(GithubSnippetResponse),
    /// ファイルが無い、ファイルでない、または行がファイルの外
    NotFound,
    /// バイナリ・大きすぎるなど、行として出せない
    NotDisplayable,
    /// 利用者の取得回数の上限を超えた
    RateLimited,
}

fn valid_owner(owner: &str) -> bool {
    let bytes = owner.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_OWNER_LEN
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
}

fn valid_repo(repo: &str) -> bool {
    !repo.is_empty()
        && repo.len() <= MAX_REPO_LEN
        && repo != "."
        && repo != ".."
        && repo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_BYTES
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !segment.chars().any(char::is_control)
        })
}

/// クエリを検査する。規則は frontend の `parseGithubPermalink` と同じ。
pub fn parse_request(query: &GithubSnippetQuery) -> Option<SnippetRequest> {
    if !valid_owner(&query.owner) || !valid_repo(&query.repo) || !valid_path(&query.path) {
        return None;
    }
    if query.sha.len() != 40 || !query.sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let end = query.end.unwrap_or(query.start);
    if query.start < 1 || end < query.start {
        return None;
    }
    Some(SnippetRequest {
        owner: query.owner.clone(),
        repo: query.repo.clone(),
        sha: query.sha.to_ascii_lowercase(),
        path: query.path.clone(),
        start: query.start,
        end,
    })
}

/// `owner/repo` を連携しているプロジェクトの連携行を、テナントの ID と一緒に古い順で返す。
/// GitHub は owner / repo の大文字小文字を区別しないので、小文字にそろえて比べる。
pub async fn find_integrations(
    db: &DatabaseConnection,
    owner: &str,
    repo: &str,
) -> Result<Vec<(github_integrations::Model, Uuid)>, DbErr> {
    let rows = github_integrations::Entity::find()
        .find_also_related(projects::Entity)
        .filter(
            Expr::expr(Func::lower(Expr::col((
                github_integrations::Entity,
                github_integrations::Column::RepoOwner,
            ))))
            .eq(owner.to_lowercase()),
        )
        .filter(
            Expr::expr(Func::lower(Expr::col((
                github_integrations::Entity,
                github_integrations::Column::RepoName,
            ))))
            .eq(repo.to_lowercase()),
        )
        .order_by_asc(github_integrations::Column::CreatedAt)
        .order_by_asc(github_integrations::Column::Id)
        .limit(MAX_CANDIDATES)
        .all(db)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(integration, project)| Some((integration, project?.tenant_id)))
        .collect())
}

/// `text` の `start`〜`end` 行目（1 始まり、両端を含む）を切り出す。
///
/// 返すのは（行、実際に返した最後の行の番号、上限で打ち切ったか）。`start` がファイルの
/// 行数を超えていれば `None`。`end` は行数に丸める。上限に達して 1 行も入らないときは
/// 行が空になる（呼び出し側が「出せない」と扱う）。
pub fn slice_lines(text: &str, start: u32, end: u32) -> Option<(Vec<String>, u32, bool)> {
    let body = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() || start < 1 {
        return None;
    }
    let skip = usize::try_from(start - 1).ok()?;
    let wanted = usize::try_from(end.saturating_sub(start)).ok()? + 1;

    let mut lines = Vec::new();
    let mut chars = 0usize;
    let mut truncated = false;
    let mut reached_start = false;
    for line in body.split('\n').skip(skip).take(wanted) {
        reached_start = true;
        let line = line.strip_suffix('\r').unwrap_or(line);
        let cost = line.encode_utf16().count() + 1;
        if lines.len() >= MAX_SNIPPET_LINES || chars + cost > MAX_SNIPPET_CHARS {
            truncated = true;
            break;
        }
        chars += cost;
        lines.push(line.to_string());
    }
    if !reached_start {
        return None;
    }
    let end_line = start + u32::try_from(lines.len()).ok()?.saturating_sub(1);
    Some((lines, end_line, truncated))
}

/// 抜粋のキャッシュのキー。`installation_id` を入れて、別のインストールが読んだ中身を
/// 取り違えないようにする。path は `:` を含み得るので末尾に置く。
pub fn cache_key(installation_id: i64, request: &SnippetRequest) -> String {
    format!(
        "{CACHE_KEY_PREFIX}{installation_id}:{}/{}@{}:{}-{}:{}",
        request.owner.to_lowercase(),
        request.repo.to_lowercase(),
        request.sha,
        request.start,
        request.end,
        request.path
    )
}

/// 利用者ごとの取得回数を 1 つ数え、上限内なら true を返す（固定窓）。
async fn try_acquire_fetch_slot(
    conn: &mut impl redis::aio::ConnectionLike,
    user_id: Uuid,
) -> Result<bool, anyhow::Error> {
    let key = format!("{RATE_LIMIT_KEY_PREFIX}{user_id}");
    let (count, _): (i64, i64) = redis::pipe()
        .atomic()
        .cmd("INCR")
        .arg(&key)
        .cmd("EXPIRE")
        .arg(&key)
        .arg(FETCH_RATE_WINDOW_SECS)
        .arg("NX")
        .query_async(conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis snippet rate limit: {e}"))?;
    Ok(count <= FETCH_RATE_LIMIT)
}

/// Contents API の応答から読めたもの。
enum Fetched {
    Text(String),
    NotFound,
    NotDisplayable,
}

fn contents_url(
    integration: &github_integrations::Model,
    request: &SnippetRequest,
) -> Result<reqwest::Url, anyhow::Error> {
    let mut url = reqwest::Url::parse(&api_base())?;
    url.path_segments_mut()
        .map_err(|()| anyhow::anyhow!("github api base cannot be a base url"))?
        .pop_if_empty()
        .extend([
            "repos",
            integration.repo_owner.as_str(),
            integration.repo_name.as_str(),
            "contents",
        ])
        .extend(request.path.split('/'));
    url.query_pairs_mut().append_pair("ref", &request.sha);
    Ok(url)
}

async fn fetch_file(
    http: &Client,
    token: &str,
    url: reqwest::Url,
) -> Result<Fetched, anyhow::Error> {
    let mut res = request(http, Method::GET, url.as_str(), token)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await?;
    let status = res.status();
    if status == StatusCode::NOT_FOUND {
        return Ok(Fetched::NotFound);
    }
    if !status.is_success() {
        return Err(anyhow::anyhow!("get repository content failed: {status}"));
    }
    if res
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
    {
        return Ok(Fetched::NotDisplayable);
    }
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Ok(Fetched::NotDisplayable);
        }
        body.extend_from_slice(&chunk);
    }

    let json: serde_json::Value = serde_json::from_slice(&body)?;
    // ディレクトリは配列で返る。symlink / submodule は type が file でない
    if json.get("type").and_then(|v| v.as_str()) != Some("file") {
        return Ok(Fetched::NotFound);
    }
    let size = json.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
    if json.get("encoding").and_then(|v| v.as_str()) != Some("base64") || size > MAX_FILE_BYTES {
        return Ok(Fetched::NotDisplayable);
    }
    let Some(content) = json.get("content").and_then(|v| v.as_str()) else {
        return Ok(Fetched::NotDisplayable);
    };
    // GitHub は base64 を 60 桁ごとに改行して返す
    let compact: String = content
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(compact) else {
        return Ok(Fetched::NotDisplayable);
    };
    Ok(match String::from_utf8(bytes) {
        Ok(text) => Fetched::Text(text),
        Err(_) => Fetched::NotDisplayable,
    })
}

/// `integration` のリポジトリから抜粋を取る。`request` は [`parse_request`] を通した値。
/// `user_id` は、キャッシュに無い抜粋を取りに行く回数を数える相手。
///
/// 取得先の owner / repo は連携行の値を使う（要求の値は連携行を引くためだけに使う）。
pub async fn snippet_for(
    redis: &RedisConnection,
    http: &Client,
    settings: &GithubAppSettings,
    user_id: Uuid,
    integration: &github_integrations::Model,
    request: &SnippetRequest,
) -> Result<SnippetOutcome, anyhow::Error> {
    let key = cache_key(integration.installation_id, request);
    let mut conn = redis
        .conn
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("redis acquire: {e}"))?;
    let cached: Option<String> = redis::cmd("GET")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis GET snippet: {e}"))?;
    if let Some(found) = cached
        .as_deref()
        .and_then(|raw| serde_json::from_str::<GithubSnippetResponse>(raw).ok())
    {
        return Ok(SnippetOutcome::Found(found));
    }

    if !try_acquire_fetch_slot(&mut conn, user_id).await? {
        return Ok(SnippetOutcome::RateLimited);
    }

    let token = installation_token(http, settings, integration.installation_id).await?;
    let text = match fetch_file(http, &token, contents_url(integration, request)?).await? {
        Fetched::Text(text) => text,
        Fetched::NotFound => return Ok(SnippetOutcome::NotFound),
        Fetched::NotDisplayable => return Ok(SnippetOutcome::NotDisplayable),
    };
    let Some((lines, end_line, truncated)) = slice_lines(&text, request.start, request.end) else {
        return Ok(SnippetOutcome::NotFound);
    };
    if lines.is_empty() {
        return Ok(SnippetOutcome::NotDisplayable);
    }
    let found = GithubSnippetResponse {
        repo_owner: integration.repo_owner.clone(),
        repo_name: integration.repo_name.clone(),
        sha: request.sha.clone(),
        path: request.path.clone(),
        start_line: request.start,
        end_line,
        lines,
        truncated,
    };
    let _: () = redis::cmd("SET")
        .arg(&key)
        .arg(serde_json::to_string(&found)?)
        .arg("EX")
        .arg(CACHE_TTL_SECS)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET snippet: {e}"))?;
    Ok(SnippetOutcome::Found(found))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "a3f92c1e7b81d4000000000000000000000000aa";

    fn query(
        owner: &str,
        repo: &str,
        sha: &str,
        path: &str,
        start: u32,
        end: Option<u32>,
    ) -> GithubSnippetQuery {
        GithubSnippetQuery {
            owner: owner.into(),
            repo: repo.into(),
            sha: sha.into(),
            path: path.into(),
            start,
            end,
        }
    }

    #[test]
    fn parse_request_accepts_permalink_shapes() {
        let parsed = parse_request(&query("acme", "backend", SHA, "src/a.rs", 3, None))
            .expect("single line");
        assert_eq!((parsed.start, parsed.end), (3, 3));

        let parsed =
            parse_request(&query("acme", "backend", SHA, "src/a.rs", 3, Some(10))).expect("range");
        assert_eq!((parsed.start, parsed.end), (3, 10));

        // SHA は小文字にそろえる
        let parsed = parse_request(&query(
            "Acme",
            "back.end_x-1",
            &SHA.to_uppercase(),
            "docs/a b/日本語.md",
            1,
            Some(1),
        ))
        .expect("upper sha and spaces");
        assert_eq!(parsed.sha, SHA);
        assert_eq!(parsed.owner, "Acme");
    }

    #[test]
    fn parse_request_rejects_everything_else() {
        let long_path = "a/".repeat(600);
        let cases = [
            query("acme", "backend", "main", "a.rs", 1, None),
            query("acme", "backend", &SHA[..7], "a.rs", 1, None),
            query(
                "acme",
                "backend",
                &format!("{}g", &SHA[..39]),
                "a.rs",
                1,
                None,
            ),
            query("acme", "backend", SHA, "a.rs", 0, None),
            query("acme", "backend", SHA, "a.rs", 5, Some(4)),
            query("acme", "backend", SHA, "../a.rs", 1, None),
            query("acme", "backend", SHA, "a/./b.rs", 1, None),
            query("acme", "backend", SHA, "a//b.rs", 1, None),
            query("acme", "backend", SHA, "/a.rs", 1, None),
            query("acme", "backend", SHA, "a\n.rs", 1, None),
            query("acme", "backend", SHA, "", 1, None),
            query("acme", "backend", SHA, &long_path, 1, None),
            query("-acme", "backend", SHA, "a.rs", 1, None),
            query("ac/me", "backend", SHA, "a.rs", 1, None),
            query(&"a".repeat(40), "backend", SHA, "a.rs", 1, None),
            query("", "backend", SHA, "a.rs", 1, None),
            query("acme", "..", SHA, "a.rs", 1, None),
            query("acme", ".", SHA, "a.rs", 1, None),
            query("acme", "back end", SHA, "a.rs", 1, None),
            query("acme", &"r".repeat(101), SHA, "a.rs", 1, None),
        ];
        for case in cases {
            assert_eq!(parse_request(&case), None, "{case:?}");
        }
    }

    fn strings(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| line.to_string()).collect()
    }

    #[test]
    fn slice_lines_cuts_the_requested_range() {
        let text = "one\ntwo\nthree\nfour\n";
        assert_eq!(
            slice_lines(text, 2, 3),
            Some((strings(&["two", "three"]), 3, false))
        );
        assert_eq!(
            slice_lines(text, 4, 4),
            Some((strings(&["four"]), 4, false))
        );
        // 末尾の改行が無くても同じ行数
        assert_eq!(
            slice_lines("one\ntwo", 2, 2),
            Some((strings(&["two"]), 2, false))
        );
        // 末尾の改行が作る空要素は行に数えない
        assert_eq!(slice_lines(text, 5, 5), None);
        // 空行そのものは行
        assert_eq!(
            slice_lines("one\n\n", 2, 2),
            Some((strings(&[""]), 2, false))
        );
    }

    #[test]
    fn slice_lines_strips_carriage_returns() {
        assert_eq!(
            slice_lines("one\r\ntwo\r\n", 1, 2),
            Some((strings(&["one", "two"]), 2, false))
        );
    }

    #[test]
    fn slice_lines_clamps_end_and_rejects_start_past_the_file() {
        // end はファイルの行数に丸める（打ち切りではない）
        assert_eq!(
            slice_lines("one\ntwo\n", 2, 99),
            Some((strings(&["two"]), 2, false))
        );
        assert_eq!(slice_lines("one\ntwo\n", 3, 3), None);
        assert_eq!(slice_lines("", 1, 1), None);
    }

    #[test]
    fn slice_lines_truncates_at_the_line_limit() {
        let total = MAX_SNIPPET_LINES + 50;
        let text: String = (1..=total).map(|n| format!("line {n}\n")).collect();
        let (lines, end_line, truncated) = slice_lines(&text, 1, total as u32).expect("in range");
        assert_eq!(lines.len(), MAX_SNIPPET_LINES);
        assert_eq!(end_line, MAX_SNIPPET_LINES as u32);
        assert!(truncated);
        assert_eq!(lines.last().map(String::as_str), Some("line 200"));

        // 対照: ちょうど上限の行数は打ち切りにならない
        let (lines, end_line, truncated) =
            slice_lines(&text, 11, 10 + MAX_SNIPPET_LINES as u32).expect("in range");
        assert_eq!(lines.len(), MAX_SNIPPET_LINES);
        assert_eq!(end_line, 10 + MAX_SNIPPET_LINES as u32);
        assert!(!truncated);
    }

    #[test]
    fn slice_lines_truncates_at_the_char_limit() {
        // 1 行 = 999 文字 + 改行 1 つ = 1000。48 行でちょうど上限
        let line = "x".repeat(999);
        let text: String = (0..60).map(|_| format!("{line}\n")).collect();
        let (lines, end_line, truncated) = slice_lines(&text, 1, 60).expect("in range");
        assert_eq!(lines.len(), MAX_SNIPPET_CHARS / 1000);
        assert_eq!(end_line, (MAX_SNIPPET_CHARS / 1000) as u32);
        assert!(truncated);

        // UTF-16 の符号単位で数える（BMP 外の 1 文字は 2）
        let astral = "😀".repeat(MAX_SNIPPET_CHARS / 2);
        let (lines, _, truncated) = slice_lines(&astral, 1, 1).expect("in range");
        assert!(lines.is_empty());
        assert!(truncated);
    }

    #[test]
    fn slice_lines_returns_no_lines_when_one_line_exceeds_the_limit() {
        let text = format!("{}\nshort\n", "x".repeat(MAX_SNIPPET_CHARS));
        assert_eq!(slice_lines(&text, 1, 2), Some((Vec::new(), 1, true)));
    }

    #[test]
    fn cache_key_separates_installations() {
        let request =
            parse_request(&query("Acme", "Backend", SHA, "src/a.rs", 3, Some(10))).expect("valid");
        let key = cache_key(42, &request);
        assert_eq!(
            key,
            format!("{CACHE_KEY_PREFIX}42:acme/backend@{SHA}:3-10:src/a.rs")
        );
        assert_ne!(key, cache_key(43, &request));
    }
}
