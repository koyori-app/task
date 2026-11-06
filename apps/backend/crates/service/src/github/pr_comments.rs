//! PR への要約コメント（GitHub Issue Comments API）と Approve（Pull Request Reviews API）。
//!
//! レビュー指摘の一覧・状態は task 側が権威で、GitHub には**マーカー付きの
//! コメント 1 本**だけを置く。2 回目以降は同じコメントを編集して更新する。
//! マージ可になったら、レビュワー本人のトークンで、レビューした commit を本文なしで
//! Approve する（インライン投稿はしない。仕様 `docs/dev/features/review-findings/github-comment.md`）。

// 受け入れ条件（docs/dev/features/review-findings/github-comment.md から移した。テストの無い条件はページの「未実装と既知の制限」に残す）
// - 要約コメントの持ち主: それぞれのコメントを自分のマーカーで見分ける（プロジェクトごとにマーカーが違う）
// - マーカーの先取り: 作成者が自分でないコメントは自分のものとみなさない

use reqwest::{Client, Method, StatusCode};
use serde::Deserialize;
use uuid::Uuid;

use super::client::api_base;

const USER_AGENT: &str = "task-backend";
const API_VERSION: &str = "2022-11-28";

/// 自分が書いたコメントを特定するための印。本文の先頭に置く。
///
/// プロジェクト ID を含めるのは、同じリポジトリへ 2 つのプロジェクトが連携したときに
/// 互いのコメントを自分のものと誤認して交互に上書きするのを防ぐため（仕様 §7）。
#[must_use]
pub fn summary_marker(project_id: Uuid) -> String {
    format!("<!-- koyori-review-summary:{project_id} -->")
}

/// 承認の取り消し（`REQUEST_CHANGES`）に付ける印。task が出したレビューと見分け、
/// 承認し直してよいかの判定に使う。プロジェクトごとなのは [`summary_marker`] と同じ理由。
#[must_use]
pub fn revoke_marker(project_id: Uuid) -> String {
    format!("<!-- koyori-approval-revoked:{project_id} -->")
}

/// マーカー探索で読むコメントの最大ページ数（100 件 × 5 ページ）。
/// これを超えて遡らないのは、要約コメントは初回に作られるため実際には
/// 1 ページ目で見つかるのが普通で、長大な PR で全ページを舐める費用に
/// 見合わないため。見つからなければ新規投稿になる（重複は起きうるが、
/// 次回以降は新しい方が見つかって更新され続ける）。
const MAX_SEARCH_PAGES: u32 = 5;
const PER_PAGE: u32 = 100;

#[derive(Debug, Clone, Deserialize)]
struct IssueComment {
    id: i64,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    user: Option<CommentUser>,
}

#[derive(Debug, Clone, Deserialize)]
struct CommentUser {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    login: Option<String>,
    #[serde(rename = "type", default)]
    kind: Option<String>,
}

impl CommentUser {
    /// 自分（GitHub App の bot）か。login は人間のアカウントでも名乗れるので種別も見る。
    fn is_bot(&self, bot_login: &str) -> bool {
        self.kind.as_deref() == Some("Bot") && self.login.as_deref() == Some(bot_login)
    }
}

impl IssueComment {
    /// このコメントが自分（GitHub App の bot）のものか。
    ///
    /// マーカーは PR の参加者なら誰でも本文に書けるので、マーカーだけで特定すると
    /// 第三者が先取りできる——App は他人のコメントを編集できないため更新は失敗し続け、
    /// 失敗はベストエフォートで握り潰されるので正式な要約が永久に作られない（仕様 §7）。
    fn is_written_by(&self, bot_login: &str) -> bool {
        self.user.as_ref().is_some_and(|u| u.is_bot(bot_login))
    }
}

/// PR に付いたレビューのうち、自分の承認を探すのに使う部分。
#[derive(Debug, Clone, Deserialize)]
struct PullRequestReview {
    #[serde(default)]
    user: Option<CommentUser>,
    #[serde(default)]
    commit_id: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

/// 本人の同じ commit への最後の決定（APPROVED / CHANGES_REQUESTED / DISMISSED）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Approved,
    Dismissed,
    /// `by_task` は本文に取り消しの目印がある（task が出した）こと
    ChangesRequested {
        by_task: bool,
    },
}

