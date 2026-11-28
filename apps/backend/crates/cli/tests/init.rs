//! `task init`。接続の設定（足りない分だけ）と `.koyori-task.yaml` の作成。
//!
//! 端末への問い合わせは台本どおりに答える `ScriptedPrompt` に差し替え、git は一時ディレクトリに
//! 実際のリポジトリを作る。

mod common;

use std::path::{Path, PathBuf};
use std::rc::Rc;

use common::*;
use serde_json::{Value, json};
use task_cli::config::{ConfigStore, Profile, TaskConfig};
use task_cli::error::Result;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ME: &str = "/v1/personal_tokens/me";
const FILE: &str = ".koyori-task.yaml";
const OTHER_TENANT: &str = "88888888-8888-4888-8888-888888888888";
const ORIGIN: &str = "git@github.com:acme/app.git";

struct Init {
    home: tempfile::TempDir,
    server: MockServer,
    env: Vec<(String, String)>,
    /// git リポジトリ（`None` は git の外の一時ディレクトリ）
    repo: tempfile::TempDir,
    cwd: PathBuf,
    prompt: Rc<ScriptedPrompt>,
}

impl Init {
    /// 設定を置かない足場。`origin` を渡すと git リポジトリにその remote を足す。
    async fn unconfigured(origin: Option<&str>) -> Self {
        let repo = git_repo(origin);
        Self {
            home: tempfile::tempdir().unwrap(),
            server: MockServer::start().await,
            env: Vec::new(),
            cwd: repo.path().to_path_buf(),
            repo,
            prompt: ScriptedPrompt::piped(None),
        }
    }

    /// 接続が設定済み（`api_url` はモックの根、tenant は明示）の足場。`/me` は答える。
    async fn connected(origin: Option<&str>) -> Self {
        let init = Self::unconfigured(origin).await;
        init.store()
            .save(&TaskConfig {
                api_url: Some(init.server.uri()),
                token: Some("token-1".into()),
                tenant_id: Some(TENANT.into()),
                ..Default::default()
            })
            .unwrap();
        mount_me(&init.server, "", "token-1", token_identity_json()).await;
        init
    }

    fn prompt(mut self, prompt: Rc<ScriptedPrompt>) -> Self {
        self.prompt = prompt;
        self
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
        let context = task_cli::Context::new(self.store(), move |key| {
            env.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        })
        .with_repo_dir(&self.cwd)
        .with_prompt(self.prompt.clone());
        task_cli::run(parse(args), &context).await
    }

    fn file(&self) -> PathBuf {
        self.repo.path().join(FILE)
    }

    /// 作られた `.koyori-task.yaml` の中身。無ければ `None`。
    fn written(&self) -> Option<String> {
        std::fs::read_to_string(self.file()).ok()
    }

    async fn requests(&self) -> Vec<wiremock::Request> {
        self.server.received_requests().await.unwrap()
    }

    async fn calls_to(&self, target: &str) -> usize {
        self.requests()
            .await
            .iter()
            .filter(|request| request.url.path() == target)
            .count()
    }
}

/// そのトークンの `/me` に答える。
async fn mount_me(server: &MockServer, prefix: &str, token: &str, identity: Value) {
    Mock::given(method("GET"))
        .and(path(format!("{prefix}{ME}")))
        .and(header("authorization", format!("Bearer {token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity))
        .with_priority(1)
        .mount(server)
        .await;
}

/// 認証なし（と、知らないトークン）の `/me` に、task の API と同じ 401 を返す。
async fn answer_as_api(server: &MockServer, prefix: &str) {
    Mock::given(method("GET"))
        .and(path(format!("{prefix}{ME}")))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"message": "unauthorized"})))
        .mount(server)
        .await;
}

fn projects_path(prefix: &str, tenant: &str) -> String {
    format!("{prefix}/v1/tenants/{tenant}/projects")
}

async fn mount_projects(server: &MockServer, prefix: &str, tenant: &str, projects: Vec<Value>) {
    Mock::given(method("GET"))
        .and(path(projects_path(prefix, tenant)))
        .respond_with(ResponseTemplate::new(200).set_body_json(projects))
        .mount(server)
        .await;
}

fn project_id(number: u32) -> String {
    format!("{number:08}-1111-4111-8111-111111111111")
}

fn project(number: u32, key: &str, linked: Option<(&str, &str)>) -> Value {
    json!({
        "id": project_id(number),
        "name": format!("{key} project"),
        "description": "",
        "tenant_id": TENANT,
        "icon_emoji": null,
        "icon_url": null,
        "key": key,
        "is_personal": false,
        "personal_owner_id": null,
        "inherit_tenant_members": true,
        "repository": linked.map(|(owner, name)| json!({
            "host": "github",
            "host_url": "https://github.com",
            "repo_owner": owner,
            "repo_name": name,
            "html_url": format!("https://github.com/{owner}/{name}"),
        })),
    })
}

/// origin（`acme/app`）に連携した `APP` と、別のリポジトリに連携した `LIB`、連携の無い `OPS`。
fn one_match() -> Vec<Value> {
    vec![
        project(1, "OPS", None),
        project(2, "LIB", Some(("acme", "lib"))),
        project(3, "APP", Some(("Acme", "App"))),
    ]
}

fn yes_no() -> String {
    "line:Use project APP (APP project) — linked to github.com/Acme/App? [Y/n] ".to_string()
}

