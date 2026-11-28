//! config.yaml の名前付きプロファイル（`--profile` / `TASK_PROFILE` / `.koyori-task.yaml` の `profile`）。
//!
//! トップレベルはサーバー A、`profiles.work` はサーバー B を指す。どのトークンがどこへ
//! 送られたかを、両方のサーバーが受けた要求で確かめる。

mod common;

use common::*;
use serde_json::json;
use task_cli::cli::Cli;
use task_cli::config::{ConfigStore, Profile, TaskConfig};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OTHER_TENANT: &str = "88888888-8888-4888-8888-888888888888";
const ME: &str = "/v1/personal_tokens/me";

struct ProfileHarness {
    home: tempfile::TempDir,
    _repo: tempfile::TempDir,
    a: MockServer,
    b: MockServer,
    context: task_cli::Context,
}

struct Setup {
    env: Vec<(&'static str, String)>,
    /// `.koyori-task.yaml` の中身（無ければ置かない）
    file: Option<&'static str>,
    /// `profiles.work.tenant_id`。`None` は `/me` から引く経路
    work_tenant: Option<&'static str>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            env: vec![],
            file: None,
            work_tenant: Some(TENANT),
        }
    }
}

fn with_env(env: &[(&'static str, &str)]) -> Setup {
    Setup {
        env: env.iter().map(|(k, v)| (*k, v.to_string())).collect(),
        ..Setup::default()
    }
}

fn with_file(file: &'static str) -> Setup {
    Setup {
        file: Some(file),
        ..Setup::default()
    }
}

async fn profile_harness(setup: Setup) -> ProfileHarness {
    let a = MockServer::start().await;
    let b = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    if let Some(file) = setup.file {
        std::fs::write(repo.path().join(".koyori-task.yaml"), file).unwrap();
    }
    let store = ConfigStore::from_home(home.path());
    store
        .save(&TaskConfig {
            api_url: Some(a.uri()),
            token: Some("top-token".into()),
            // プロファイルへ継がれたら B へ別テナントのパスで送られる
            tenant_id: Some(OTHER_TENANT.into()),
            profiles: [
                (
                    "work".to_string(),
                    Profile {
                        api_url: Some(b.uri()),
                        token: Some("work-token".into()),
                        tenant_id: setup.work_tenant.map(Into::into),
                    },
                ),
                (
                    "half".to_string(),
                    Profile {
                        api_url: Some(b.uri()),
                        token: None,
                        tenant_id: None,
                    },
                ),
            ]
            .into(),
        })
        .unwrap();
    let env = setup.env;
    let context = task_cli::Context::new(store, move |key| {
        env.iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.clone())
    })
    .with_repo_dir(repo.path());
    ProfileHarness {
        home,
        _repo: repo,
        a,
        b,
        context,
    }
}

async fn received(server: &MockServer) -> Vec<wiremock::Request> {
    server.received_requests().await.unwrap()
}

impl ProfileHarness {
    async fn run(&self, args: &[&str]) -> task_cli::error::Result<i32> {
        let cli = Cli::parse_args(args.iter().map(|arg| arg.to_string()))
            .expect("the test invoked the CLI with invalid arguments");
        task_cli::run(cli, &self.context).await
    }

    fn config(&self) -> TaskConfig {
        self.context.store().load().unwrap()
    }

    fn raw_config(&self) -> String {
        std::fs::read_to_string(self.context.store().path()).unwrap()
    }

    async fn sent_nothing(&self) -> bool {
        received(&self.a).await.is_empty() && received(&self.b).await.is_empty()
    }

    /// A と B の両方に、どちらが選ばれても答えられる応答を置く。
    async fn mount_everywhere(&self) {
        for server in [&self.a, &self.b] {
            mount_reads(server).await;
        }
    }
}

async fn mount_get(server: &MockServer, path_: String, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(path_))
        .respond_with(response)
        .mount(server)
        .await;
}