/// 本人の同じ commit へのレビューの走査結果。
#[derive(Debug, Default)]
struct ReviewScan {
    /// APPROVED か DISMISSED が 1 件でもある
    any_approved_or_dismissed: bool,
    /// 最後の決定。COMMENTED などは無視する
    last: Option<Decision>,
}

impl ReviewScan {
    /// 一覧（古い順）の 1 ページぶんを取り込む。login ではなく数値の ID で本人を見るのは、
    /// 改名しても同じ人と分かるようにするため。
    fn absorb(
        &mut self,
        reviews: &[PullRequestReview],
        commit_sha: &str,
        approver_id: i64,
        marker: &str,
    ) {
        for r in reviews {
            if r.user.as_ref().and_then(|u| u.id) != Some(approver_id)
                || r.commit_id.as_deref() != Some(commit_sha)
            {
                continue;
            }
            let decision = match r.state.as_deref() {
                Some("APPROVED") => Decision::Approved,
                Some("DISMISSED") => Decision::Dismissed,
                Some("CHANGES_REQUESTED") => Decision::ChangesRequested {
                    by_task: r.body.as_deref().is_some_and(|b| b.contains(marker)),
                },
                _ => continue,
            };
            // 人に取り下げられた承認も含める。含めないと、取り下げた直後の遷移で
            // 同じ commit を承認し直し、人の判断と争う
            self.any_approved_or_dismissed |=
                matches!(decision, Decision::Approved | Decision::Dismissed);
            self.last = Some(decision);
        }
    }
}

/// 本人の同じ commit へのレビューを全ページ（上限あり）辿って走査する。
async fn scan_reviews(
    http: &Client,
    token: &str,
    reviews_url: &str,
    commit_sha: &str,
    approver_id: i64,
    marker: &str,
) -> Result<ReviewScan, anyhow::Error> {
    let mut scan = ReviewScan::default();
    // 探索の上限はコメントと同じ。500 件を超えるレビューが付く PR は想定しない
    for page in 1..=MAX_SEARCH_PAGES {
        let page_url = format!("{reviews_url}?per_page={PER_PAGE}&page={page}");
        let res = request(http, Method::GET, &page_url, token).send().await?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("list pr reviews failed: {status}: {body}"));
        }
        let reviews = res.json::<Vec<PullRequestReview>>().await?;
        scan.absorb(&reviews, commit_sha, approver_id, marker);
        if (reviews.len() as u32) < PER_PAGE {
            break;
        }
    }
    Ok(scan)
}

/// PR のメタ情報のうち、表示用にキャッシュするもの。
#[derive(Debug, Clone, Deserialize)]
pub struct PullRequestMeta {
    pub title: String,
    pub user: Option<PullRequestUser>,
    /// 現在の PR head。レビューした commit との照合に使う（仕様 §7）
    #[serde(default)]
    pub head: Option<PullRequestHead>,
    /// `open` / `closed`。閉じた PR は Approve しない
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequestHead {
    pub sha: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequestUser {
    pub login: String,
}

fn request(http: &Client, method: Method, url: &str, token: &str) -> reqwest::RequestBuilder {
    http.request(method, url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", API_VERSION)
        .header("User-Agent", USER_AGENT)
}

/// PR のタイトル・作者・現在の head・開閉状態を取る。
pub async fn fetch_pull_request(
    http: &Client,
    token: &str,
    owner: &str,
    repo: &str,
    number: i32,
) -> Result<PullRequestMeta, anyhow::Error> {
    let url = format!("{}/repos/{owner}/{repo}/pulls/{number}", api_base());
    let res = request(http, Method::GET, &url, token).send().await?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "fetch pull request failed: {status}: {body}"
        ));
    }
    Ok(res.json::<PullRequestMeta>().await?)
}

