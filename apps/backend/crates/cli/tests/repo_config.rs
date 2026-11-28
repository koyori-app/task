//! リポジトリ内の `.koyori-task.yaml` から `--project` の既定値を補う経路と、
//! 要求を送るどの口でも先にファイルを読んで止まることを確かめる。

mod common;

use common::*;
use serde_json::json;
use task_cli::cli::Cli;
use task_cli::config::{ConfigStore, TaskConfig};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TASK_UUID: &str = "22222222-2222-4222-8222-222222222222";

/// 一時ディレクトリに `.git/` と（あれば）`.koyori-task.yaml` を置き、そこを起点にした Context。
struct RepoHarness {
    _home: tempfile::TempDir,
    _repo: tempfile::TempDir,
    server: MockServer,
    context: task_cli::Context,
}

struct Setup {
    file: Option<&'static str>,
    /// config に置く tenant_id。`None` は `/me` から引く経路
    tenant: Option<&'static str>,
    /// api_url と token を config ではなく env から渡す
    env_only: bool,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            file: None,
            tenant: Some(TENANT),
            env_only: false,
        }
    }
}

fn with_file(file: &'static str) -> Setup {
    Setup {
        file: Some(file),
        ..Setup::default()
    }
}

async fn repo_harness(setup: Setup) -> RepoHarness {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    if let Some(file) = setup.file {
        std::fs::write(repo.path().join(".koyori-task.yaml"), file).unwrap();
    }
    let nested = repo.path().join("src/deep");
    std::fs::create_dir_all(&nested).unwrap();

    let uri = server.uri();
    let env: Vec<(String, String)> = if setup.env_only {
        vec![
            ("TASK_API_URL".into(), uri.clone()),
            ("TASK_TOKEN".into(), "token-1".into()),
            ("TASK_TENANT".into(), TENANT.into()),
        ]
    } else {
        ConfigStore::from_home(home.path())
            .save(&TaskConfig {
                api_url: Some(uri),
                token: Some("token-1".into()),
                tenant_id: setup.tenant.map(Into::into),
            })
            .unwrap();
        vec![]
    };
    let context = task_cli::Context::new(ConfigStore::from_home(home.path()), move |key| {
        env.iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    })
    .with_repo_dir(nested);
    RepoHarness {
        _home: home,
        _repo: repo,
        server,
        context,
    }
}

impl RepoHarness {
    async fn run(&self, args: &[&str]) -> task_cli::error::Result<i32> {
        let cli = Cli::parse_args(args.iter().map(|arg| arg.to_string()))
            .expect("the test invoked the CLI with invalid arguments");
        task_cli::run(cli, &self.context).await
    }

    async fn calls_to(&self, path: &str) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == path)
            .count()
    }

    async fn sent_nothing(&self) -> bool {
        self.server
            .received_requests()
            .await
            .is_some_and(|requests| requests.is_empty())
    }

    async fn mount_get(&self, path_: String, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(path_))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    async fn mount_project_lookup(&self) {
        self.mount_get(projects_path(), json!([project_json()]))
            .await;
        self.mount_get(
            format!("/v1/tenants/{TENANT}/projects/{PROJECT_ID}"),
            project_json(),
        )
        .await;
    }

    /// `tasks list` に要るもの（プロジェクト・状態・一覧）。
    async fn mount_task_list(&self) {
        self.mount_project_lookup().await;
        self.mount_get(project_path("statuses"), json!([])).await;
        self.mount_get(project_path("tasks"), json!({ "tasks": [], "total": 0 }))
            .await;
    }
}

fn projects_path() -> String {
    format!("/v1/tenants/{TENANT}/projects")
}

/// `--project` が必須だった口が、ファイルの `project` で通る。修正前は引数の解析で落ちる。
#[tokio::test]
async fn tasks_list_uses_the_project_in_the_file() {
    let harness = repo_harness(with_file("project: APP\n")).await;
    harness.mount_task_list().await;

    assert_eq!(harness.run(&["task", "tasks", "list"]).await.unwrap(), 0);
    assert_eq!(harness.calls_to(&projects_path()).await, 1);
    assert_eq!(harness.calls_to(&project_path("tasks")).await, 1);
}

#[tokio::test]
async fn review_sprints_and_project_reads_use_the_project_in_the_file() {
    let harness = repo_harness(with_file("project: APP\n")).await;
    harness.mount_project_lookup().await;
    harness
        .mount_get(project_path("review-findings"), json!([finding_json()]))
        .await;
    harness.mount_get(project_path("sprints"), json!([])).await;
    harness
        .mount_get(
            project_path("statuses"),
            json!([status_json(
                "33333333-3333-4333-8333-333333333333",
                "Todo",
                true,
                false,
                false,
                0
            )]),
        )
        .await;

    for args in [
        &["task", "review", "list", "--pr", "618"][..],
        &["task", "sprints", "list"],
        &["task", "projects", "statuses"],
    ] {
        assert_eq!(harness.run(args).await.unwrap(), 0, "{args:?}");
    }
    assert_eq!(harness.calls_to(&project_path("review-findings")).await, 1);
    assert_eq!(harness.calls_to(&project_path("sprints")).await, 1);
    assert_eq!(harness.calls_to(&project_path("statuses")).await, 1);
}