/// 修正前は `init` というコマンドが無く、引数の解析で落ちる。
#[tokio::test]
async fn writes_the_given_project_at_the_git_root_from_a_subdirectory() {
    let mut init = Init::connected(Some(ORIGIN)).await;
    mount_projects(&init.server, "", TENANT, one_match()).await;
    init.cwd = init.repo.path().join("src/deep");
    std::fs::create_dir_all(&init.cwd).unwrap();

    assert_eq!(
        init.run(&["task", "init", "--project", "lib"])
            .await
            .unwrap(),
        0
    );

    assert_eq!(init.written().as_deref(), Some("project: LIB\n"));
    assert!(!init.cwd.join(FILE).exists(), "実行した階層には作らない");
    assert!(init.prompt.asked().is_empty(), "何も聞かない");
    // 接続の設定は触らない
    assert_eq!(
        init.store().load().unwrap().token.as_deref(),
        Some("token-1")
    );
}

#[tokio::test]
async fn a_project_that_cannot_be_resolved_writes_nothing() {
    // 無いキー
    let init = Init::connected(Some(ORIGIN)).await;
    mount_projects(&init.server, "", TENANT, one_match()).await;
    let err = init
        .run(&["task", "init", "--project", "NOPE"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 5, "{}", err.message);
    assert_eq!(init.written(), None);

    // 無い UUID（単体取得が 404）と、入れない UUID（403）
    for (status, exit_code) in [(404, 5), (403, 4)] {
        let init = Init::connected(Some(ORIGIN)).await;
        let id = project_id(9);
        Mock::given(method("GET"))
            .and(path(format!("{}/{id}", projects_path("", TENANT))))
            .respond_with(ResponseTemplate::new(status).set_body_json(json!({"message": "x"})))
            .mount(&init.server)
            .await;
        let err = init
            .run(&["task", "init", "--project", &id])
            .await
            .unwrap_err();
        assert_eq!(err.exit_code, exit_code, "{}", err.message);
        assert_eq!(init.written(), None);
    }

    // 一覧が 403（scope 不足）
    let init = Init::connected(Some(ORIGIN))
        .await
        .prompt(ScriptedPrompt::terminal(&["1"], &[]));
    Mock::given(method("GET"))
        .and(path(projects_path("", TENANT)))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({"message": "forbidden"})))
        .mount(&init.server)
        .await;
    let err = init.run(&["task", "init"]).await.unwrap_err();
    assert_eq!(err.exit_code, 4, "{}", err.message);
    assert!(err.message.contains("read:project"), "{}", err.message);
    assert_eq!(init.written(), None);
    assert!(init.prompt.asked().is_empty(), "一覧が取れなければ聞かない");
}

/// origin に連携したプロジェクトが 1 つなら、それを候補として確かめる。
#[tokio::test]
async fn the_one_project_linked_to_the_origin_is_offered() {
    // y・空（Enter）・YES は採る
    for answer in ["y", "", "YES"] {
        let init = Init::connected(Some(ORIGIN))
            .await
            .prompt(ScriptedPrompt::terminal(&[answer], &[]));
        mount_projects(&init.server, "", TENANT, one_match()).await;

        assert_eq!(init.run(&["task", "init"]).await.unwrap(), 0, "{answer:?}");
        assert_eq!(init.written().as_deref(), Some("project: APP\n"));
        assert_eq!(init.prompt.asked(), [yes_no()]);
    }

    // n と、入力なし（EOF）は書かない
    for answers in [&["n"][..], &["maybe"], &[]] {
        let init = Init::connected(Some(ORIGIN))
            .await
            .prompt(ScriptedPrompt::terminal(answers, &[]));
        mount_projects(&init.server, "", TENANT, one_match()).await;

        let err = init.run(&["task", "init"]).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{answers:?}: {}", err.message);
        assert_eq!(init.written(), None, "{answers:?}");
        assert_eq!(init.prompt.asked(), [yes_no()], "聞き直さない");
    }

    // --yes は聞かずに採る（端末でなくても）
    for prompt in [
        ScriptedPrompt::terminal(&[], &[]),
        ScriptedPrompt::piped(None),
    ] {
        let init = Init::connected(Some(ORIGIN)).await.prompt(prompt);
        mount_projects(&init.server, "", TENANT, one_match()).await;

        assert_eq!(init.run(&["task", "init", "--yes"]).await.unwrap(), 0);
        assert_eq!(init.written().as_deref(), Some("project: APP\n"));
        assert!(init.prompt.asked().is_empty());
    }

    // 端末でなく --yes も無ければ、決めずに止まる
    let init = Init::connected(Some(ORIGIN)).await;
    mount_projects(&init.server, "", TENANT, one_match()).await;
    let err = init.run(&["task", "init"]).await.unwrap_err();
    assert_eq!(err.exit_code, 2, "{}", err.message);
    assert!(err.message.contains("--yes"), "{}", err.message);
    assert_eq!(init.written(), None);
}