/// 自分が書いたマーカー付きのコメントを探す。無ければ `None`。
///
/// 条件は「マーカー一致」かつ「作成者が自分の bot」の両方。第三者が同じマーカーを
/// 書いたコメントは無視する（無視しないと、編集できないコメントを掴んで更新が
/// 失敗し続ける。仕様 §7）。
async fn find_summary_comment(
    http: &Client,
    token: &str,
    owner: &str,
    repo: &str,
    number: i32,
    marker: &str,
    bot_login: &str,
) -> Result<Option<i64>, anyhow::Error> {
    for page in 1..=MAX_SEARCH_PAGES {
        let url = format!(
            "{}/repos/{owner}/{repo}/issues/{number}/comments?per_page={PER_PAGE}&page={page}",
            api_base()
        );
        let res = request(http, Method::GET, &url, token).send().await?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("list pr comments failed: {status}: {body}"));
        }
        let comments = res.json::<Vec<IssueComment>>().await?;
        let fetched = comments.len();

        if let Some(found) = comments.into_iter().find(|c| {
            c.body.as_deref().is_some_and(|b| b.contains(marker)) && c.is_written_by(bot_login)
        }) {
            return Ok(Some(found.id));
        }
        if (fetched as u32) < PER_PAGE {
            break;
        }
    }
    Ok(None)
}

/// 要約コメントの投稿先と本文。
pub struct SummaryCommentTarget<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub number: i32,
    /// 自分のコメントを見分ける印（[`summary_marker`]）
    pub marker: &'a str,
    /// 自分の GitHub App の bot login（`{app-name}[bot]`）
    pub bot_login: &'a str,
    /// 前回投稿したコメントの控え。あれば探索を飛ばす
    pub known_comment_id: Option<i64>,
    pub body: &'a str,
}

/// 要約コメントを作るか、既にあれば同じコメントを更新する。
///
/// `known_comment_id` は前回投稿したコメントの控え。あれば探索せずに直接更新する。
/// 更新が **404（コメントが存在しない）を返したときだけ**控えを捨てて作り直す——
/// レート制限や 5xx でも作り直すと、一時障害のたびにコメントが増え、古い方が
/// 「マージ可」と書かれたまま PR に残る（仕様 §7）。
///
/// 戻り値は使ったコメント ID。呼び出し側は控えとして保存する。
pub async fn upsert_summary_comment(
    http: &Client,
    token: &str,
    target: &SummaryCommentTarget<'_>,
) -> Result<i64, anyhow::Error> {
    let SummaryCommentTarget {
        owner,
        repo,
        number,
        marker,
        bot_login,
        known_comment_id,
        body,
    } = *target;
    let payload = serde_json::json!({ "body": body });

    // 控えがあれば探索を飛ばす。無ければ（初回・控えを失ったとき）探す
    let existing = match known_comment_id {
        Some(id) => Some(id),
        None => find_summary_comment(http, token, owner, repo, number, marker, bot_login).await?,
    };

    if let Some(comment_id) = existing {
        let url = format!(
            "{}/repos/{owner}/{repo}/issues/comments/{comment_id}",
            api_base()
        );
        let res = request(http, Method::PATCH, &url, token)
            .json(&payload)
            .send()
            .await?;
        let status = res.status();
        if status.is_success() {
            return Ok(comment_id);
        }
        if status != StatusCode::NOT_FOUND {
            let text = res.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "update pr comment failed: {status}: {text}"
            ));
        }
        // 404 のときだけ「手で消された」と見なして作り直す
        tracing::info!(
            comment_id,
            pr = number,
            "summary comment is gone; creating a new one"
        );
    }

    let url = format!(
        "{}/repos/{owner}/{repo}/issues/{number}/comments",
        api_base()
    );
    let res = request(http, Method::POST, &url, token)
        .json(&payload)
        .send()
        .await?;
    let status = res.status();
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "create pr comment failed: {status}: {text}"
        ));
    }
    Ok(res.json::<IssueComment>().await?.id)
}

/// Approve の対象。
pub struct ApproveTarget<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub number: i32,
    /// レビューした commit。承認はこの commit に付ける
    pub commit_sha: &'a str,
    /// 承認する人（トークンの持ち主）の GitHub ユーザー ID
    pub approver_id: i64,
    /// 取り消しの目印（[`revoke_marker`]）。task が取り消した後なら承認し直す
    pub revoke_marker: &'a str,
}

/// [`approve_pull_request`] の結果。
#[derive(Debug, PartialEq, Eq)]
pub enum ApproveOutcome {
    Approved,
    /// 同じ commit への本人の承認（取り下げ済みを含む）が既にある
    AlreadyApproved,
    /// GitHub が受け付けなかった（422）。PR 作者本人は自分の PR を承認できない
    Unprocessable(String),
}