fn ok(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

async fn mount_reads(server: &MockServer) {
    mount_get(server, projects_path(), ok(json!([project_json()]))).await;
    mount_get(
        server,
        format!("/v1/tenants/{TENANT}/projects/{PROJECT_ID}"),
        ok(project_json()),
    )
    .await;
    mount_get(server, project_path("statuses"), ok(json!([]))).await;
    mount_get(
        server,
        project_path("tasks"),
        ok(json!({ "tasks": [], "total": 0 })),
    )
    .await;
    mount_get(server, project_path("tasks/APP-7"), ok(task_detail_json())).await;
    mount_get(server, ME.into(), ok(token_identity_json())).await;
}

fn projects_path() -> String {
    format!("/v1/tenants/{TENANT}/projects")
}

/// 受けた要求がすべてそのトークンで送られていること。
async fn all_sent_with(server: &MockServer, token: &str) -> bool {
    let requests = received(server).await;
    !requests.is_empty()
        && requests.iter().all(|request| {
            request
                .headers
                .get("authorization")
                .is_some_and(|value| value == format!("Bearer {token}").as_str())
        })
}

/// `--profile` で B へ、B のトークンで送る。A には 1 本も送らない。
/// 修正前は `--profile` が未知の引数で解析に落ちる。
#[tokio::test]
async fn the_profile_flag_sends_to_the_profile_with_its_token() {
    let harness = profile_harness(Setup::default()).await;
    harness.mount_everywhere().await;

    for args in [
        &["task", "--profile", "work", "projects", "list"][..],
        &["task", "projects", "list", "--profile=work"],
    ] {
        assert_eq!(harness.run(args).await.unwrap(), 0, "{args:?}");
    }

    assert!(received(&harness.a).await.is_empty());
    assert_eq!(received(&harness.b).await.len(), 2);
    assert!(all_sent_with(&harness.b, "work-token").await);
}

#[tokio::test]
async fn without_a_choice_the_top_level_is_used() {
    let harness = profile_harness(Setup::default()).await;
    // トップレベルの tenant_id は OTHER_TENANT
    mount_get(
        &harness.a,
        format!("/v1/tenants/{OTHER_TENANT}/projects"),
        ok(json!([])),
    )
    .await;

    assert_eq!(harness.run(&["task", "projects", "list"]).await.unwrap(), 0);
    assert!(received(&harness.b).await.is_empty());
    assert!(all_sent_with(&harness.a, "top-token").await);
}

/// `TASK_PROFILE` で選べる。`--profile` があれば `TASK_PROFILE` の名前は見ない。
#[tokio::test]
async fn the_flag_wins_over_task_profile() {
    let harness = profile_harness(with_env(&[("TASK_PROFILE", "work")])).await;
    harness.mount_everywhere().await;
    assert_eq!(harness.run(&["task", "projects", "list"]).await.unwrap(), 0);
    assert!(all_sent_with(&harness.b, "work-token").await);
    assert!(received(&harness.a).await.is_empty());

    // `--profile other` は無い名前。`work` に落ちずに止まる
    let harness = profile_harness(with_env(&[("TASK_PROFILE", "work")])).await;
    let err = harness
        .run(&["task", "--profile", "other", "projects", "list"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 2);
    assert!(
        err.message
            .starts_with("Profile \"other\" (from --profile) is not defined in "),
        "{}",
        err.message
    );
    assert!(harness.sent_nothing().await);

    // `--profile default` はトップレベルの明示。`TASK_PROFILE` を見ない
    let harness = profile_harness(with_env(&[("TASK_PROFILE", "work")])).await;
    mount_get(
        &harness.a,
        format!("/v1/tenants/{OTHER_TENANT}/projects"),
        ok(json!([])),
    )
    .await;
    assert_eq!(
        harness
            .run(&["task", "--profile", "default", "projects", "list"])
            .await
            .unwrap(),
        0
    );
    assert!(received(&harness.b).await.is_empty());
    assert!(all_sent_with(&harness.a, "top-token").await);
}

/// 無い名前・不完全なプロファイルは、トップレベルへ落とさずに送信前で止まる。
#[tokio::test]
async fn an_undefined_or_incomplete_profile_stops_before_sending() {
    for (setup, args, needle) in [
        (
            Setup::default(),
            &["task", "--profile", "missing", "projects", "list"][..],
            "Profile \"missing\" (from --profile) is not defined in ",
        ),
        (
            with_env(&[("TASK_PROFILE", "missing")]),
            &["task", "projects", "list"],
            "Profile \"missing\" (from TASK_PROFILE) is not defined in ",
        ),
        (
            Setup::default(),
            &["task", "--profile", "half", "auth", "whoami"],
            "Profile \"half\" (from --profile) is missing token in ",
        ),
        (
            Setup::default(),
            &["task", "--profile", "a/b", "projects", "list"],
            "Invalid profile name",
        ),
    ] {
        let harness = profile_harness(setup).await;
        harness.mount_everywhere().await;
        let err = harness.run(args).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{args:?}");
        assert!(err.message.contains(needle), "{args:?}: {}", err.message);
        assert!(harness.sent_nothing().await, "{args:?}");
    }
}

/// 両 env があれば config を読まないので、プロファイルの名前も確かめない。
#[tokio::test]
async fn both_environment_values_ignore_the_profile() {
    let c = MockServer::start().await;
    mount_reads(&c).await;
    let harness = profile_harness(with_env(&[
        ("TASK_API_URL", &c.uri()),
        ("TASK_TOKEN", "env-token"),
        ("TASK_TENANT", TENANT),
    ]))
    .await;

    assert_eq!(
        harness
            .run(&["task", "--profile", "missing", "projects", "list"])
            .await
            .unwrap(),
        0
    );
    assert!(all_sent_with(&c, "env-token").await);
    assert!(harness.sent_nothing().await);
}

/// `.koyori-task.yaml` の `profile` で B へ。`--project` 明示・`KEY-N`・`auth whoami` でも効く。
#[tokio::test]
async fn the_profile_in_the_repository_file_selects_the_connection() {
    let harness = profile_harness(with_file("profile: work\n")).await;
    harness.mount_everywhere().await;

    for args in [
        &["task", "projects", "list"][..],
        &["task", "tasks", "list", "--project", "APP"],
        &["task", "tasks", "show", "APP-7"],
        &["task", "auth", "whoami"],
    ] {
        assert_eq!(harness.run(args).await.unwrap(), 0, "{args:?}");
    }
    assert!(received(&harness.a).await.is_empty());
    assert!(all_sent_with(&harness.b, "work-token").await);
}

/// `--profile` と `TASK_PROFILE` はファイルの `profile` に勝つ（ファイルの名前は確かめない）。
#[tokio::test]
async fn the_flag_and_task_profile_win_over_the_repository_file() {
    let harness = profile_harness(Setup {
        env: vec![("TASK_PROFILE", "work".into())],
        file: Some("profile: missing\n"),
        ..Setup::default()
    })
    .await;
    harness.mount_everywhere().await;
    assert_eq!(harness.run(&["task", "projects", "list"]).await.unwrap(), 0);
    assert!(all_sent_with(&harness.b, "work-token").await);

    let harness = profile_harness(with_file("profile: missing\n")).await;
    harness.mount_everywhere().await;
    assert_eq!(
        harness
            .run(&["task", "--profile", "work", "projects", "list"])
            .await
            .unwrap(),
        0
    );
    assert!(all_sent_with(&harness.b, "work-token").await);
    assert!(received(&harness.a).await.is_empty());
}

/// プロファイルに tenant_id があれば `/me` を叩かない。無ければ B の `/me` を 1 回。
/// どちらでもトップレベルの tenant_id（OTHER_TENANT）は使わない。
#[tokio::test]
async fn the_tenant_comes_only_from_the_profile_or_its_token() {
    for (work_tenant, me_calls) in [(Some(TENANT), 0), (None, 1)] {
        let harness = profile_harness(Setup {
            work_tenant,
            ..Setup::default()
        })
        .await;
        harness.mount_everywhere().await;

        assert_eq!(
            harness
                .run(&["task", "--profile", "work", "projects", "list"])
                .await
                .unwrap(),
            0,
            "{work_tenant:?}"
        );
        let requests = received(&harness.b).await;
        let paths: Vec<&str> = requests.iter().map(|r| r.url.path()).collect();
        assert_eq!(
            paths.iter().filter(|p| **p == ME).count(),
            me_calls,
            "{paths:?}"
        );
        assert!(paths.contains(&projects_path().as_str()), "{paths:?}");
        assert!(!paths.iter().any(|p| p.contains(OTHER_TENANT)), "{paths:?}");
    }
}

/// プロファイルの tenant_id で 403 を受けたら、そのプロファイルの tenant_id を外す案内を出す。
#[tokio::test]
async fn a_forbidden_response_names_the_profile_to_fix() {
    let harness = profile_harness(Setup::default()).await;
    mount_get(&harness.b, projects_path(), ResponseTemplate::new(403)).await;

    let err = harness
        .run(&["task", "--profile", "work", "projects", "list"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 4);
    assert!(
        err.message
            .ends_with("run `task config unset --profile work tenant_id`"),
        "{}",
        err.message
    );
}

/// 複数テナントのトークンで止まるとき、そのプロファイルへの設定を案内する。
#[tokio::test]
async fn the_multi_tenant_stop_names_the_profile() {
    let harness = profile_harness(Setup {
        work_tenant: None,
        ..Setup::default()
    })
    .await;
    let mut identity = token_identity_json();
    identity["tenant_ids"] = json!([TENANT, OTHER_TENANT]);
    mount_get(&harness.b, ME.into(), ok(identity)).await;

    let err = harness
        .run(&["task", "--profile", "work", "projects", "list"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 2);
    assert!(
        err.message
            .contains("run `task config set --profile work tenant_id <name or tenant_id>`"),
        "{}",
        err.message
    );
    assert!(
        received(&harness.b)
            .await
            .iter()
            .all(|r| r.url.path() != projects_path())
    );
}

/// 書く操作は `TASK_PROFILE` も `.koyori-task.yaml` も読まず、トップレベルを対象にする。
#[tokio::test]
async fn writes_without_the_flag_target_the_top_level_even_when_a_profile_is_chosen_elsewhere() {
    let harness = profile_harness(Setup {
        env: vec![("TASK_PROFILE", "work".into())],
        file: Some("profile: work\n"),
        ..Setup::default()
    })
    .await;

    assert_eq!(harness.run(&["task", "auth", "logout"]).await.unwrap(), 0);
    let config = harness.config();
    assert_eq!(config.token, None);
    assert_eq!(config.profiles["work"].token.as_deref(), Some("work-token"));

    assert_eq!(
        harness
            .run(&["task", "config", "set", "tenant_id", TENANT])
            .await
            .unwrap(),
        0
    );
    let config = harness.config();
    assert_eq!(config.tenant_id.as_deref(), Some(TENANT));
    assert_eq!(config.profiles["work"].tenant_id.as_deref(), Some(TENANT));
    assert_eq!(config.profiles["half"].tenant_id, None);
    assert!(harness.sent_nothing().await);
}

#[tokio::test]
async fn writes_with_the_flag_target_only_that_profile() {
    let harness = profile_harness(Setup::default()).await;
    let before = harness.config();

    assert_eq!(
        harness
            .run(&["task", "auth", "logout", "--profile", "work"])
            .await
            .unwrap(),
        0
    );
    let config = harness.config();
    assert_eq!(config.token, before.token);
    assert_eq!(config.profiles["half"], before.profiles["half"]);
    assert_eq!(config.profiles["work"].token, None);

    assert_eq!(
        harness
            .run(&[
                "task",
                "auth",
                "token",
                "--profile",
                "work",
                "new-work-token"
            ])
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        harness.config().profiles["work"].token.as_deref(),
        Some("new-work-token")
    );
    assert_eq!(harness.config().token, before.token);

    // 空になってもプロファイルは残す
    for key in ["api_url", "token", "tenant_id"] {
        assert_eq!(
            harness
                .run(&["task", "config", "unset", "--profile", "work", key])
                .await
                .unwrap(),
            0
        );
    }
    assert_eq!(harness.config().profiles["work"], Profile::default());
    assert!(
        harness.raw_config().contains("work: {}"),
        "{}",
        harness.raw_config()
    );

    // `config set --profile` だけがプロファイルを作れる（不完全でよい）
    assert_eq!(
        harness
            .run(&[
                "task",
                "config",
                "set",
                "--profile",
                "new",
                "api_url",
                "http://x.invalid",
                "--no-verify",
            ])
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        harness.config().profiles["new"],
        Profile {
            api_url: Some("http://x.invalid".into()),
            ..Profile::default()
        }
    );

    // `--profile default` はトップレベル
    assert_eq!(
        harness
            .run(&[
                "task",
                "config",
                "set",
                "--profile",
                "default",
                "tenant_id",
                TENANT
            ])
            .await
            .unwrap(),
        0
    );
    let config = harness.config();
    assert_eq!(config.tenant_id.as_deref(), Some(TENANT));
    assert!(!config.profiles.contains_key("default"));
    assert!(harness.sent_nothing().await);
}

/// 無い名前は作らない。終了コード 2 で、ファイルの中身を変えない。
#[tokio::test]
async fn writes_to_an_undefined_profile_fail_without_touching_the_file() {
    let harness = profile_harness(Setup::default()).await;
    let before = harness.raw_config();

    for args in [
        &["task", "config", "get", "--profile", "missing", "token"][..],
        &["task", "config", "unset", "--profile", "missing", "token"],
        &["task", "config", "list", "--profile", "missing"],
        &["task", "auth", "token", "--profile", "missing", "x"],
        &["task", "auth", "logout", "--profile", "missing"],
        &["task", "config", "set", "--profile", "a/b", "token", "x"],
    ] {
        let err = harness.run(args).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{args:?}");
        assert_eq!(harness.raw_config(), before, "{args:?}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn writes_to_a_profile_keep_the_file_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let harness = profile_harness(Setup::default()).await;
    for args in [
        &["task", "config", "set", "--profile", "new", "token", "x"][..],
        &["task", "auth", "logout", "--profile", "work"],
    ] {
        assert_eq!(harness.run(args).await.unwrap(), 0, "{args:?}");
        let mode = std::fs::metadata(harness.context.store().path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{args:?}");
    }
}

/// 実バイナリで、標準出力（書く操作の対象）と標準エラー（ファイル由来のプロファイル）を見る。
fn run_binary(
    home: &std::path::Path,
    cwd: &std::path::Path,
    args: &[&str],
) -> (bool, String, String) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_task"))
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("HOME", home)
        .env("TASK_NO_UPDATE_CHECK", "1")
        .output()
        .unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[tokio::test]
async fn the_binary_reports_the_target_and_the_profile_from_the_file() {
    let harness = profile_harness(Setup::default()).await;
    harness.mount_everywhere().await;
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    std::fs::write(repo.path().join(".koyori-task.yaml"), "profile: work\n").unwrap();
    let home = harness.home.path().to_path_buf();
    let cwd = repo.path().to_path_buf();

    let (ok, stdout, stderr) = tokio::task::spawn_blocking({
        let (home, cwd) = (home.clone(), cwd.clone());
        move || run_binary(&home, &cwd, &["--json", "projects", "list"])
    })
    .await
    .unwrap();
    assert!(ok, "{stdout}{stderr}");
    let file = repo.path().join(".koyori-task.yaml");
    // `connect` を複数回呼ぶ口でも 1 行だけ
    assert_eq!(
        stderr.matches("profile: work (from ").count(),
        1,
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("profile: work (from {})", file.display())),
        "{stderr}"
    );
    serde_json::from_str::<serde_json::Value>(&stdout).expect("stdout stays JSON");
    assert!(received(&harness.a).await.is_empty());

    let (ok, stdout, stderr) = tokio::task::spawn_blocking({
        let (home, cwd) = (home.clone(), cwd.clone());
        move || run_binary(&home, &cwd, &["auth", "logout"])
    })
    .await
    .unwrap();
    assert!(ok, "{stderr}");
    assert!(
        stdout.trim_end().ends_with("(profile: default)"),
        "{stdout}"
    );
    assert!(!stderr.contains("profile:"), "{stderr}");

    let (ok, stdout, _) = tokio::task::spawn_blocking(move || {
        run_binary(
            &home,
            &cwd,
            &["config", "set", "--profile", "work", "tenant_id", TENANT],
        )
    })
    .await
    .unwrap();
    assert!(ok);
    assert!(stdout.trim_end().ends_with("(profile: work)"), "{stdout}");
}

/// `config list` は、プロファイルがあれば名前の一覧を足し、`--profile` ならその値を出す。
#[tokio::test]
async fn config_list_shows_profile_names_or_one_profile() {
    let harness = profile_harness(Setup::default()).await;
    let home = harness.home.path().to_path_buf();
    let cwd = home.clone();
    let (ok, stdout, stderr) = tokio::task::spawn_blocking({
        let (home, cwd) = (home.clone(), cwd.clone());
        move || run_binary(&home, &cwd, &["--json", "config", "list"])
    })
    .await
    .unwrap();
    assert!(ok, "{stderr}");
    let listing: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(listing["profiles"], json!(["half", "work"]));
    assert_eq!(listing["token"], "top-token");

    let (ok, stdout, stderr) = tokio::task::spawn_blocking(move || {
        run_binary(
            &home,
            &cwd,
            &["--json", "config", "list", "--profile", "work"],
        )
    })
    .await
    .unwrap();
    assert!(ok, "{stderr}");
    let listing: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(listing["token"], "work-token");
    assert!(listing.get("profiles").is_none(), "{listing}");
}
