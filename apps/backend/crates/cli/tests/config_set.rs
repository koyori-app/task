//! `task config set api_url` の疎通確認。
//!
//! 保存の前に、認証なしの要求で API が答えるかを確かめる。確かめられなければ何も保存しない。
//! 外へ出るのはこの確認だけで、トークンは送らない。

mod common;

use std::path::PathBuf;

use common::*;
use serde_json::json;
use task_cli::config::{ConfigStore, TaskConfig};
use task_cli::error::Result;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ME: &str = "/v1/personal_tokens/me";

struct Local {
    home: tempfile::TempDir,
    server: MockServer,
    env: Vec<(String, String)>,
    repo_dir: Option<PathBuf>,
}

impl Local {
    async fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            server: MockServer::start().await,
            env: Vec::new(),
            repo_dir: None,
        }
    }

    fn env(mut self, pairs: &[(&str, &str)]) -> Self {
        self.env = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        self
    }

    fn store(&self) -> ConfigStore {
        ConfigStore::from_home(self.home.path())
    }

    async fn run(&self, args: &[&str]) -> Result<i32> {
        let env = self.env.clone();
        let mut context = task_cli::Context::new(self.store(), move |key| {
            env.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        });
        if let Some(dir) = &self.repo_dir {
            context = context.with_repo_dir(dir);
        }
        task_cli::run(parse(args), &context).await
    }

    async fn requests(&self) -> Vec<wiremock::Request> {
        self.server.received_requests().await.unwrap()
    }

    fn config_file_exists(&self) -> bool {
        self.store().path().exists()
    }
}

/// 認証なしの `/me` に、task の API と同じ 401 を返す。
async fn answer_as_api(server: &MockServer, prefix: &str) {
    Mock::given(method("GET"))
        .and(path(format!("{prefix}{ME}")))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"message": "unauthorized"})))
        .mount(server)
        .await;
}

/// 修正前は渡した値がそのまま入り、`/api` の無い URL が実行時に分かりにくく失敗していた。
#[tokio::test]
async fn saves_the_api_path_found_below_the_given_url() {
    let local = Local::new().await.env(&[("TASK_TOKEN", "env-token")]);
    answer_as_api(&local.server, "/api").await;
    local
        .store()
        .save(&TaskConfig {
            token: Some("config-token".into()),
            ..Default::default()
        })
        .unwrap();
    let uri = local.server.uri();

    assert_eq!(
        local
            .run(&["task", "config", "set", "api_url", &uri])
            .await
            .unwrap(),
        0
    );

    let config = local.store().load().unwrap();
    assert_eq!(config.api_url, Some(format!("{uri}/api")));
    assert_eq!(config.token.as_deref(), Some("config-token"));
    let requests = local.requests().await;
    let paths: Vec<&str> = requests.iter().map(|request| request.url.path()).collect();
    assert_eq!(paths, [ME, "/api/v1/personal_tokens/me"]);
    // config のトークンも `TASK_TOKEN` も送らない
    for request in &requests {
        assert!(
            !request.headers.contains_key("authorization"),
            "{}",
            request.url
        );
    }
}

#[tokio::test]
async fn keeps_a_url_that_already_answers_and_sends_one_request() {
    // `/api` まで書いた値
    let local = Local::new().await;
    answer_as_api(&local.server, "/api").await;
    let api = format!("{}/api", local.server.uri());
    for input in [api.clone(), format!("{api}/")] {
        assert_eq!(
            local
                .run(&["task", "config", "set", "api_url", &input])
                .await
                .unwrap(),
            0
        );
        assert_eq!(local.store().load().unwrap().api_url, Some(api.clone()));
    }
    assert_eq!(local.requests().await.len(), 2, "1 回の実行につき 1 本");

    // API が根に居る構成では `/api` を足さない
    let local = Local::new().await;
    answer_as_api(&local.server, "").await;
    let uri = local.server.uri();
    assert_eq!(
        local
            .run(&["task", "config", "set", "api_url", &format!("{uri}/")])
            .await
            .unwrap(),
        0
    );
    assert_eq!(local.store().load().unwrap().api_url, Some(uri));
    assert_eq!(local.requests().await.len(), 1);
}

