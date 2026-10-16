//! タスクに結ばれた PR / コミット / ブランチ（docs/features/tasks/9.github-tasks.md §8）。

use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// `GET /tasks/{id}/forge`。3 配列は空でも必ず返す
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskForgeResponse {
    pub pull_requests: Vec<TaskForgePullRequest>,
    pub commits: Vec<TaskForgeCommit>,
    /// ブランチのリンクは未実装（TASK-291）で、常に空
    pub branches: Vec<TaskForgeBranch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskForgePullRequest {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    /// `github` / `gitlab` / `forgejo`
    pub host: String,
    pub host_url: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub number: i32,
    pub title: String,
    pub author_handle: String,
    /// `open` / `closed` / `merged` / `unknown`
    pub state: String,
    /// ホスト上の PR の URL。組み立てずにそのまま使う
    pub html_url: String,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub merged_at: Option<DateTime<Utc>>,
    /// マージ時の副作用（タスク完了）を処理した時刻。`state = merged` で NULL なら未適用
    #[schema(value_type = Option<String>, format = "date-time")]
    pub merge_effects_processed_at: Option<DateTime<Utc>>,
    /// PR のコミット一覧を読み切れているか
    pub commits_complete: bool,
    /// このタスクを参照している箇所（`title` / `body` / `branch` / `manual`）。昇順
    pub sources: Vec<String>,
    /// どれかの参照元がクローズキーワード付きか
    pub closes: bool,
    /// マージを塞いでいるレビュー指摘（High / Medium の open / fixing / fixed）の件数。
    /// レビューの無い PR、タスクと別プロジェクトの PR 行、`read:review` を持たない PAT では null
    pub unresolved: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskForgeCommit {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub host: String,
    pub host_url: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub sha: String,
    /// メッセージの先頭行
    pub message: String,
    /// ホスト上のアカウント名。解決できなければ空文字列
    pub author_handle: String,
    /// git の author 名
    pub author_name: String,
    #[schema(value_type = String, format = "date-time")]
    pub committed_at: DateTime<Utc>,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskForgeBranch {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub host: String,
    pub host_url: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub branch: String,
    pub html_url: String,
}