/// 一致が 0 件・複数なら、一覧（キーの昇順）から選ばせる。
#[tokio::test]
async fn zero_or_several_matches_are_chosen_from_the_list() {
    // 同じリポジトリを 2 つのプロジェクトへ連携している
    let two_matches = || {
        vec![
            project(1, "OPS", None),
            project(2, "WEB", Some(("acme", "app"))),
            project(3, "APP", Some(("acme", "app"))),
        ]
    };
    let no_match = || {
        vec![
            project(1, "OPS", None),
            project(2, "LIB", Some(("acme", "lib"))),
        ]
    };

    // 番号（昇順で APP, OPS, WEB）とキー
    for (projects, answer, expected) in [
        (two_matches(), "3", "WEB"),
        (two_matches(), "app", "APP"),
        (no_match(), "1", "LIB"),
        (no_match(), "ops", "OPS"),
    ] {
        let init = Init::connected(Some(ORIGIN))
            .await
            .prompt(ScriptedPrompt::terminal(&[answer], &[]));
        mount_projects(&init.server, "", TENANT, projects).await;

        assert_eq!(init.run(&["task", "init"]).await.unwrap(), 0, "{answer}");
        assert_eq!(
            init.written(),
            Some(format!("project: {expected}\n")),
            "{answer}"
        );
        assert_eq!(init.prompt.asked(), ["line:Project (number or key): "]);
    }

    // 入力なし（EOF）・範囲外・空・無いキーは、聞き直さずに止まる
    for answers in [&[][..], &["0"], &["4"], &[""], &["NOPE"]] {
        let init = Init::connected(Some(ORIGIN))
            .await
            .prompt(ScriptedPrompt::terminal(answers, &[]));
        mount_projects(&init.server, "", TENANT, two_matches()).await;

        let err = init.run(&["task", "init"]).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{answers:?}: {}", err.message);
        assert_eq!(init.written(), None, "{answers:?}");
        assert_eq!(init.prompt.asked().len(), 1, "{answers:?}");
    }

    // --yes は 1 件のときだけ効く。端末でなければ選べない
    for (args, prompt) in [
        (
            &["task", "init", "--yes"][..],
            ScriptedPrompt::terminal(&["1"], &[]),
        ),
        (&["task", "init"], ScriptedPrompt::piped(None)),
    ] {
        let init = Init::connected(Some(ORIGIN)).await.prompt(prompt);
        mount_projects(&init.server, "", TENANT, two_matches()).await;

        let err = init.run(args).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{args:?}: {}", err.message);
        assert!(err.message.contains("--project"), "{}", err.message);
        assert_eq!(init.written(), None);
        assert!(init.prompt.asked().is_empty(), "{args:?}");
    }
}

#[tokio::test]
async fn a_two_digit_number_picks_from_a_long_list() {
    let init = Init::connected(Some(ORIGIN))
        .await
        .prompt(ScriptedPrompt::terminal(&["11"], &[]));
    // キーの昇順は P01, P02, …, P12
    let projects = (1..=12)
        .rev()
        .map(|number| project(number, &format!("P{number:02}"), None))
        .collect();
    mount_projects(&init.server, "", TENANT, projects).await;

    assert_eq!(init.run(&["task", "init"]).await.unwrap(), 0);
    assert_eq!(init.written().as_deref(), Some("project: P11\n"));
}

/// github.com 以外の origin と、origin の無いリポジトリは「一致なし」で、一覧から選ぶ。
#[tokio::test]
async fn other_hosts_and_a_missing_origin_do_not_match() {
    for origin in [
        Some("git@gitlab.com:acme/app.git"),
        Some("https://github.com.evil.example/acme/app"),
        Some("ssh://git@github.com/acme/app"),
        None,
    ] {
        let init = Init::connected(origin)
            .await
            .prompt(ScriptedPrompt::terminal(&["OPS"], &[]));
        mount_projects(&init.server, "", TENANT, one_match()).await;

        assert_eq!(init.run(&["task", "init"]).await.unwrap(), 0, "{origin:?}");
        assert_eq!(init.written().as_deref(), Some("project: OPS\n"));
        assert_eq!(
            init.prompt.asked(),
            ["line:Project (number or key): "],
            "{origin:?}: 連携先が同名でも候補として確かめない"
        );

        // --yes でも採らない
        let init = Init::connected(origin).await;
        mount_projects(&init.server, "", TENANT, one_match()).await;
        let err = init.run(&["task", "init", "--yes"]).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{origin:?}");
        assert_eq!(init.written(), None);
    }
}

/// 探索経路に既にファイルがあれば、作らず、接続もしない。
#[tokio::test]
async fn an_existing_file_on_the_search_path_is_left_alone() {
    for contents in [
        "project: OLD\n",
        "profile: work\nextra: 1\n",
        "project: [unclosed\n",
        "",
    ] {
        // 根にあるファイルを、子の階層から見つける
        let mut init = Init::connected(Some(ORIGIN)).await;
        std::fs::write(init.file(), contents).unwrap();
        init.cwd = init.repo.path().join("sub");
        std::fs::create_dir_all(&init.cwd).unwrap();

        assert_eq!(
            init.run(&["task", "init", "--project", "APP"])
                .await
                .unwrap(),
            0,
            "{contents:?}"
        );
        assert_eq!(init.written().as_deref(), Some(contents));
        assert!(init.requests().await.is_empty(), "{contents:?}");

        // 子の階層にあるファイル。根には作らない
        let mut init = Init::connected(Some(ORIGIN)).await;
        init.cwd = init.repo.path().join("sub");
        std::fs::create_dir_all(&init.cwd).unwrap();
        std::fs::write(init.cwd.join(FILE), contents).unwrap();

        assert_eq!(init.run(&["task", "init", "--yes"]).await.unwrap(), 0);
        assert_eq!(init.written(), None, "{contents:?}: 根には作らない");
        assert_eq!(
            std::fs::read_to_string(init.cwd.join(FILE)).unwrap(),
            contents
        );
        assert!(init.requests().await.is_empty(), "{contents:?}");
    }

    // 接続の設定が無くても、聞かずに終わる
    let init = Init::unconfigured(None)
        .await
        .prompt(ScriptedPrompt::terminal(&["https://x.invalid"], &["t"]));
    std::fs::write(init.file(), "project: OLD\n").unwrap();
    assert_eq!(init.run(&["task", "init"]).await.unwrap(), 0);
    assert!(init.prompt.asked().is_empty());
    assert!(!init.store().path().exists());
}