/// 確かめられなければ終了コード 2 で、config を作りも変えもしない。
#[tokio::test]
async fn saves_nothing_when_no_candidate_answers_as_the_api() {
    let elsewhere = MockServer::start().await;
    answer_as_api(&elsewhere, "").await;
    answer_as_api(&elsewhere, "/api").await;

    let not_found = Local::new().await;

    let ok = Local::new().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"message": "ok"})))
        .mount(&ok.server)
        .await;

    let html = Local::new().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_string("<html>Sign in</html>"))
        .mount(&html.server)
        .await;

    let no_message = Local::new().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "unauthorized"})))
        .mount(&no_message.server)
        .await;

    let redirect = Local::new().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(301)
                .insert_header("Location", format!("{}{ME}", elsewhere.uri())),
        )
        .mount(&redirect.server)
        .await;

    for (name, local, outcome) in [
        ("404", &not_found, "404"),
        ("200", &ok, "200"),
        ("401 html", &html, "401 without a task API error body"),
        (
            "401 no message",
            &no_message,
            "401 without a task API error body",
        ),
        ("301", &redirect, "301 -> "),
    ] {
        let uri = local.server.uri();
        let err = local
            .run(&["task", "config", "set", "api_url", &uri])
            .await
            .unwrap_err();
        assert_eq!(err.exit_code, 2, "{name}: {}", err.message);
        for expected in [
            format!("Could not confirm a task API at {uri}"),
            format!("{uri}{ME}: {outcome}"),
            format!("{uri}/api{ME}: {outcome}"),
            "--no-verify".to_string(),
        ] {
            assert!(err.message.contains(&expected), "{name}: {}", err.message);
        }
        assert_eq!(local.requests().await.len(), 2, "{name}");
        assert!(!local.config_file_exists(), "{name}: config を作らない");
    }
    // リダイレクトは辿らない
    assert!(elsewhere.received_requests().await.unwrap().is_empty());

    // 閉じたポート。既にある config は実行前のまま
    let closed = Local::new().await;
    let before = TaskConfig {
        api_url: Some("https://old.invalid/api".into()),
        token: Some("config-token".into()),
        ..Default::default()
    };
    closed.store().save(&before).unwrap();
    let err = closed
        .run(&["task", "config", "set", "api_url", "http://127.0.0.1:9"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 2, "{}", err.message);
    assert!(
        err.message
            .contains("http://127.0.0.1:9/api/v1/personal_tokens/me: "),
        "{}",
        err.message
    );
    assert!(err.message.contains("--no-verify"), "{}", err.message);
    assert_eq!(closed.store().load().unwrap(), before);
}

#[tokio::test]
async fn rejects_a_value_that_is_not_an_http_url_without_sending() {
    let local = Local::new().await;
    for value in ["task.example.com", "ftp://task.example.com", ""] {
        let err = local
            .run(&["task", "config", "set", "api_url", value])
            .await
            .unwrap_err();
        assert_eq!(err.exit_code, 2, "{value:?}");
        assert!(
            err.message.contains("expected an http(s) URL"),
            "{}",
            err.message
        );
    }
    assert!(local.requests().await.is_empty());
    assert!(!local.config_file_exists());
}