/// レビュワー本人のトークン（`token`）で、PR を `commit_sha` に対して Approve する。
/// 本人の承認（取り下げ済みを含む）が既にあれば何もしない。ただし最後の決定が
/// task の取り消し（目印付きの `CHANGES_REQUESTED`）なら承認し直す。
///
/// `commit_id` は必ず明示する。省くと GitHub は最新の commit に付けるので、判定と投稿の
/// 間に push されると、レビューしていない commit を承認してしまう。
pub async fn approve_pull_request(
    http: &Client,
    token: &str,
    target: &ApproveTarget<'_>,
) -> Result<ApproveOutcome, anyhow::Error> {
    let ApproveTarget {
        owner,
        repo,
        number,
        commit_sha,
        approver_id,
        revoke_marker,
    } = *target;
    let url = format!("{}/repos/{owner}/{repo}/pulls/{number}/reviews", api_base());

    let scan = scan_reviews(http, token, &url, commit_sha, approver_id, revoke_marker).await?;
    let revoked_by_task = scan.last == Some(Decision::ChangesRequested { by_task: true });
    if scan.any_approved_or_dismissed && !revoked_by_task {
        return Ok(ApproveOutcome::AlreadyApproved);
    }

    let payload = serde_json::json!({ "event": "APPROVE", "commit_id": commit_sha });
    let res = request(http, Method::POST, &url, token)
        .json(&payload)
        .send()
        .await?;
    let status = res.status();
    if status == StatusCode::UNPROCESSABLE_ENTITY {
        return Ok(ApproveOutcome::Unprocessable(
            res.text().await.unwrap_or_default(),
        ));
    }
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "approve pull request failed: {status}: {text}"
        ));
    }
    Ok(ApproveOutcome::Approved)
}

/// 承認の取り消しの対象。
pub struct RevokeTarget<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub number: i32,
    /// 取り消す承認の commit。`REQUEST_CHANGES` もこの commit に付ける
    pub commit_sha: &'a str,
    /// 承認した人（トークンの持ち主）の GitHub ユーザー ID
    pub approver_id: i64,
    /// 本文に入れる目印（[`revoke_marker`]）
    pub revoke_marker: &'a str,
    /// 指摘一覧の URL。あれば本文に添える
    pub findings_url: Option<&'a str>,
}

/// [`revoke_approval`] の結果。
#[derive(Debug, PartialEq, Eq)]
pub enum RevokeOutcome {
    Revoked,
    /// 最後の決定が承認ではない（無い・取り下げ済み・人の変更要求・取り消し済み）
    NotApproved,
    /// GitHub が受け付けなかった（422）
    Unprocessable(String),
}

