//! 受信口が作る正規化イベント（docs/features/tasks/9.github-tasks.md §5）。
//! タスク側の処理はこの型だけを見て、どのホストから来たかで分岐しない。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// リポジトリの識別（§2 の共通列）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeRepo {
    pub host: String,
    pub host_url: String,
    pub repo_owner: String,
    pub repo_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeCommit {
    /// 小文字 16 進
    pub sha: String,
    pub message: String,
    /// ホスト上のアカウント名。解決できなければ空文字列
    pub author_handle: String,
    /// git の author 名
    pub author_name: String,
    pub committed_at: DateTime<Utc>,
    pub html_url: String,
}

/// ジョブのペイロードとして永続化されるので、機微情報を持たせない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ForgeEvent {
    Push {
        repo: ForgeRepo,
        ref_name: String,
        forced: bool,
        /// push 前の ref の先頭コミット（小文字 16 進）。
        /// ブランチの新規作成では `None`（比較の起点が無い）
        before: Option<String>,
        /// push 後の ref の先頭コミット（小文字 16 進）。
        /// 切り詰められた `commits` を API で埋めるときの終点にする
        after: String,
        /// ホストが `commits` を上限で切り詰めている。
        /// 足りない分は `after` から遡って取り直さないと、そのコミットは二度と処理されない
        commits_truncated: bool,
        /// 古い順。`commits_truncated` のときは新しい側が欠けている
        commits: Vec<ForgeCommit>,
    },
    /// PR の作成・更新。本文は公開情報なので運んでよい（タスク参照の抽出に使う）
    PullRequest {
        repo: ForgeRepo,
        action: PrAction,
        number: i32,
        title: String,
        body: String,
        author_handle: String,
        /// `open` / `closed`（マージ済みかは `merged_at` で表す）
        state: String,
        merged_at: Option<DateTime<Utc>>,
        /// 小文字 16 進
        head_sha: String,
        head_branch: String,
        html_url: String,
        /// ホスト側で PR が最後に更新された時刻。配送順の逆転を見分ける世代に使う（§5）
        updated_at: DateTime<Utc>,
    },
    /// ブランチの作成。タグは受信口で捨てる
    BranchCreated {
        repo: ForgeRepo,
        branch: String,
        /// ホスト上のブランチの URL。ホストごとに組み立て規則が違うので受信口で作る
        html_url: String,
    },
    BranchDeleted {
        repo: ForgeRepo,
        branch: String,
    },
}

/// 処理対象の PR action。それ以外の action は受信口で捨てる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrAction {
    Opened,
    Edited,
    Synchronize,
    Reopened,
    Closed { merged: bool },
}
