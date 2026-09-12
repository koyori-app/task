//! GitHub の webhook ペイロードを正規化イベントへ変換する受信口
//! （docs/features/tasks/9.github-tasks.md §5）。
//!
//! GitHub 固有の語彙（installation / sender / pusher 等）はここから外へ出さない。
//! 変換後の型に無いフィールドは、ジョブにもタスク側の処理にも届かない。

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::forge::events::{ForgeCommit, ForgeEvent, ForgeRepo};

pub const HOST: &str = "github";
/// GitHub App の連携先は github.com だけなので固定する
pub const HOST_URL: &str = "https://github.com";

/// GitHub が push の `commits` に載せる最大件数。
///
/// 「The array includes a maximum of 2048 commits.」
/// <https://docs.github.com/en/webhooks/webhook-events-and-payloads#push>
///
/// ここに達した push は残りが欠けているので、ワーカーが API で取り直す。
/// ペイロードには総件数を示すフィールドが無いため、この件数そのものを合図に使う。
const COMMITS_MAX: usize = 2048;

#[derive(Deserialize)]
struct PushPayload {
    #[serde(rename = "ref")]
    ref_name: String,
    #[serde(default)]
    forced: bool,
    /// push 後の ref の先頭コミット
    after: String,
    repository: PushRepository,
    #[serde(default)]
    commits: Vec<PushCommit>,
}

#[derive(Deserialize)]
struct PushRepository {
    name: String,
    owner: PushOwner,
}

#[derive(Deserialize)]
struct PushOwner {
    login: String,
}

#[derive(Deserialize)]
struct PushCommit {
    id: String,
    message: String,
    timestamp: DateTime<Utc>,
    url: String,
    author: PushCommitAuthor,
}

#[derive(Deserialize)]
struct PushCommitAuthor {
    name: String,
    /// 作者のメールアドレスが GitHub アカウントに結び付いているときだけ入る
    #[serde(default)]
    username: Option<String>,
}

