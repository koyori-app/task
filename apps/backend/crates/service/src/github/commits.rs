//! 切り詰められた push のコミットを REST で取り直す。
//!
//! `forge-github` は GitHub App のインストール周りを扱うクレートで、コミット API は
//! 持たない。push のリンクというこのアプリ固有の用途にしか使わないため、`issues.rs` と
//! 同じくこちら側に置いている。
//!
//! 取り直しは 1 回につき 1 ページだけ行う。1 回で全部読もうとすると、極端に大きな push で
//! ページ上限に当たり続け、ジョブを何度再試行しても同じ場所で失敗して進まない。

use chrono::{DateTime, Utc};
use reqwest::{Client, Method};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, prelude::Uuid};
use serde::Deserialize;

use common::settings::GithubAppSettings;
use entity::github_integrations;

use super::client::api_base;
use super::sync::installation_token;
use crate::forge::events::{ForgeCommit, ForgeRepo};

const USER_AGENT: &str = "task-backend";
const API_VERSION: &str = "2022-11-28";

/// 1 ページあたりの取得件数（GitHub の上限）。
pub const PER_PAGE: u32 = 100;

#[derive(Debug, Deserialize)]
struct CommitItem {
    sha: String,
    html_url: String,
    commit: CommitBody,
    /// git の author が GitHub アカウントに結び付いているときだけ入る
    #[serde(default)]
    author: Option<CommitUser>,
}

#[derive(Debug, Deserialize)]
struct CommitBody {
    message: String,
    author: CommitBodyAuthor,
}

#[derive(Debug, Deserialize)]
struct CommitBodyAuthor {
    name: String,
    date: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CommitUser {
    login: String,
}

/// `GET /repos/{owner}/{repo}/compare/{before}...{after}` の応答（使う分だけ）。
#[derive(Debug, Deserialize)]
struct CompareResponse {
    /// 比較に含まれるコミットの総数。ページングの終端判定に使う
    total_commits: u32,
    commits: Vec<CommitItem>,
}

impl From<CommitItem> for ForgeCommit {
    fn from(item: CommitItem) -> Self {
        ForgeCommit {
            sha: item.sha.to_ascii_lowercase(),
            message: item.commit.message,
            author_handle: item.author.map(|user| user.login).unwrap_or_default(),
            author_name: item.commit.author.name,
            committed_at: item.commit.author.date,
            html_url: item.html_url,
        }
    }
}

/// 取り直しの 1 ページ分。
#[derive(Debug)]
pub struct BackfillPage {
    pub commits: Vec<ForgeCommit>,
    /// 続きがあるなら次に読むページ
    pub next_page: Option<u32>,
}

/// 取り直す範囲（正規化イベントとジョブの続きから決まる）。
#[derive(Debug, Clone, Copy)]
pub struct PushRange<'a> {
    pub repo: &'a ForgeRepo,
    /// 比較の起点。ブランチを作った push では `None`
    pub before: Option<&'a str>,
    pub after: &'a str,
    pub page: u32,
}

fn request(http: &Client, method: Method, url: &str, token: &str) -> reqwest::RequestBuilder {
    http.request(method, url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", API_VERSION)
        .header("User-Agent", USER_AGENT)
}

async fn get_json<T: serde::de::DeserializeOwned>(
    http: &Client,
    token: &str,
    url: &str,
    context: &str,
) -> Result<T, anyhow::Error> {
    let res = request(http, Method::GET, url, token).send().await?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("{context} failed: {status}: {body}"));
    }
    Ok(res.json().await?)
}

/// `before...after` の差分を 1 ページ取る。
///
/// push で増えたコミットはこの比較そのもの。`after` からの履歴を辿ると、マージを含む
/// 履歴では別の親から辿れる push 前のコミットまで混ざる（日付順に並ぶため、受信済みの
/// 最新コミットより先に出てくることがある）。
async fn compare_page(
    http: &Client,
    token: &str,
    owner: &str,
    repo: &str,
    before: &str,
    after: &str,
    page: u32,
) -> Result<BackfillPage, anyhow::Error> {
    let url = format!(
        "{}/repos/{owner}/{repo}/compare/{before}...{after}?per_page={PER_PAGE}&page={page}",
        api_base()
    );
    let body: CompareResponse = get_json(http, token, &url, "compare commits").await?;

    let fetched = body.commits.len() as u32;
    let next_page = (fetched > 0 && page * PER_PAGE < body.total_commits).then_some(page + 1);
    Ok(BackfillPage {
        commits: body.commits.into_iter().map(ForgeCommit::from).collect(),
        next_page,
    })
}

/// `after` から履歴を 1 ページ辿る（新しい順）。
///
/// ブランチを作った push には比較の起点が無いので、こちらで辿る。
///
/// ponytail: 走査はルートまで行く。1 回 1 ページなので詰まらないが、ジョブ数は履歴の
/// 長さに比例する。既定ブランチとの比較で打ち切るなら、その分だけ実装を足す。
async fn history_page(
    http: &Client,
    token: &str,
    owner: &str,
    repo: &str,
    after: &str,
    page: u32,
) -> Result<BackfillPage, anyhow::Error> {
    let url = format!(
        "{}/repos/{owner}/{repo}/commits?sha={after}&per_page={PER_PAGE}&page={page}",
        api_base()
    );
    let items: Vec<CommitItem> = get_json(http, token, &url, "list commits").await?;

    let next_page = (items.len() as u32 == PER_PAGE).then_some(page + 1);
    Ok(BackfillPage {
        commits: items.into_iter().map(ForgeCommit::from).collect(),
        next_page,
    })
}

/// 切り詰められた push の差分コミットを 1 ページ取り直す。
pub async fn fetch_push_commits_page(
    db: &DatabaseConnection,
    http: &Client,
    settings: &GithubAppSettings,
    project_id: Uuid,
    range: PushRange<'_>,
) -> Result<BackfillPage, anyhow::Error> {
    let integration = github_integrations::Entity::find()
        .filter(github_integrations::Column::ProjectId.eq(project_id))
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("project {project_id} has no github integration"))?;
    let token = installation_token(http, settings, integration.installation_id).await?;

    let owner = &range.repo.repo_owner;
    let repo = &range.repo.repo_name;
    match range.before {
        Some(before) => {
            compare_page(http, &token, owner, repo, before, range.after, range.page).await
        }
        None => history_page(http, &token, owner, repo, range.after, range.page).await,
    }
}
