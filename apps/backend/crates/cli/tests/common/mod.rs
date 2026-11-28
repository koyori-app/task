//! コマンドを実際に走らせるための足場。
//!
//! HTTP はモックサーバーへ向け、設定は一時ディレクトリに置く。実プロセスの
//! 環境変数（`TASK_API_URL` など）は読まないので、手元の設定でテストが揺れない。

// テストバイナリごとに使う足場が違うので、片方では未使用になるものがある。
#![allow(dead_code)]

use task_cli::cli::Cli;
use task_cli::config::{ConfigStore, TaskConfig};
use task_cli::error::Result;
use wiremock::MockServer;

pub const TENANT: &str = "99999999-9999-4999-8999-999999999999";
pub const PROJECT_ID: &str = "11111111-1111-4111-8111-111111111111";
/// 40 桁の小文字 16 進。ゲートが厳密一致で比べるので、短縮 SHA は投入時に弾かれる。
pub const HEAD_SHA: &str = "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e";

pub struct Harness {
    _home: tempfile::TempDir,
    pub server: MockServer,
    context: task_cli::Context,
}

pub async fn harness() -> Harness {
    harness_with(Some(TENANT), &[]).await
}

/// config の tenant_id と、環境変数を選べる足場。tenant の解決経路を確かめる。
pub async fn harness_with(
    tenant_id: Option<&str>,
    env: &'static [(&'static str, &'static str)],
) -> Harness {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let store = ConfigStore::from_home(home.path());
    store
        .save(&TaskConfig {
            api_url: Some(server.uri()),
            token: Some("token-1".into()),
            tenant_id: tenant_id.map(Into::into),
            ..Default::default()
        })
        .unwrap();

    let context = task_cli::Context::new(ConfigStore::from_home(home.path()), |key| {
        env.iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    });
    Harness {
        _home: home,
        server,
        context,
    }
}

/// 設定を置かない足場。検証と設定不足の、どちらが先に報告されるかを確かめる。
pub fn harness_without_config() -> (tempfile::TempDir, task_cli::Context) {
    let home = tempfile::tempdir().unwrap();
    let context = task_cli::Context::new(ConfigStore::from_home(home.path()), |_| None);
    (home, context)
}

pub fn parse(args: &[&str]) -> Cli {
    Cli::parse_args(args.iter().map(|arg| arg.to_string()))
        .expect("the test invoked the CLI with invalid arguments")
}

impl Harness {
    pub async fn run(&self, args: &[&str]) -> Result<i32> {
        let cli = Cli::parse_args(args.iter().map(|arg| arg.to_string()))
            .expect("the test invoked the CLI with invalid arguments");
        task_cli::run(cli, &self.context).await
    }

    pub fn with_updater(mut self, updater: task_cli::update::Updater) -> Self {
        self.context = self.context.with_updater(updater);
        self
    }

    pub async fn update_notice(&self) -> Option<String> {
        task_cli::update_notice(&self.context).await
    }

    pub fn store(&self) -> &ConfigStore {
        self.context.store()
    }

    /// 指定のパスへ届いたリクエストの数。
    pub async fn calls_to(&self, path: &str) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == path)
            .count()
    }

    /// 送信前に弾いたことを確かめる（1 本もリクエストが出ていないこと）。
    pub async fn sent_nothing(&self) -> bool {
        self.server
            .received_requests()
            .await
            .is_some_and(|requests| requests.is_empty())
    }
}

/// `GET /v1/personal_tokens/me` の応答。tenant を自動で引くときの元になる。
/// 束縛が 1 件のトークンの形（新しいサーバーは `tenant_id` と `tenant_ids` の両方を返す）。
pub fn token_identity_json() -> serde_json::Value {
    serde_json::json!({
        "id": "77777777-7777-4777-8777-777777777777",
        "name": "review-bot",
        "user_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "username": "yupix",
        "tenant_id": TENANT,
        "tenant_ids": [TENANT],
        "scopes": ["read:task"],
        "allowed_project_ids": null,
        "expires_at": null,
    })
}

pub fn project_json() -> serde_json::Value {
    serde_json::json!({
        "id": PROJECT_ID,
        "name": "App",
        "description": "",
        "tenant_id": TENANT,
        "icon_emoji": null,
        "icon_url": null,
        "key": "APP",
        "is_personal": false,
        "personal_owner_id": null,
        "inherit_tenant_members": true,
    })
}

pub fn status_json(
    id: &str,
    name: &str,
    is_default: bool,
    is_done_state: bool,
    is_default_done: bool,
    position: i16,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "project_id": PROJECT_ID,
        "name": name,
        "color": "#112233",
        "position": position,
        "is_default": is_default,
        "is_done_state": is_done_state,
        "is_default_done": is_default_done,
        "created_at": "2026-01-01T00:00:00Z",
    })
}

pub fn task_detail_json() -> serde_json::Value {
    serde_json::json!({
        "id": "22222222-2222-4222-8222-222222222222",
        "project_id": PROJECT_ID,
        "seq_id": 7,
        "title": "Golden task",
        "description": null,
        "status_id": "33333333-3333-4333-8333-333333333333",
        "priority": "Medium",
        "progress_pct": 0,
        "parent_task_id": null,
        "milestone_id": null,
        "sprint_id": null,
        "soft_deadline": null,
        "hard_deadline": null,
        "planned_start_on": null,
        "estimated_minutes": null,
        "is_archived": false,
        "created_by": null,
        "assignees": [],
        "labels": [],
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "completed_at": null,
        "deleted_at": null,
        "custom_field_values": [],
    })
}

