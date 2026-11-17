//! 連携リポジトリのファイルから、パーマリンクが指す行を切り出す。
//!
//! 本文に貼った GitHub のパーマリンク（コミット SHA と行範囲つきの blob URL）を、画面が
//! コードの抜粋として出すための取得口。GitHub App の Contents: Read で読み、SHA 指定で
//! 中身が変わらないので Redis に置く。Installation Access Token の取得は 1 回につき
//! GitHub へ 2 リクエスト出るため、閲覧のたびには取りに行かない。
//!
//! GitHub へのリクエストはインストールのレート制限を Issue・PR・コミットの同期と分け合う。
//! 閲覧がそれを食い潰さないよう、取れなかった結果も覚える:
//! - ファイルが無い・行として出せない: 抜粋と同じキーに短い TTL で置く（壊れたパーマリンクを
//!   開くたびに取りに行かない）
//! - インストール全体に及ぶ GitHub 側の失敗（レート制限の 403 / 429・5xx・接続の失敗）:
//!   インストール単位で止める（レート制限に当たった後も各利用者の枠が残る限り叩き続ける、
//!   ということをしない）。止める期限は延ばすだけで、後から来た短い失敗では縮めない
//! - その要求に固有の失敗（400 / 422 / 451、レート制限でない 403）: 「無い」と同じに覚える。
//!   インストールは止めない（1 つのリポジトリの失敗で、ほかのリポジトリの抜粋を止めない）
//! - その要求の一時的な失敗（タイムアウト・本文の読み取りの失敗）: 抜粋のキーに短く覚える
//!
//! 誰に見せてよいかはここでは決めない。呼び出し側（handler）が、連携行のプロジェクトに
//! 入れる利用者かを確かめてから [`snippet_for`] を呼ぶ。

use std::sync::LazyLock;
use std::time::Duration;

use base64::Engine;
use reqwest::{Client, Method, StatusCode};
use sea_orm::sea_query::{Expr, ExprTrait, Func, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, prelude::Uuid,
};
use serde::{Deserialize, Serialize};

use common::cache::redis::RedisConnection;
use common::settings::GithubAppSettings;
use entity::{github_integrations, project_members, projects, tenant_members, tenants};
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
/// 「無い」「出せない」を覚える時間。コミット SHA の指定なので結果はほぼ変わらないが、push 前の
/// コミットやインストールの対象リポジトリの変更で後から取れるようになるので短くする。
pub const NEGATIVE_CACHE_TTL_SECS: u64 = 600;
/// GitHub 側の失敗の後、そのインストールで取りに行くのを止めるキー。
pub const PAUSE_KEY_PREFIX: &str = "github:snippet:pause:";
/// 止める秒数が応答から読めないときに止める秒数。
pub const DEFAULT_PAUSE_SECS: u64 = 60;
/// タイムアウトなど、その要求の一時的な失敗を覚える秒数。
pub const TRANSIENT_CACHE_TTL_SECS: u64 = 60;
/// `Retry-After` を受け入れる上限（GitHub のレート制限の窓は 1 時間）。
pub const MAX_PAUSE_SECS: u64 = 3600;
pub const RATE_LIMIT_KEY_PREFIX: &str = "github:snippet:rl:";
/// キャッシュに無い抜粋を 1 利用者が取りに行ける回数（[`FETCH_RATE_WINDOW_SECS`] あたり）。
pub const FETCH_RATE_LIMIT: i64 = 60;
pub const FETCH_RATE_WINDOW_SECS: i64 = 600;

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// 同じリポジトリを連携しているプロジェクト（利用者に関わりのあるもの）のうち、認可を試す件数の上限。
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
    /// GitHub 側の失敗（レート制限・障害・タイムアウト）で今は取れない。秒数の間は取りに行かない
    Unavailable {
        retry_after_secs: u64,
    },
}

/// Redis に置く結果。取れた抜粋と、取り直しても変わらない「無い」「出せない」、
/// その要求の一時的な失敗（短く覚える）。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Cached {
    Found(GithubSnippetResponse),
    NotFound,
    NotDisplayable,
    /// タイムアウト・本文の読み取りの失敗。インストール全体の失敗とは限らないので、
    /// インストールは止めずにこの抜粋だけ短く止める
    Unavailable,
}