/// 本人の最後の決定が APPROVED のときだけ、本人のトークンで `REQUEST_CHANGES` を出して
/// 承認を上書きする。dismiss は保護ブランチで管理者権限が要り、人の取り下げと
/// 区別できないので使わない。
pub async fn revoke_approval(
    http: &Client,
    token: &str,
    target: &RevokeTarget<'_>,
) -> Result<RevokeOutcome, anyhow::Error> {
    let RevokeTarget {
        owner,
        repo,
        number,
        commit_sha,
        approver_id,
        revoke_marker,
        findings_url,
    } = *target;
    let url = format!("{}/repos/{owner}/{repo}/pulls/{number}/reviews", api_base());

    let scan = scan_reviews(http, token, &url, commit_sha, approver_id, revoke_marker).await?;
    if scan.last != Some(Decision::Approved) {
        return Ok(RevokeOutcome::NotApproved);
    }

    let mut body = format!(
        "{revoke_marker}\ntask に未解決の High / Medium の指摘があるため、先に出した承認を取り下げます。解消してマージ可になれば承認し直します。"
    );
    if let Some(u) = findings_url {
        body.push_str(&format!("\n\n指摘一覧: {u}"));
    }
    let payload = serde_json::json!({
        "event": "REQUEST_CHANGES",
        "commit_id": commit_sha,
        "body": body,
    });
    let res = request(http, Method::POST, &url, token)
        .json(&payload)
        .send()
        .await?;
    let status = res.status();
    if status == StatusCode::UNPROCESSABLE_ENTITY {
        return Ok(RevokeOutcome::Unprocessable(
            res.text().await.unwrap_or_default(),
        ));
    }
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("revoke approval failed: {status}: {text}"));
    }
    Ok(RevokeOutcome::Revoked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_is_an_html_comment_so_it_stays_invisible() {
        let marker = summary_marker(Uuid::new_v4());
        assert!(marker.starts_with("<!--"));
        assert!(marker.ends_with("-->"));
    }

    /// マーカーはプロジェクトごとに違う（同じリポジトリを見る 2 プロジェクトが
    /// 互いのコメントを自分のものと誤認しない）。
    #[test]
    fn markers_differ_per_project() {
        assert_ne!(
            summary_marker(Uuid::new_v4()),
            summary_marker(Uuid::new_v4())
        );
    }

    /// マーカーが一致しても、書いたのが自分の bot でなければ自分のコメントではない。
    /// マーカーは PR の参加者なら誰でも本文に書ける。
    #[test]
    fn a_third_party_comment_is_not_ours() {
        let ours = IssueComment {
            id: 1,
            body: Some("marker".into()),
            user: Some(CommentUser {
                id: None,
                login: Some("koyori-task[bot]".into()),
                kind: Some("Bot".into()),
            }),
        };
        assert!(ours.is_written_by("koyori-task[bot]"));

        // 人間が同じマーカーを書いただけ
        let impostor = IssueComment {
            id: 2,
            body: Some("marker".into()),
            user: Some(CommentUser {
                id: None,
                login: Some("koyori-task[bot]".into()),
                kind: Some("User".into()),
            }),
        };
        assert!(!impostor.is_written_by("koyori-task[bot]"));

        // 別の App の bot
        let other_bot = IssueComment {
            id: 3,
            body: Some("marker".into()),
            user: Some(CommentUser {
                id: None,
                login: Some("other[bot]".into()),
                kind: Some("Bot".into()),
            }),
        };
        assert!(!other_bot.is_written_by("koyori-task[bot]"));

        // 作成者が分からないコメント
        let unknown = IssueComment {
            id: 4,
            body: Some("marker".into()),
            user: None,
        };
        assert!(!unknown.is_written_by("koyori-task[bot]"));
    }

    fn review(id: i64, commit: &str, state: &str, body: Option<&str>) -> PullRequestReview {
        PullRequestReview {
            user: Some(CommentUser {
                id: Some(id),
                login: Some("someone".into()),
                kind: Some("User".into()),
            }),
            commit_id: Some(commit.into()),
            state: Some(state.into()),
            body: body.map(Into::into),
        }
    }

    fn scan(reviews: &[PullRequestReview]) -> ReviewScan {
        let mut s = ReviewScan::default();
        s.absorb(reviews, "abc", 42, "MARK");
        s
    }

    /// 対象は「同じ GitHub ユーザー ID・同じ commit」のレビューだけ。
    #[test]
    fn only_the_approvers_reviews_of_the_same_commit_count() {
        let s = scan(&[
            review(42, "old", "APPROVED", None),
            review(7, "abc", "APPROVED", None),
        ]);
        assert_eq!(s.last, None);
        assert!(!s.any_approved_or_dismissed);

        let s = scan(&[review(42, "abc", "DISMISSED", None)]);
        assert_eq!(s.last, Some(Decision::Dismissed));
        assert!(s.any_approved_or_dismissed);
    }

    /// 最後の決定は COMMENTED を飛ばして決める。
    #[test]
    fn the_last_decision_skips_comments() {
        let s = scan(&[
            review(42, "abc", "APPROVED", None),
            review(42, "abc", "COMMENTED", None),
        ]);
        assert_eq!(s.last, Some(Decision::Approved));
    }

    /// 目印があるときだけ task の取り消しとみなす。
    #[test]
    fn changes_requested_is_ours_only_with_the_marker() {
        let ours = scan(&[
            review(42, "abc", "APPROVED", None),
            review(42, "abc", "CHANGES_REQUESTED", Some("MARK\n説明")),
        ]);
        assert_eq!(
            ours.last,
            Some(Decision::ChangesRequested { by_task: true })
        );
        assert!(ours.any_approved_or_dismissed);

        let human = scan(&[review(42, "abc", "CHANGES_REQUESTED", Some("直して"))]);
        assert_eq!(
            human.last,
            Some(Decision::ChangesRequested { by_task: false })
        );
    }

    #[test]
    fn revoke_marker_differs_per_project() {
        assert_ne!(revoke_marker(Uuid::new_v4()), revoke_marker(Uuid::new_v4()));
    }
}