pub fn summary_json(
    rounds: i32,
    blocking: u64,
    repository: Option<&str>,
    mergeable: bool,
) -> serde_json::Value {
    serde_json::json!({
        "pr_number": 618,
        "rounds": rounds,
        "counts": [],
        "blocking": blocking,
        "latest_head_sha": (rounds > 0).then_some(HEAD_SHA),
        "cached_pr_head_sha": null,
        "pr_head_checked_at": null,
        "owner_override_rejections": 0,
        "repository": repository,
        "mergeable": mergeable,
        // CLI は gate を見ない（権威のゲートは --head 照合）。形だけ合わせる
        "gate": "stale_unknown",
    })
}

pub fn finding_json() -> serde_json::Value {
    serde_json::json!({
        "id": "44444444-4444-4444-8444-444444444444",
        "review_id": "55555555-5555-4555-8555-555555555555",
        "pr_number": 618,
        "round": 2,
        "severity": "medium",
        "title": "セレクタが複数一致する",
        "body": "説明文にも一致するため",
        "file": "src/App.vue",
        "line": 42,
        "state": "open",
        "deferred_task_id": null,
        "fixed_by": null,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "transitions": [],
        "available_actions": ["fixed"],
    })
}

/// 呼ばれるたびに次の応答へ進むモック。同じ URL でサーバー側の状態変化を表す。
/// 最後の応答はそれ以降も返し続ける。
pub struct Changing(std::sync::Mutex<std::collections::VecDeque<serde_json::Value>>);

impl Changing {
    pub fn new(bodies: Vec<serde_json::Value>) -> Self {
        assert!(!bodies.is_empty(), "応答を 1 つ以上渡すこと");
        Self(std::sync::Mutex::new(bodies.into()))
    }
}

impl wiremock::Respond for Changing {
    fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let mut bodies = self.0.lock().unwrap();
        let body = if bodies.len() > 1 {
            bodies.pop_front().expect("応答が残っている")
        } else {
            bodies[0].clone()
        };
        wiremock::ResponseTemplate::new(200).set_body_json(body)
    }
}

pub fn project_path(suffix: &str) -> String {
    format!("/v1/tenants/{TENANT}/projects/{PROJECT_ID}/{suffix}")
}

/// 台本どおりに答える問い合わせ（`task init` のテスト用）。答えが尽きたら EOF（`None`）。
/// 何を聞かれたかを `asked` に残す。
pub struct ScriptedPrompt {
    interactive: bool,
    lines: std::cell::RefCell<std::collections::VecDeque<String>>,
    secrets: std::cell::RefCell<std::collections::VecDeque<String>>,
    piped: std::cell::RefCell<Option<String>>,
    asked: std::cell::RefCell<Vec<String>>,
}

impl ScriptedPrompt {
    /// 端末。`lines` は 1 行の問いへの答え、`secrets` はトークンの問いへの答え。
    pub fn terminal(lines: &[&str], secrets: &[&str]) -> std::rc::Rc<Self> {
        std::rc::Rc::new(Self {
            interactive: true,
            lines: std::cell::RefCell::new(lines.iter().map(|s| s.to_string()).collect()),
            secrets: std::cell::RefCell::new(secrets.iter().map(|s| s.to_string()).collect()),
            piped: std::cell::RefCell::new(None),
            asked: std::cell::RefCell::new(Vec::new()),
        })
    }

    /// 端末でない標準入力。`piped` は繋がっている中身（無ければ何も繋がっていない）。
    pub fn piped(piped: Option<&str>) -> std::rc::Rc<Self> {
        std::rc::Rc::new(Self {
            interactive: false,
            lines: Default::default(),
            secrets: Default::default(),
            piped: std::cell::RefCell::new(piped.map(str::to_string)),
            asked: std::cell::RefCell::new(Vec::new()),
        })
    }

    /// 聞かれた問い（`line:<文>` / `secret:<文>` / `piped:<対象>`）。
    pub fn asked(&self) -> Vec<String> {
        self.asked.borrow().clone()
    }
}

impl task_cli::prompt::Prompt for ScriptedPrompt {
    fn interactive(&self) -> bool {
        self.interactive
    }

    fn line(&self, label: &str) -> Result<Option<String>> {
        self.asked.borrow_mut().push(format!("line:{label}"));
        Ok(self.lines.borrow_mut().pop_front())
    }

    fn secret(&self, label: &str) -> Result<Option<String>> {
        self.asked.borrow_mut().push(format!("secret:{label}"));
        Ok(self.secrets.borrow_mut().pop_front())
    }

    fn piped(&self, what: &str) -> Result<Option<String>> {
        self.asked.borrow_mut().push(format!("piped:{what}"));
        Ok(self.piped.borrow_mut().take())
    }
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git is installed");
    assert!(status.success(), "git {args:?}");
}

/// 一時ディレクトリに git リポジトリを作る。`origin` を渡すとその URL を remote に足す。
pub fn git_repo(origin: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    if let Some(url) = origin {
        git(dir.path(), &["remote", "add", "origin", url]);
    }
    dir
}