impl Cached {
    fn ttl_secs(&self) -> u64 {
        match self {
            Cached::Found(_) => CACHE_TTL_SECS,
            Cached::NotFound | Cached::NotDisplayable => NEGATIVE_CACHE_TTL_SECS,
            Cached::Unavailable => TRANSIENT_CACHE_TTL_SECS,
        }
    }
}

impl From<Cached> for SnippetOutcome {
    fn from(cached: Cached) -> Self {
        match cached {
            Cached::Found(found) => SnippetOutcome::Found(found),
            Cached::NotFound => SnippetOutcome::NotFound,
            Cached::NotDisplayable => SnippetOutcome::NotDisplayable,
            Cached::Unavailable => SnippetOutcome::Unavailable {
                retry_after_secs: TRANSIENT_CACHE_TTL_SECS,
            },
        }
    }
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
///
/// 返すのは `user_id` に関わりのあるプロジェクトだけ: オーナーかメンバーであるテナントの
/// プロジェクトと、`project_members` に行があるプロジェクト。これは候補の絞り込みであって
/// 認可ではない（入れるかどうかは呼び出し側が候補ごとに確かめる）。絞らずに件数で切ると、
/// 同じリポジトリを多くのテナントが連携しているときに、無関係なテナントの行で枠が埋まる。
pub async fn find_integrations(
    db: &DatabaseConnection,
    user_id: Uuid,
    owner: &str,
    repo: &str,
) -> Result<Vec<(github_integrations::Model, Uuid)>, DbErr> {
    let owned_tenants = Query::select()
        .column(tenants::Column::Id)
        .from(tenants::Entity)
        .and_where(tenants::Column::OwnerId.eq(user_id))
        .to_owned();
    let member_tenants = Query::select()
        .column(tenant_members::Column::TenantId)
        .from(tenant_members::Entity)
        .and_where(tenant_members::Column::UserId.eq(user_id))
        .to_owned();
    let member_projects = Query::select()
        .column(project_members::Column::ProjectId)
        .from(project_members::Entity)
        .and_where(project_members::Column::UserId.eq(user_id))
        .to_owned();
    let rows = github_integrations::Entity::find()
        .find_also_related(projects::Entity)
        .filter(
            Condition::any()
                .add(
                    Expr::col((projects::Entity, projects::Column::TenantId))
                        .in_subquery(owned_tenants),
                )
                .add(
                    Expr::col((projects::Entity, projects::Column::TenantId))
                        .in_subquery(member_tenants),
                )
                .add(
                    Expr::col((
                        github_integrations::Entity,
                        github_integrations::Column::ProjectId,
                    ))
                    .in_subquery(member_projects),
                ),
        )
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
    /// その要求の一時的な失敗（タイムアウト・本文の読み取りの失敗）
    Transient,
    /// インストール全体に及ぶ失敗。止める秒数が応答から読めればその値
    Pause {
        secs: Option<u64>,
    },
}

/// `Retry-After` の秒数（日付の形は読まない）。
fn retry_after_secs(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    header_number(headers, reqwest::header::RETRY_AFTER.as_str())
}

fn header_number(headers: &reqwest::header::HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

/// 一次レート制限を使い切った応答なら、窓が明けるまでの秒数（最低 1 秒）。
/// GitHub は一次制限に `Retry-After` を付けず、`x-ratelimit-remaining: 0` と
/// `x-ratelimit-reset`（Unix 秒）で知らせる。
fn rate_limit_reset_secs(headers: &reqwest::header::HeaderMap, now_unix: u64) -> Option<u64> {
    if header_number(headers, "x-ratelimit-remaining")? != 0 {
        return None;
    }
    let reset = header_number(headers, "x-ratelimit-reset")?;
    Some(Ord::max(reset.saturating_sub(now_unix), 1))
}

/// Contents API の 404 以外の非 2xx を、インストールを止める失敗かどうかに分ける。
///
/// 返すのは `Some(止める秒数。読めなければ None)` か、止めないなら `None`。
/// - 429 と 5xx: 止める
/// - 403: レート制限（`Retry-After` があるか、一次制限を使い切っている）のときだけ止める。
///   それ以外の 403 はそのリポジトリやファイルに固有の拒否（GitHub 側でブロックされた
///   リポジトリなど）で、待っても戻らず、ほかのリポジトリには関係しない
/// - それ以外（400 / 401 / 422 / 451 など）: その要求に固有なので止めない
fn pause_for_status(
    status: StatusCode,
    headers: &reqwest::header::HeaderMap,
    now_unix: u64,
) -> Option<Option<u64>> {
    let hinted = retry_after_secs(headers).or_else(|| rate_limit_reset_secs(headers, now_unix));
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return Some(hinted);
    }
    if status == StatusCode::FORBIDDEN && hinted.is_some() {
        return Some(hinted);
    }
    None
}

/// 止める期限を延ばす（縮めない）。返すのは、延ばした後に残っている秒数。
///
/// 止めているかの確認から GitHub の応答までの間に、同じインストールの取得が並行して進む。
/// 無条件に上書きすると、後から届いた短い失敗（既定の 60 秒）が、先に届いた長い
/// `Retry-After` を消す。`SET NX` では逆順（短い → 長い）に届いたときに長いほうが入らない。
static EXTEND_PAUSE_SCRIPT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
        local remaining = redis.call('TTL', KEYS[1])
        local wanted = tonumber(ARGV[1])
        if remaining < wanted then
            redis.call('SET', KEYS[1], '1', 'EX', wanted)
            return wanted
        end
        return remaining
        "#,
    )
});