/// UUID の `project` なら、通知設定はテナントの API を叩かない（明示の UUID と同じ）。
#[tokio::test]
async fn notification_settings_use_a_uuid_project_in_the_file_without_the_tenant_api() {
    let harness = repo_harness(Setup {
        file: Some("project: 11111111-1111-4111-8111-111111111111\n"),
        tenant: None,
        ..Setup::default()
    })
    .await;
    harness
        .mount_get(
            format!("/v1/users/me/notification-settings/{PROJECT_ID}"),
            json!({ "email_events": [], "in_app_events": ["assigned"] }),
        )
        .await;

    assert_eq!(
        harness
            .run(&["task", "notifications", "settings"])
            .await
            .unwrap(),
        0
    );
    let requests = harness.server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|request| !request.url.path().starts_with("/v1/tenants/")
                && request.url.path() != "/v1/personal_tokens/me"),
        "{:?}",
        requests.iter().map(|r| r.url.path()).collect::<Vec<_>>()
    );
}

/// タスク UUID の参照は所属プロジェクトを含まないので、ファイルの `project` で補う。
#[tokio::test]
async fn a_task_uuid_uses_the_project_in_the_file() {
    let harness = repo_harness(with_file("project: APP\n")).await;
    harness.mount_project_lookup().await;
    harness
        .mount_get(
            project_path(&format!("tasks/{TASK_UUID}")),
            task_detail_json(),
        )
        .await;
    harness.mount_get(project_path("statuses"), json!([])).await;

    assert_eq!(
        harness
            .run(&["task", "tasks", "show", TASK_UUID])
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        harness
            .calls_to(&project_path(&format!("tasks/{TASK_UUID}")))
            .await,
        1
    );
}

/// ファイルが無く `--project` も無いと、送る前に終了 2 で止まり、書き方を案内する。
#[tokio::test]
async fn without_the_file_a_missing_project_stops_before_sending() {
    let harness = repo_harness(Setup::default()).await;

    for args in [
        &["task", "tasks", "list"][..],
        &["task", "review", "list", "--pr", "618"],
        &["task", "sprints", "list"],
        &["task", "notifications", "settings"],
    ] {
        let err = harness.run(args).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{args:?}");
        assert!(err.message.contains(".koyori-task.yaml"), "{}", err.message);
    }
    let err = harness
        .run(&["task", "tasks", "show", TASK_UUID])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 2);
    assert!(
        err.message.contains("--project is required"),
        "{}",
        err.message
    );
    assert!(err.message.contains(".koyori-task.yaml"), "{}", err.message);
    assert!(harness.sent_nothing().await);
}

/// `project` の無いファイル（未知のキーだけ）も、ファイルが無いのと同じ。
#[tokio::test]
async fn a_file_without_a_project_still_requires_one() {
    let harness = repo_harness(with_file("other: 1\n")).await;
    let err = harness.run(&["task", "tasks", "list"]).await.unwrap_err();
    assert_eq!(err.exit_code, 2);
    assert!(harness.sent_nothing().await);
}

/// `--project` を明示したら、ファイルの `project` は使わない。
#[tokio::test]
async fn an_explicit_project_wins_over_the_file() {
    let harness = repo_harness(with_file("project: OTHER\n")).await;
    harness.mount_task_list().await;

    assert_eq!(
        harness
            .run(&["task", "tasks", "list", "--project", "APP"])
            .await
            .unwrap(),
        0
    );
    // OTHER を引いていたら一覧に無く終了 5 になる
    assert_eq!(harness.calls_to(&project_path("tasks")).await, 1);
}

/// `KEY-N` は参照内のキーを使い、ファイルの `project` を見ない。
#[tokio::test]
async fn a_key_n_reference_ignores_the_project_in_the_file() {
    let harness = repo_harness(with_file("project: OTHER\n")).await;
    harness.mount_project_lookup().await;
    harness
        .mount_get(project_path("tasks/APP-7"), task_detail_json())
        .await;
    harness.mount_get(project_path("statuses"), json!([])).await;

    assert_eq!(
        harness
            .run(&["task", "tasks", "show", "APP-7"])
            .await
            .unwrap(),
        0
    );
    assert_eq!(harness.calls_to(&project_path("tasks/APP-7")).await, 1);
}

