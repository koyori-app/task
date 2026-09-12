//! push で切り詰められたコミットを REST で取り直す。
//!
//! `forge-github` は GitHub App のインストール周りを扱うクレートで、コミット API は
//! 持たない。push のリンクというこのアプリ固有の用途にしか使わないため、`issues.rs` と
//! 同じくこちら側に置いている。

use std::collections::HashSet;

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
const PER_PAGE: usize = 100;

/// 遡るページ数の上限。
///
/// 上限に当たったら取りこぼしたまま成功させず、エラーにして再試行させる。
/// 黙って打ち切ると、その push の残りのコミットは二度とタスクに結ばれない。
const MAX_PAGES: usize = 60;

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

fn request(http: &Client, method: Method, url: &str, token: &str) -> reqwest::RequestBuilder {
    http.request(method, url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", API_VERSION)
        .header("User-Agent", USER_AGENT)
}

/// `head_sha` から遡り、`stop_sha` に当たる手前までのコミットを新しい順に返す。
///
/// `stop_sha` に届く前に履歴が尽きたりページ上限に当たったら、取りこぼしを隠さず
/// エラーにする（呼び出し元のジョブを再試行させる）。
async fn list_commits_until(
    http: &Client,
    token: &str,
    owner: &str,
    repo: &str,
    head_sha: &str,
    stop_sha: &str,
) -> Result<Vec<ForgeCommit>, anyhow::Error> {
    let mut collected = Vec::new();
    for page in 1..=MAX_PAGES {
        let url = format!(
            "{}/repos/{owner}/{repo}/commits?sha={head_sha}&per_page={PER_PAGE}&page={page}",
            api_base()
        );
        let res = request(http, Method::GET, &url, token).send().await?;

        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("list commits failed: {status}: {body}"));
        }
        let items: Vec<CommitItem> = res.json().await?;
        let fetched = items.len();
        for item in items {
            if item.sha.eq_ignore_ascii_case(stop_sha) {
                return Ok(collected);
            }
            collected.push(ForgeCommit::from(item));
        }
        if fetched < PER_PAGE {
            break;
        }
    }
    Err(anyhow::anyhow!(
        "{owner}/{repo}: could not reach {stop_sha} from {head_sha}; push commits are incomplete"
    ))
}

/// 切り詰められた push に、欠けているコミットを足して古い順で返す。
///
/// 受信したコミットは古い順で、欠けるのは新しい側。受信済みの最新コミットまで
/// `after` から遡れば、その間が欠けている分になる。
pub async fn backfill_push_commits(
    db: &DatabaseConnection,
    http: &Client,
    settings: &GithubAppSettings,
    project_id: Uuid,
    repo: &ForgeRepo,
    after: &str,
    commits: &[ForgeCommit],
) -> Result<Vec<ForgeCommit>, anyhow::Error> {
    // 1 件も届いていなければ遡る起点が決まらない（ブランチ削除の push など）
    let Some(newest) = commits.last() else {
        return Ok(commits.to_vec());
    };

    let integration = github_integrations::Entity::find()
        .filter(github_integrations::Column::ProjectId.eq(project_id))
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("project {project_id} has no github integration"))?;
    let token = installation_token(http, settings, integration.installation_id).await?;

    let mut missing = list_commits_until(
        http,
        &token,
        &repo.repo_owner,
        &repo.repo_name,
        after,
        &newest.sha,
    )
    .await?;
    missing.reverse();

    // 同じ SHA を二重に積まない（境界の重なりや、押し戻された履歴での再取得）
    let known: HashSet<&str> = commits.iter().map(|commit| commit.sha.as_str()).collect();
    let mut merged = commits.to_vec();
    merged.extend(
        missing
            .into_iter()
            .filter(|commit| !known.contains(commit.sha.as_str())),
    );
    Ok(merged)
}