async fn extend_pause(
    conn: &mut impl redis::aio::ConnectionLike,
    pause_key: &str,
    secs: Option<u64>,
) -> Result<u64, anyhow::Error> {
    let wanted = secs.unwrap_or(DEFAULT_PAUSE_SECS).clamp(1, MAX_PAUSE_SECS);
    let remaining: u64 = EXTEND_PAUSE_SCRIPT
        .key(pause_key)
        .arg(wanted)
        .invoke_async(conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis extend snippet pause: {e}"))?;
    Ok(remaining)
}

/// Installation Access Token の取得の失敗が「インストールが GitHub 側から消えている」か。
///
/// ponytail: forge-github がステータスをエラー型に持たないため文字列で判定している
/// （`handlers::github::is_installation_gone` と同じ判定）。型で受け取れるようになったら差し替える。
fn is_installation_gone(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("failed: 404") || message.contains("failed: 410")
}

/// Installation Access Token の取得の失敗が GitHub とのやり取りで起きたものか。
/// そうでない失敗（App の秘密鍵で JWT に署名できない、など）はこちら側の不具合なので、
/// GitHub 側の失敗（503）に混ぜずに内部エラーとして返す。
///
/// ponytail: `is_installation_gone` と同じく、forge-github が失敗の種類を型で返さないので、
/// HTTP の応答があったことを文言（`parse_ok` の `… failed: {status} …`）で、送信の失敗を
/// 原因の `reqwest::Error` で見分けている。
fn is_github_exchange_failure(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<reqwest::Error>().is_some())
        || error.to_string().contains(" failed: ")
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
    let sent = request(http, Method::GET, url.as_str(), token)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await;
    let mut res = match sent {
        Ok(res) => res,
        // 応答が時間内に来ないのはそのファイルに固有のこともあるので、インストールは止めない
        Err(error) if error.is_timeout() => {
            tracing::warn!(%error, "github contents request timed out");
            return Ok(Fetched::Transient);
        }
        // つながらないのは GitHub（または経路）の障害
        Err(error) => {
            tracing::warn!(%error, "github contents request failed");
            return Ok(Fetched::Pause { secs: None });
        }
    };
    let status = res.status();
    if status == StatusCode::NOT_FOUND {
        return Ok(Fetched::NotFound);
    }
    if !status.is_success() {
        tracing::warn!(%status, "github contents responded with an error");
        let now_unix = u64::try_from(chrono::Utc::now().timestamp()).unwrap_or(0);
        return Ok(match pause_for_status(status, res.headers(), now_unix) {
            Some(secs) => Fetched::Pause { secs },
            // その要求に固有の拒否。取り直しても変わらないので「無い」と同じに覚える
            None => Fetched::NotFound,
        });
    }
    if res
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
    {
        return Ok(Fetched::NotDisplayable);
    }
    let mut body = Vec::new();
    loop {
        let chunk = match res.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "github contents body could not be read");
                return Ok(Fetched::Transient);
            }
        };
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
    if let Some(cached) = cached
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Cached>(raw).ok())
    {
        return Ok(cached.into());
    }

    // GitHub 側の失敗の後は、そのインストールではしばらく取りに行かない（利用者の枠も数えない）
    let pause_key = format!("{PAUSE_KEY_PREFIX}{}", integration.installation_id);
    let paused_for: i64 = redis::cmd("TTL")
        .arg(&pause_key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis TTL snippet pause: {e}"))?;
    if let Ok(retry_after_secs @ 1..) = u64::try_from(paused_for) {
        return Ok(SnippetOutcome::Unavailable { retry_after_secs });
    }

    if !try_acquire_fetch_slot(&mut conn, user_id).await? {
        return Ok(SnippetOutcome::RateLimited);
    }

    let resolved = match resolve(http, settings, integration, request).await? {
        Resolved::Settled(cached) => cached,
        Resolved::Pause { secs } => {
            let retry_after_secs = extend_pause(&mut conn, &pause_key, secs).await?;
            return Ok(SnippetOutcome::Unavailable { retry_after_secs });
        }
    };
    let _: () = redis::cmd("SET")
        .arg(&key)
        .arg(serde_json::to_string(&resolved)?)
        .arg("EX")
        .arg(resolved.ttl_secs())
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis SET snippet: {e}"))?;
    Ok(resolved.into())
}