/// `--no-verify` は今までと同じく、値をそのまま保存して何も送らない。
#[tokio::test]
async fn no_verify_saves_the_value_as_given_without_sending() {
    let local = Local::new().await;
    answer_as_api(&local.server, "/api").await;
    let uri = format!("{}/", local.server.uri());

    for args in [
        &["task", "config", "set", "api_url", &uri, "--no-verify"][..],
        &["task", "config", "set", "--no-verify", "api_url", &uri],
    ] {
        assert_eq!(local.run(args).await.unwrap(), 0);
        assert_eq!(local.store().load().unwrap().api_url, Some(uri.clone()));
    }
    assert_eq!(
        local
            .run(&[
                "task",
                "config",
                "set",
                "api_url",
                "http://127.0.0.1:9",
                "--no-verify"
            ])
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        local.store().load().unwrap().api_url.as_deref(),
        Some("http://127.0.0.1:9")
    );
    assert!(local.requests().await.is_empty());
}

/// 新しいプロファイルは、確認が通ったときだけできる。
#[tokio::test]
async fn creates_a_profile_only_when_the_url_is_confirmed() {
    let local = Local::new().await;
    answer_as_api(&local.server, "/api").await;
    let uri = local.server.uri();
    let top = TaskConfig {
        api_url: Some("https://top.invalid/api".into()),
        token: Some("top-token".into()),
        ..Default::default()
    };
    local.store().save(&top).unwrap();

    let err = local
        .run(&[
            "task",
            "config",
            "set",
            "--profile",
            "gone",
            "api_url",
            "http://127.0.0.1:9",
        ])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 2);
    assert_eq!(local.store().load().unwrap(), top, "プロファイルを作らない");

    assert_eq!(
        local
            .run(&["task", "config", "set", "--profile", "new", "api_url", &uri])
            .await
            .unwrap(),
        0
    );
    let config = local.store().load().unwrap();
    assert_eq!(config.profiles["new"].api_url, Some(format!("{uri}/api")));
    assert_eq!(config.profiles["new"].token, None);
    assert_eq!(config.api_url, top.api_url, "トップレベルは変えない");
}

/// `api_url` 以外のキーは、`--no-verify` の有無を問わず何も送らない。
#[tokio::test]
async fn other_keys_are_never_checked() {
    let local = Local::new().await;
    let uri = local.server.uri();
    for args in [
        &["task", "config", "set", "tenant_id", TENANT][..],
        &["task", "config", "set", "tenant_id", TENANT, "--no-verify"],
        &["task", "config", "set", "token", &uri],
        &["task", "config", "set", "token", &uri, "--no-verify"],
        &["task", "config", "get", "api_url"],
        &["task", "config", "list"],
        &["task", "config", "unset", "tenant_id"],
        &["task", "auth", "token", "token-2"],
        &["task", "auth", "logout"],
    ] {
        assert_eq!(local.run(args).await.unwrap(), 0, "{args:?}");
    }
    assert!(local.requests().await.is_empty());
}

/// 接続設定（env・`.koyori-task.yaml`）を読まないので、片側 env でも、壊れたファイルがあっても動く。
#[tokio::test]
async fn ignores_the_environment_and_the_repository_file() {
    let repo = git_repo(None);
    std::fs::write(
        repo.path().join(".koyori-task.yaml"),
        "project: [unclosed\n",
    )
    .unwrap();

    for env in [
        &[("TASK_API_URL", "http://127.0.0.1:9")][..],
        &[("TASK_TOKEN", "env-token")],
        &[
            ("TASK_API_URL", "http://127.0.0.1:9"),
            ("TASK_TOKEN", "env-token"),
        ],
        &[("TASK_PROFILE", "gone")],
    ] {
        let mut local = Local::new().await.env(env);
        local.repo_dir = Some(repo.path().to_path_buf());
        answer_as_api(&local.server, "/api").await;
        let uri = local.server.uri();

        assert_eq!(
            local
                .run(&["task", "config", "set", "api_url", &uri])
                .await
                .unwrap(),
            0,
            "{env:?}"
        );
        let config = local.store().load().unwrap();
        assert_eq!(config.api_url, Some(format!("{uri}/api")), "{env:?}");
        assert!(config.profiles.is_empty(), "{env:?}: トップレベルへ書く");
        for request in local.requests().await {
            assert!(!request.headers.contains_key("authorization"), "{env:?}");
        }
    }
}