/// 切れたリンクも「ある」と扱い、その先に書かない。
#[cfg(unix)]
#[tokio::test]
async fn a_dangling_symlink_counts_as_an_existing_file() {
    let init = Init::connected(Some(ORIGIN)).await;
    let target = init.repo.path().join("elsewhere.yaml");
    std::os::unix::fs::symlink(&target, init.file()).unwrap();

    assert_eq!(
        init.run(&["task", "init", "--project", "APP"])
            .await
            .unwrap(),
        0
    );
    assert!(!target.exists());
    assert!(init.requests().await.is_empty());
}

/// git の外では接続だけを確かめて、ファイルを作らない。
#[tokio::test]
async fn outside_a_git_repository_only_the_connection_is_checked() {
    let mut init = Init::connected(None).await;
    let outside = tempfile::tempdir().unwrap();
    init.cwd = outside.path().to_path_buf();

    assert_eq!(
        init.run(&["task", "init", "--project", "APP"])
            .await
            .unwrap(),
        0
    );
    assert!(!outside.path().join(FILE).exists());
    assert_eq!(init.calls_to(ME).await, 1);
    assert_eq!(init.calls_to(&projects_path("", TENANT)).await, 0);
}

/// 制限付きのトークンは一覧を読めないので、UUID を聞いて単体取得で確かめる。
#[tokio::test]
async fn a_project_limited_token_asks_for_the_uuid() {
    let allowed = project_id(3);
    let restricted = || {
        let mut identity = token_identity_json();
        identity["allowed_project_ids"] = json!([project_id(3)]);
        identity
    };
    let limited = |prompt: Rc<ScriptedPrompt>| async {
        let init = Init::unconfigured(Some(ORIGIN)).await.prompt(prompt);
        init.store()
            .save(&TaskConfig {
                api_url: Some(init.server.uri()),
                token: Some("token-1".into()),
                tenant_id: Some(TENANT.into()),
                ..Default::default()
            })
            .unwrap();
        mount_me(&init.server, "", "token-1", restricted()).await;
        Mock::given(method("GET"))
            .and(path(format!(
                "{}/{}",
                projects_path("", TENANT),
                project_id(3)
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(project(
                3,
                "APP",
                Some(("acme", "app")),
            )))
            .mount(&init.server)
            .await;
        init
    };
    let single = format!("{}/{allowed}", projects_path("", TENANT));

    // 端末: UUID を聞く。書くのはキーではなく UUID
    let init = limited(ScriptedPrompt::terminal(&[&allowed], &[])).await;
    assert_eq!(init.run(&["task", "init"]).await.unwrap(), 0);
    assert_eq!(init.written(), Some(format!("project: {allowed}\n")));
    assert_eq!(init.prompt.asked(), ["line:Project UUID: "]);
    assert_eq!(
        init.calls_to(&projects_path("", TENANT)).await,
        0,
        "一覧を叩かない"
    );
    assert_eq!(init.calls_to(&single).await, 1);

    // --project <UUID>
    let init = limited(ScriptedPrompt::piped(None)).await;
    assert_eq!(
        init.run(&["task", "init", "--project", &allowed])
            .await
            .unwrap(),
        0
    );
    assert_eq!(init.written(), Some(format!("project: {allowed}\n")));

    // キーは引けない（一覧を叩く前に止める）
    let init = limited(ScriptedPrompt::piped(None)).await;
    let err = init
        .run(&["task", "init", "--project", "APP"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 4, "{}", err.message);
    assert!(err.message.contains("project UUID"), "{}", err.message);
    assert_eq!(init.calls_to(&projects_path("", TENANT)).await, 0);
    assert_eq!(init.written(), None);

    // 端末でない・UUID でない答え・入力なしは書かない
    for prompt in [
        ScriptedPrompt::piped(None),
        ScriptedPrompt::terminal(&["APP"], &[]),
        ScriptedPrompt::terminal(&[], &[]),
    ] {
        let init = limited(prompt).await;
        let err = init.run(&["task", "init"]).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{}", err.message);
        assert_eq!(init.written(), None);
        assert_eq!(init.calls_to(&single).await, 0);
    }
}

/// 設定が無いとき: URL を確かめ、確かめた URL にだけトークンを送り、`/me` が通ってから保存する。
#[tokio::test]
async fn sets_up_the_connection_before_writing_the_file() {
    let init = Init::unconfigured(Some(ORIGIN)).await;
    let uri = init.server.uri();
    let init = init.prompt(ScriptedPrompt::terminal(&[&uri], &["good-token\n"]));
    answer_as_api(&init.server, "/api").await;
    mount_me(&init.server, "/api", "good-token", token_identity_json()).await;
    mount_projects(&init.server, "/api", TENANT, one_match()).await;

    assert_eq!(init.run(&["task", "init", "--yes"]).await.unwrap(), 0);

    assert_eq!(
        init.store().load().unwrap(),
        TaskConfig {
            api_url: Some(format!("{uri}/api")),
            token: Some("good-token".into()),
            ..Default::default()
        }
    );
    assert_eq!(init.written().as_deref(), Some("project: APP\n"));
    assert_eq!(
        init.prompt.asked(),
        [
            "line:Instance URL (e.g. https://task.example.com): ",
            "secret:Token: "
        ]
    );
    // 疎通の 2 本（根は外れ、`/api` で当たり）は認証なし。トークンは確かめた `/api` にだけ送る
    let requests = init.requests().await;
    let sent: Vec<(&str, Option<&str>)> = requests
        .iter()
        .map(|request| {
            (
                request.url.path(),
                request
                    .headers
                    .get("authorization")
                    .map(|value| value.to_str().unwrap()),
            )
        })
        .collect();
    assert_eq!(sent[0], (ME, None));
    assert_eq!(sent[1], ("/api/v1/personal_tokens/me", None));
    for (target, authorization) in &sent[2..] {
        assert!(target.starts_with("/api/"), "{target}");
        assert_eq!(*authorization, Some("Bearer good-token"), "{target}");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(init.store().path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[tokio::test]
async fn a_failed_setup_saves_nothing() {
    // トークンが通らない（401）
    let init = Init::unconfigured(Some(ORIGIN)).await;
    let uri = init.server.uri();
    let init = init.prompt(ScriptedPrompt::terminal(&[&uri], &["bad-token"]));
    answer_as_api(&init.server, "/api").await;
    let err = init.run(&["task", "init", "--yes"]).await.unwrap_err();
    assert_eq!(err.exit_code, 3, "{}", err.message);
    assert!(!init.store().path().exists(), "config を作らない");
    assert_eq!(init.written(), None);

    // `/me` の応答が読めない
    let init = Init::unconfigured(Some(ORIGIN)).await;
    let uri = init.server.uri();
    let init = init.prompt(ScriptedPrompt::terminal(&[&uri], &["good-token"]));
    answer_as_api(&init.server, "/api").await;
    mount_me(
        &init.server,
        "/api",
        "good-token",
        json!({"unexpected": true}),
    )
    .await;
    let err = init.run(&["task", "init", "--yes"]).await.unwrap_err();
    assert_eq!(err.exit_code, 1, "{}", err.message);
    assert!(!init.store().path().exists());
    assert_eq!(init.written(), None);

    // 疎通がどの候補でも不成立。トークンを聞かない
    let init = Init::unconfigured(Some(ORIGIN)).await;
    let uri = init.server.uri();
    let init = init.prompt(ScriptedPrompt::terminal(&[&uri], &["good-token"]));
    let err = init.run(&["task", "init"]).await.unwrap_err();
    assert_eq!(err.exit_code, 2, "{}", err.message);
    assert!(err.message.contains("Could not confirm"), "{}", err.message);
    assert!(err.message.contains("task init"), "{}", err.message);
    assert_eq!(init.prompt.asked().len(), 1, "URL だけを聞いた");
    assert!(!init.store().path().exists());

    // URL・トークンの入力なし（EOF）
    for (lines, secrets, asked) in [(&[][..], &[][..], 1), (&[uri.as_str()], &[], 2)] {
        let init = Init::unconfigured(Some(ORIGIN)).await;
        let uri = init.server.uri();
        let lines: Vec<&str> = lines.iter().map(|_| uri.as_str()).collect();
        let init = init.prompt(ScriptedPrompt::terminal(&lines, secrets));
        answer_as_api(&init.server, "/api").await;
        let err = init.run(&["task", "init"]).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{}", err.message);
        assert_eq!(init.prompt.asked().len(), asked);
        assert!(!init.store().path().exists());
    }
}

/// 保存の後でプロジェクトを選ばずに終えると、確かめた接続だけが残る（再実行で続きから）。
#[tokio::test]
async fn an_abandoned_project_choice_keeps_the_verified_connection() {
    let init = Init::unconfigured(None).await;
    let uri = init.server.uri();
    let init = init.prompt(ScriptedPrompt::terminal(&[&uri], &["good-token"]));
    answer_as_api(&init.server, "/api").await;
    mount_me(&init.server, "/api", "good-token", token_identity_json()).await;
    mount_projects(&init.server, "/api", TENANT, one_match()).await;

    let err = init.run(&["task", "init"]).await.unwrap_err();
    assert_eq!(err.exit_code, 2, "{}", err.message);
    assert_eq!(init.written(), None);
    assert_eq!(
        init.store().load().unwrap().api_url,
        Some(format!("{uri}/api"))
    );
}

/// 既にある値は変えない。足りない方だけを足す。
#[tokio::test]
async fn only_the_missing_half_of_the_connection_is_added() {
    let half = |prompt: Rc<ScriptedPrompt>| async {
        let init = Init::unconfigured(Some(ORIGIN)).await.prompt(prompt);
        let api = format!("{}/api", init.server.uri());
        init.store()
            .save(&TaskConfig {
                api_url: Some(api.clone()),
                ..Default::default()
            })
            .unwrap();
        mount_me(&init.server, "/api", "good-token", token_identity_json()).await;
        mount_projects(&init.server, "/api", TENANT, one_match()).await;
        (init, api)
    };

    // URL は聞かず（確かめ直しもせず）、トークンだけを足す
    let (init, api) = half(ScriptedPrompt::terminal(&[], &["good-token"])).await;
    assert_eq!(init.run(&["task", "init", "--yes"]).await.unwrap(), 0);
    assert_eq!(init.prompt.asked(), ["secret:Token: "]);
    let config = init.store().load().unwrap();
    assert_eq!(config.api_url, Some(api.clone()));
    assert_eq!(config.token.as_deref(), Some("good-token"));
    for request in init.requests().await {
        assert!(
            request.headers.contains_key("authorization"),
            "疎通は送らない"
        );
    }

    // 同じ値の --api-url は通る（`/api` の無い形でも）
    let (init, _) = half(ScriptedPrompt::terminal(&[], &["good-token"])).await;
    let uri = init.server.uri();
    assert_eq!(
        init.run(&["task", "init", "--yes", "--api-url", &uri])
            .await
            .unwrap(),
        0
    );

    // 違う値の --api-url は、聞かずに止まる
    let (init, api) = half(ScriptedPrompt::terminal(&[], &["good-token"])).await;
    let err = init
        .run(&["task", "init", "--api-url", "https://other.invalid"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 2, "{}", err.message);
    assert!(err.message.contains("already set"), "{}", err.message);
    assert!(init.prompt.asked().is_empty());
    assert!(init.requests().await.is_empty());
    assert_eq!(init.store().load().unwrap().api_url, Some(api));

    // 接続が揃っているときも同じ
    let init = Init::connected(Some(ORIGIN)).await;
    let err = init
        .run(&[
            "task",
            "init",
            "--api-url",
            "https://other.invalid",
            "--yes",
        ])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 2, "{}", err.message);
    assert!(init.requests().await.is_empty());
}

/// `--profile` を付けるとそのプロファイルへ書き、`.koyori-task.yaml` に `profile` も入れる。
#[tokio::test]
async fn the_profile_flag_chooses_where_the_connection_is_saved() {
    let top = TaskConfig {
        api_url: Some("https://top.invalid/api".into()),
        token: Some("top-token".into()),
        ..Default::default()
    };
    let setup = || async {
        let init = Init::unconfigured(Some(ORIGIN)).await;
        let uri = init.server.uri();
        let init = init.prompt(ScriptedPrompt::terminal(&[&uri], &["good-token"]));
        answer_as_api(&init.server, "/api").await;
        mount_me(&init.server, "/api", "good-token", token_identity_json()).await;
        mount_projects(&init.server, "/api", TENANT, one_match()).await;
        (init, uri)
    };

    // 未定義の名前: プロファイルを作る。トップレベルは変えない
    let (init, uri) = setup().await;
    init.store().save(&top).unwrap();
    assert_eq!(
        init.run(&["task", "--profile", "work", "init", "--yes"])
            .await
            .unwrap(),
        0
    );
    let config = init.store().load().unwrap();
    assert_eq!(
        config.profiles["work"],
        Profile {
            api_url: Some(format!("{uri}/api")),
            token: Some("good-token".into()),
            tenant_id: None,
        }
    );
    assert_eq!(
        (config.api_url, config.token),
        (top.api_url.clone(), top.token.clone())
    );
    assert_eq!(
        init.written().as_deref(),
        Some("project: APP\nprofile: work\n")
    );
    // 書いたファイルを読むと、同じプロファイルで同じプロジェクトに届く
    let repo = task_cli::repo_config::load(init.repo.path())
        .unwrap()
        .unwrap();
    assert_eq!(repo.profile.as_deref(), Some("work"));
    assert_eq!(repo.project.as_deref(), Some("APP"));

    // `default` はトップレベル。`profile` は書かない
    let (init, uri) = setup().await;
    assert_eq!(
        init.run(&["task", "--profile", "default", "init", "--yes"])
            .await
            .unwrap(),
        0
    );
    let config = init.store().load().unwrap();
    assert_eq!(config.api_url, Some(format!("{uri}/api")));
    assert!(config.profiles.is_empty());
    assert_eq!(init.written().as_deref(), Some("project: APP\n"));

    // 不正な名前は何もせずに止まる
    let (init, _) = setup().await;
    let err = init
        .run(&["task", "--profile", "no/pe", "init"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 2);
    assert!(init.prompt.asked().is_empty());
    assert!(!init.store().path().exists());
}

/// 書く層と読む層が食い違うとき（`TASK_PROFILE`・片側 env）は、聞かずに今のエラーで止まる。
#[tokio::test]
async fn does_not_ask_when_the_connection_is_chosen_by_the_environment() {
    for env in [
        &[("TASK_PROFILE", "work")][..],
        &[("TASK_API_URL", "http://127.0.0.1:9")],
        &[("TASK_TOKEN", "env-token")],
    ] {
        let init = Init::unconfigured(Some(ORIGIN))
            .await
            .env(env)
            .prompt(ScriptedPrompt::terminal(&["https://x.invalid"], &["t"]));
        let err = init
            .run(&["task", "init", "--api-url", "https://x.invalid"])
            .await
            .unwrap_err();
        assert_eq!(err.exit_code, 2, "{env:?}: {}", err.message);
        assert!(init.prompt.asked().is_empty(), "{env:?}");
        assert!(!init.store().path().exists(), "{env:?}");
        assert!(init.requests().await.is_empty(), "{env:?}");
        assert_eq!(init.written(), None);
    }
}

/// env の組は接続にそのまま使い、ホームの設定は書かない。
#[tokio::test]
async fn an_environment_pair_is_used_without_saving() {
    let init = Init::unconfigured(Some(ORIGIN)).await;
    let uri = init.server.uri();
    let init = init.env(&[
        ("TASK_API_URL", &uri),
        ("TASK_TOKEN", "token-1"),
        ("TASK_PROFILE", "gone"),
    ]);
    mount_me(&init.server, "", "token-1", token_identity_json()).await;
    mount_projects(&init.server, "", TENANT, one_match()).await;

    assert_eq!(init.run(&["task", "init", "--yes"]).await.unwrap(), 0);
    assert_eq!(init.written().as_deref(), Some("project: APP\n"));
    assert!(!init.store().path().exists());
}

/// 端末でないときは、`--api-url` と標準入力のトークンで進める。
#[tokio::test]
async fn takes_the_url_and_the_token_without_a_terminal() {
    let init = Init::unconfigured(Some(ORIGIN))
        .await
        .prompt(ScriptedPrompt::piped(Some("good-token\n")));
    let uri = init.server.uri();
    answer_as_api(&init.server, "/api").await;
    mount_me(&init.server, "/api", "good-token", token_identity_json()).await;
    mount_projects(&init.server, "/api", TENANT, one_match()).await;

    assert_eq!(
        init.run(&["task", "init", "--api-url", &uri, "--project", "APP"])
            .await
            .unwrap(),
        0
    );
    assert_eq!(init.prompt.asked(), ["piped:token"], "聞かない");
    let config = init.store().load().unwrap();
    assert_eq!(config.api_url, Some(format!("{uri}/api")));
    assert_eq!(config.token.as_deref(), Some("good-token"));
    assert_eq!(init.written().as_deref(), Some("project: APP\n"));

    // --api-url が無い
    let init = Init::unconfigured(Some(ORIGIN))
        .await
        .prompt(ScriptedPrompt::piped(Some("good-token\n")));
    let err = init
        .run(&["task", "init", "--project", "APP"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 2, "{}", err.message);
    assert!(err.message.contains("--api-url"), "{}", err.message);
    assert!(init.requests().await.is_empty());
    assert!(!init.store().path().exists());

    // トークンが繋がっていない・空
    for piped in [None, Some("  \n")] {
        let init = Init::unconfigured(Some(ORIGIN))
            .await
            .prompt(ScriptedPrompt::piped(piped));
        let uri = init.server.uri();
        answer_as_api(&init.server, "/api").await;
        let err = init
            .run(&["task", "init", "--api-url", &uri, "--project", "APP"])
            .await
            .unwrap_err();
        assert_eq!(err.exit_code, 2, "{piped:?}: {}", err.message);
        assert!(!init.store().path().exists());
        for request in init.requests().await {
            assert!(!request.headers.contains_key("authorization"));
        }
    }
}

fn multi_tenant_identity() -> Value {
    let mut identity = token_identity_json();
    identity["tenant_ids"] = json!([OTHER_TENANT, TENANT]);
    identity.as_object_mut().unwrap().remove("tenant_id");
    identity
}

/// 複数のテナントで使えるトークンは、端末で選ばせて、書いてよい層へ `tenant_id` を保存する。
#[tokio::test]
async fn a_multi_tenant_token_asks_which_tenant_to_use() {
    let multi = |profile: Option<&'static str>, prompt: Rc<ScriptedPrompt>| async move {
        let init = Init::unconfigured(Some(ORIGIN)).await.prompt(prompt);
        let connection = Profile {
            api_url: Some(init.server.uri()),
            token: Some("token-1".into()),
            tenant_id: None,
        };
        let config = match profile {
            None => TaskConfig {
                api_url: connection.api_url,
                token: connection.token,
                ..Default::default()
            },
            Some(name) => TaskConfig {
                profiles: [(name.to_string(), connection)].into(),
                ..Default::default()
            },
        };
        init.store().save(&config).unwrap();
        mount_me(&init.server, "", "token-1", multi_tenant_identity()).await;
        Mock::given(method("GET"))
            .and(path("/v1/tenants"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": TENANT, "display_id": "acme", "name": "Acme"},
                {"id": OTHER_TENANT, "display_id": "other", "name": "Other"},
            ])))
            .mount(&init.server)
            .await;
        mount_projects(&init.server, "", TENANT, one_match()).await;
        (init, config)
    };

    // トップレベル。束縛は昇順（OTHER_TENANT, TENANT）なので 2 番が TENANT
    let (init, _) = multi(None, ScriptedPrompt::terminal(&["2"], &[])).await;
    assert_eq!(init.run(&["task", "init", "--yes"]).await.unwrap(), 0);
    assert_eq!(init.prompt.asked(), ["line:Tenant [1-2]: "]);
    assert_eq!(
        init.store().load().unwrap().tenant_id.as_deref(),
        Some(TENANT)
    );
    assert_eq!(init.calls_to(&projects_path("", TENANT)).await, 1);
    assert_eq!(init.written().as_deref(), Some("project: APP\n"));

    // `--profile work` はそのプロファイルへ。トップレベルは変えない
    let (init, _) = multi(Some("work"), ScriptedPrompt::terminal(&["2"], &[])).await;
    assert_eq!(
        init.run(&["task", "--profile", "work", "init", "--yes"])
            .await
            .unwrap(),
        0
    );
    let config = init.store().load().unwrap();
    assert_eq!(config.profiles["work"].tenant_id.as_deref(), Some(TENANT));
    assert_eq!(config.tenant_id, None);
    assert_eq!(
        init.written().as_deref(),
        Some("project: APP\nprofile: work\n")
    );

    // 範囲外・入力なしは保存しない
    for answers in [&["3"][..], &["acme"], &[]] {
        let (init, before) = multi(None, ScriptedPrompt::terminal(answers, &[])).await;
        let err = init.run(&["task", "init", "--yes"]).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{answers:?}: {}", err.message);
        assert_eq!(init.store().load().unwrap(), before);
        assert_eq!(init.written(), None);
    }

    // 端末でない: 今の文で止まる
    let (init, before) = multi(None, ScriptedPrompt::piped(None)).await;
    let err = init.run(&["task", "init", "--yes"]).await.unwrap_err();
    assert_eq!(err.exit_code, 2, "{}", err.message);
    assert!(
        err.message.contains("This token can be used in 2 tenants"),
        "{}",
        err.message
    );
    assert_eq!(init.store().load().unwrap(), before);
    assert_eq!(init.written(), None);

    // `TASK_PROFILE` で選んだ層と、env の組は、端末でも選ばせない
    let (init, before) = multi(Some("work"), ScriptedPrompt::terminal(&["2"], &[])).await;
    let init = init.env(&[("TASK_PROFILE", "work")]);
    let err = init.run(&["task", "init", "--yes"]).await.unwrap_err();
    assert_eq!(err.exit_code, 2, "{}", err.message);
    assert!(init.prompt.asked().is_empty());
    assert_eq!(init.store().load().unwrap(), before);

    let (init, before) = multi(None, ScriptedPrompt::terminal(&["2"], &[])).await;
    let uri = init.server.uri();
    let init = init.env(&[("TASK_API_URL", &uri), ("TASK_TOKEN", "token-1")]);
    let err = init.run(&["task", "init", "--yes"]).await.unwrap_err();
    assert_eq!(err.exit_code, 2, "{}", err.message);
    assert!(init.prompt.asked().is_empty());
    assert_eq!(init.store().load().unwrap(), before);
    assert_eq!(init.written(), None);

    // `TASK_TENANT` の明示があれば聞かない
    let (init, before) = multi(None, ScriptedPrompt::terminal(&["1"], &[])).await;
    let init = init.env(&[("TASK_TENANT", TENANT)]);
    assert_eq!(init.run(&["task", "init", "--yes"]).await.unwrap(), 0);
    assert!(init.prompt.asked().is_empty());
    assert_eq!(init.store().load().unwrap(), before);
}

/// 実バイナリ。`--json` の標準出力は JSON 1 つで、案内（標準エラー）が混ざらない。
#[tokio::test]
async fn json_output_is_a_single_document_on_stdout() {
    let server = MockServer::start().await;
    mount_me(&server, "", "token-1", token_identity_json()).await;
    mount_projects(&server, "", TENANT, one_match()).await;
    let repo = git_repo(Some(ORIGIN));

    let uri = server.uri();
    let run = move |args: &'static [&'static str], cwd: PathBuf| {
        let uri = uri.clone();
        tokio::task::spawn_blocking(move || run_binary(&uri, args, &cwd))
    };

    let (code, stdout, stderr) = run(&["--json", "init", "--yes"], repo.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(code, Some(0), "{stderr}");
    let file = repo.path().join(FILE);
    let result: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(result["created"], true);
    assert_eq!(result["project"], "APP");
    assert_eq!(result["profile"], Value::Null);
    assert_eq!(
        std::fs::canonicalize(result["path"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(&file).unwrap()
    );
    assert!(stderr.contains("Token: review-bot"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "project: APP\n");

    // 2 回目は既存のファイルを示すだけ
    let (code, stdout, _) = run(&["--json", "init", "--yes"], repo.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(code, Some(0));
    let result: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(result["created"], false);

    // 端末でなく、決め手も無ければ終了コード 2
    let other = git_repo(None);
    let (code, stdout, stderr) = run(&["--json", "init"], other.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(!other.path().join(FILE).exists());
}

/// 実バイナリを、env の組の接続で走らせる。標準入力は端末でない（何も繋がない）。
fn run_binary(uri: &str, args: &[&str], cwd: &Path) -> (Option<i32>, String, String) {
    let home = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_task"))
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("HOME", home.path())
        // `init` が `git remote get-url origin` を呼ぶ
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("TASK_API_URL", uri)
        .env("TASK_TOKEN", "token-1")
        .env("TASK_TENANT", TENANT)
        // 実バイナリは `from_process` を通る。通知で GitHub へ出ないようにする
        .env("TASK_NO_UPDATE_CHECK", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}