/// GitHub に聞いた結果。
enum Resolved {
    /// 取り直しても変わらない結果（Redis に置く）
    Settled(Cached),
    /// インストール全体に及ぶ GitHub 側の失敗（置かず、インストール単位で止める）
    Pause { secs: Option<u64> },
}

async fn resolve(
    http: &Client,
    settings: &GithubAppSettings,
    integration: &github_integrations::Model,
    request: &SnippetRequest,
) -> Result<Resolved, anyhow::Error> {
    let token = match installation_token(http, settings, integration.installation_id).await {
        Ok(token) => token,
        // アンインストール済み。連携行だけが残っているので、ファイルが無いのと同じに扱う
        Err(error) if is_installation_gone(&error) => {
            return Ok(Resolved::Settled(Cached::NotFound));
        }
        Err(error) if is_github_exchange_failure(&error) => {
            tracing::warn!(%error, "github installation token could not be issued");
            return Ok(Resolved::Pause { secs: None });
        }
        Err(error) => return Err(error),
    };
    let text = match fetch_file(http, &token, contents_url(integration, request)?).await? {
        Fetched::Text(text) => text,
        Fetched::NotFound => return Ok(Resolved::Settled(Cached::NotFound)),
        Fetched::NotDisplayable => return Ok(Resolved::Settled(Cached::NotDisplayable)),
        Fetched::Transient => return Ok(Resolved::Settled(Cached::Unavailable)),
        Fetched::Pause { secs } => return Ok(Resolved::Pause { secs }),
    };
    let Some((lines, end_line, truncated)) = slice_lines(&text, request.start, request.end) else {
        return Ok(Resolved::Settled(Cached::NotFound));
    };
    if lines.is_empty() {
        return Ok(Resolved::Settled(Cached::NotDisplayable));
    }
    Ok(Resolved::Settled(Cached::Found(GithubSnippetResponse {
        repo_owner: integration.repo_owner.clone(),
        repo_name: integration.repo_name.clone(),
        sha: request.sha.clone(),
        path: request.path.clone(),
        start_line: request.start,
        end_line,
        lines,
        truncated,
    })))
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
    fn cached_outcomes_round_trip_and_negative_ones_expire_sooner() {
        let snippet = GithubSnippetResponse {
            repo_owner: "acme".into(),
            repo_name: "backend".into(),
            sha: SHA.into(),
            path: "src/a.rs".into(),
            start_line: 3,
            end_line: 3,
            lines: vec!["x".into()],
            truncated: false,
        };
        let round_trip = |cached: &Cached| -> SnippetOutcome {
            let raw = serde_json::to_string(cached).expect("serialize");
            serde_json::from_str::<Cached>(&raw)
                .expect("deserialize")
                .into()
        };
        let found = Cached::Found(snippet.clone());
        assert_eq!(round_trip(&found), SnippetOutcome::Found(snippet));
        assert_eq!(round_trip(&Cached::NotFound), SnippetOutcome::NotFound);
        assert_eq!(
            round_trip(&Cached::NotDisplayable),
            SnippetOutcome::NotDisplayable
        );

        // 取れなかった結果は短く覚える（後から取れるようになることがある）
        assert_eq!(found.ttl_secs(), CACHE_TTL_SECS);
        assert_eq!(Cached::NotFound.ttl_secs(), NEGATIVE_CACHE_TTL_SECS);
        assert_eq!(Cached::NotDisplayable.ttl_secs(), NEGATIVE_CACHE_TTL_SECS);
        // その要求の一時的な失敗はもっと短く、503 として返す
        assert_eq!(Cached::Unavailable.ttl_secs(), TRANSIENT_CACHE_TTL_SECS);
        assert_eq!(
            round_trip(&Cached::Unavailable),
            SnippetOutcome::Unavailable {
                retry_after_secs: TRANSIENT_CACHE_TTL_SECS
            }
        );
        const { assert!(NEGATIVE_CACHE_TTL_SECS < CACHE_TTL_SECS) };
    }

    #[test]
    fn retry_after_reads_only_plain_seconds() {
        let headers = |value: &str| {
            let mut map = reqwest::header::HeaderMap::new();
            map.insert(reqwest::header::RETRY_AFTER, value.parse().expect("header"));
            map
        };
        assert_eq!(retry_after_secs(&headers("120")), Some(120));
        assert_eq!(retry_after_secs(&headers(" 7 ")), Some(7));
        // 日付の形と負の値は読まない（既定の秒数で止める）
        assert_eq!(
            retry_after_secs(&headers("Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(retry_after_secs(&headers("-1")), None);
        assert_eq!(retry_after_secs(&reqwest::header::HeaderMap::new()), None);
    }

    fn headers(pairs: &[(&'static str, &str)]) -> reqwest::header::HeaderMap {
        let mut map = reqwest::header::HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, value.parse().expect("header"));
        }
        map
    }

    #[test]
    fn only_installation_wide_failures_pause() {
        const NOW: u64 = 1_800_000_000;
        let none = headers(&[]);
        let pause = |status: u16, map: &reqwest::header::HeaderMap| {
            pause_for_status(StatusCode::from_u16(status).expect("status"), map, NOW)
        };

        // 止める: 429 と 5xx（秒数が読めなければ既定の秒数）
        assert_eq!(pause(429, &none), Some(None));
        assert_eq!(pause(500, &none), Some(None));
        assert_eq!(
            pause(503, &headers(&[("retry-after", "30")])),
            Some(Some(30))
        );
        // 止める: レート制限の 403（二次制限は Retry-After、一次制限は残り 0 と reset）
        assert_eq!(
            pause(403, &headers(&[("retry-after", "120")])),
            Some(Some(120))
        );
        let exhausted = headers(&[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1800000900"),
        ]);
        assert_eq!(pause(403, &exhausted), Some(Some(900)));
        assert_eq!(pause(429, &exhausted), Some(Some(900)));
        // Retry-After があればそちらを使う
        let both = headers(&[
            ("retry-after", "45"),
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1800000900"),
        ]);
        assert_eq!(pause(403, &both), Some(Some(45)));
        // reset が過去でも 0 秒にはしない
        let stale = headers(&[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1799999000"),
        ]);
        assert_eq!(pause(403, &stale), Some(Some(1)));

        // 止めない: レート制限でない 403（枠が残っている）と、その要求に固有の 4xx
        let has_quota = headers(&[
            ("x-ratelimit-remaining", "4000"),
            ("x-ratelimit-reset", "1800000900"),
        ]);
        assert_eq!(pause(403, &none), None);
        assert_eq!(pause(403, &has_quota), None);
        for status in [400, 401, 409, 422, 451] {
            assert_eq!(pause(status, &none), None, "{status}");
            assert_eq!(pause(status, &exhausted), None, "{status}");
        }
    }

    #[test]
    fn token_failures_inside_this_process_are_not_upstream_failures() {
        // forge-github の parse_ok の文言（HTTP の応答があった）
        let http = anyhow::anyhow!("github installation token failed: 500 Internal Server Error x");
        assert!(is_github_exchange_failure(&http));
        assert!(!is_installation_gone(&http));
        let gone = anyhow::anyhow!("github installation token failed: 404 Not Found");
        assert!(is_installation_gone(&gone));
        // JWT の署名の失敗など、GitHub へ届く前の失敗は内部エラーのまま
        let local = anyhow::anyhow!("InvalidKeyFormat").context("sign github app jwt");
        assert!(!is_github_exchange_failure(&local));
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