/// `push` イベントを正規化する。
pub fn push_event(payload: &serde_json::Value) -> Result<ForgeEvent, serde_json::Error> {
    let push = PushPayload::deserialize(payload)?;
    Ok(ForgeEvent::Push {
        repo: ForgeRepo {
            host: HOST.to_string(),
            host_url: HOST_URL.to_string(),
            repo_owner: push.repository.owner.login,
            repo_name: push.repository.name,
        },
        ref_name: push.ref_name,
        forced: push.forced,
        after: push.after.to_ascii_lowercase(),
        commits_truncated: push.commits.len() >= COMMITS_MAX,
        commits: push
            .commits
            .into_iter()
            .map(|c| ForgeCommit {
                sha: c.id.to_ascii_lowercase(),
                message: c.message,
                author_handle: c.author.username.unwrap_or_default(),
                author_name: c.author.name,
                committed_at: c.timestamp,
                html_url: c.url,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GitHub の push ペイロード（関係するフィールドを残して削ったもの）
    fn push_fixture() -> serde_json::Value {
        serde_json::json!({
            "ref": "refs/heads/main",
            "before": "0000000000000000000000000000000000000000",
            "after": "A3F92C1E7B81D4000000000000000000000000AA",
            "created": false,
            "deleted": false,
            "forced": true,
            "repository": {
                "id": 1,
                "name": "backend",
                "full_name": "acme/backend",
                "html_url": "https://github.com/acme/backend",
                "owner": { "name": "acme", "login": "acme", "id": 10 }
            },
            "pusher": { "name": "yupix", "email": "yupix@example.com" },
            "sender": { "login": "yupix", "id": 20 },
            "installation": { "id": 30 },
            "commits": [
                {
                    "id": "A3F92C1E7B81D4000000000000000000000000AA",
                    "tree_id": "b1",
                    "distinct": true,
                    "message": "fix: トークン期限切れを修正 TASK-1\n\n本文",
                    "timestamp": "2026-09-10T12:00:00+09:00",
                    "url": "https://github.com/acme/backend/commit/a3f92c1e7b81d4000000000000000000000000aa",
                    "author": { "name": "Yupix", "email": "yupix@example.com", "username": "yupix" },
                    "committer": { "name": "GitHub", "email": "noreply@github.com", "username": "web-flow" },
                    "added": [], "removed": [], "modified": ["src/main.rs"]
                },
                {
                    "id": "e7b81d4000000000000000000000000000000bb",
                    "tree_id": "b2",
                    "distinct": true,
                    "message": "chore: 作者が GitHub アカウントに結び付いていない",
                    "timestamp": "2026-09-10T03:00:00Z",
                    "url": "https://github.com/acme/backend/commit/e7b81d4000000000000000000000000000000bb",
                    "author": { "name": "someone", "email": "someone@example.com" },
                    "committer": { "name": "someone", "email": "someone@example.com" },
                    "added": [], "removed": [], "modified": []
                }
            ],
            "head_commit": null
        })
    }

    #[test]
    fn push_payload_becomes_normalized_event() {
        let event = push_event(&push_fixture()).expect("convert push");
        assert_eq!(
            event,
            ForgeEvent::Push {
                repo: ForgeRepo {
                    host: "github".into(),
                    host_url: "https://github.com".into(),
                    repo_owner: "acme".into(),
                    repo_name: "backend".into(),
                },
                ref_name: "refs/heads/main".into(),
                forced: true,
                after: "a3f92c1e7b81d4000000000000000000000000aa".into(),
                commits_truncated: false,
                commits: vec![
                    ForgeCommit {
                        sha: "a3f92c1e7b81d4000000000000000000000000aa".into(),
                        message: "fix: トークン期限切れを修正 TASK-1\n\n本文".into(),
                        author_handle: "yupix".into(),
                        author_name: "Yupix".into(),
                        // オフセット付きの時刻は UTC に揃える
                        committed_at: "2026-09-10T03:00:00Z".parse().unwrap(),
                        html_url: "https://github.com/acme/backend/commit/a3f92c1e7b81d4000000000000000000000000aa".into(),
                    },
                    ForgeCommit {
                        sha: "e7b81d4000000000000000000000000000000bb".into(),
                        message: "chore: 作者が GitHub アカウントに結び付いていない".into(),
                        author_handle: String::new(),
                        author_name: "someone".into(),
                        committed_at: "2026-09-10T03:00:00Z".parse().unwrap(),
                        html_url: "https://github.com/acme/backend/commit/e7b81d4000000000000000000000000000000bb".into(),
                    },
                ],
            }
        );
    }

    /// ブランチ削除の push は commits が空で届く
    #[test]
    fn push_without_commits_is_accepted() {
        let mut payload = push_fixture();
        payload["deleted"] = true.into();
        payload["commits"] = serde_json::json!([]);
        let ForgeEvent::Push { commits, .. } = push_event(&payload).expect("convert push");
        assert!(commits.is_empty());
    }

    /// 上限ちょうどで届いた push は、新しい側が欠けている合図として切り詰め扱いにする
    /// （ペイロードに総件数を示すフィールドが無いので、件数そのものを合図に使う）
    #[test]
    fn push_at_the_commit_cap_is_marked_truncated() {
        let mut payload = push_fixture();
        let commit = payload["commits"][0].clone();
        payload["commits"] = serde_json::Value::Array(vec![commit; COMMITS_MAX]);

        let ForgeEvent::Push {
            commits,
            commits_truncated,
            after,
            ..
        } = push_event(&payload).expect("convert push");
        assert_eq!(commits.len(), COMMITS_MAX);
        assert!(commits_truncated, "上限に達した push は取り直しの対象");
        assert_eq!(after, "a3f92c1e7b81d4000000000000000000000000aa");
    }

    #[test]
    fn payload_missing_repository_is_rejected() {
        let mut payload = push_fixture();
        payload.as_object_mut().unwrap().remove("repository");
        assert!(push_event(&payload).is_err());
    }
}