/// `profile` はまだ扱えない。要求を送るどの口でも、1 本も送らずに終了 2 で止まる。
#[tokio::test]
async fn a_profile_stops_every_command_that_sends_before_sending() {
    for env_only in [false, true] {
        let harness = repo_harness(Setup {
            file: Some("project: APP\nprofile: work\n"),
            env_only,
            ..Setup::default()
        })
        .await;
        for args in [
            &["task", "tasks", "list", "--project", "APP"][..],
            &["task", "tasks", "list"],
            &["task", "tasks", "show", "APP-7"],
            &["task", "auth", "whoami"],
            &["task", "notifications", "list"],
            &["task", "tenants", "list"],
            &["task", "projects", "list"],
            &["task", "my", "list"],
        ] {
            let err = harness.run(args).await.unwrap_err();
            assert_eq!(err.exit_code, 2, "{args:?} env_only={env_only}");
            assert!(err.message.contains("`profile`"), "{}", err.message);
        }
        assert!(harness.sent_nothing().await);
    }
}

/// ファイルを読まない口（設定と手元のトークン操作）は、`profile` や壊れたファイルがあっても動く。
#[tokio::test]
async fn config_and_local_token_commands_do_not_read_the_file() {
    for file in ["profile: work\n", "project: [broken\n"] {
        let harness = repo_harness(with_file(file)).await;
        assert_eq!(harness.run(&["task", "config", "list"]).await.unwrap(), 0);
        assert_eq!(harness.run(&["task", "auth", "logout"]).await.unwrap(), 0);
        assert!(harness.sent_nothing().await);
    }
}

/// 壊れたファイルは「未指定」に落とさず止める（`--project` を明示していても）。
#[tokio::test]
async fn a_broken_file_stops_before_sending() {
    let harness = repo_harness(with_file("project: [broken\n")).await;
    let err = harness
        .run(&["task", "tasks", "list", "--project", "APP"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 2);
    assert!(err.message.contains("Cannot parse"), "{}", err.message);
    assert!(err.message.contains(".koyori-task.yaml"), "{}", err.message);
    assert!(harness.sent_nothing().await);
}

/// 未知のキーは警告だけで続ける。
#[tokio::test]
async fn unknown_keys_do_not_stop_the_command() {
    let harness = repo_harness(with_file("future_key: 1\n")).await;
    harness.mount_task_list().await;
    assert_eq!(
        harness
            .run(&["task", "tasks", "list", "--project", "APP"])
            .await
            .unwrap(),
        0
    );
}

fn restricted_identity(allowed: serde_json::Value) -> serde_json::Value {
    let mut identity = token_identity_json();
    identity["allowed_project_ids"] = allowed;
    identity
}

/// 制限付きの PAT（tenant を `/me` から引いた経路）では、キーを引く前に UUID を使うよう案内する。
#[tokio::test]
async fn a_restricted_token_is_told_to_use_uuids_before_the_lookup() {
    let harness = repo_harness(Setup {
        tenant: None,
        ..Setup::default()
    })
    .await;
    harness
        .mount_get(
            "/v1/personal_tokens/me".into(),
            restricted_identity(json!([PROJECT_ID])),
        )
        .await;
    harness.mount_task_list().await;

    for args in [
        &["task", "tasks", "list", "--project", "APP"][..],
        &["task", "tasks", "show", "APP-7"],
    ] {
        let err = harness.run(args).await.unwrap_err();
        assert_eq!(err.exit_code, 4, "{args:?}");
        assert!(err.message.contains("project UUID"), "{}", err.message);
    }
    assert_eq!(harness.calls_to(&projects_path()).await, 0);

    // UUID なら一覧を経ずに通る
    assert_eq!(
        harness
            .run(&["task", "tasks", "list", "--project", PROJECT_ID])
            .await
            .unwrap(),
        0
    );
    assert_eq!(harness.calls_to(&projects_path()).await, 0);
}

/// tenant を明示した経路は `/me` を読まないので、一覧の 403 がそのまま返る。
#[tokio::test]
async fn a_restricted_token_with_an_explicit_tenant_gets_the_plain_forbidden() {
    let harness = repo_harness(Setup::default()).await;
    Mock::given(method("GET"))
        .and(path(projects_path()))
        .respond_with(ResponseTemplate::new(403))
        .mount(&harness.server)
        .await;

    let err = harness
        .run(&["task", "tasks", "list", "--project", "APP"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 4);
    assert!(
        err.message.starts_with("Permission denied"),
        "{}",
        err.message
    );
    assert!(!err.message.contains("project UUID"), "{}", err.message);
    assert_eq!(harness.calls_to("/v1/personal_tokens/me").await, 0);
}

/// 制限の無いトークン（`null`）と、欄を返さない旧サーバーは、今までどおり一覧を引く。
#[tokio::test]
async fn an_unrestricted_or_old_identity_still_looks_up_the_key() {
    let mut old = token_identity_json();
    old.as_object_mut().unwrap().remove("allowed_project_ids");
    for identity in [restricted_identity(json!(null)), old] {
        let harness = repo_harness(Setup {
            tenant: None,
            ..Setup::default()
        })
        .await;
        harness
            .mount_get("/v1/personal_tokens/me".into(), identity)
            .await;
        harness.mount_task_list().await;

        assert_eq!(
            harness
                .run(&["task", "tasks", "list", "--project", "APP"])
                .await
                .unwrap(),
            0
        );
        assert_eq!(harness.calls_to(&projects_path()).await, 1);
    }
}
