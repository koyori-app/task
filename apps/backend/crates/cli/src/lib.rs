//! Task management CLI。
//!
//! API の型（`payload`）と検証の規則（`entity` / `common`）は backend と共有する。
//! CLI 側に写しを置くと、API の変更に追いつかないまま黙って食い違う（#647）。

pub mod api;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod output;
pub mod repo_config;
pub mod resolve;
pub mod text_input;
pub mod update;

use std::cell::OnceCell;
use std::path::PathBuf;

use api::ApiClient;
use cli::{Cli, Command};
use config::{ConfigStore, RuntimeConfig, resolve_runtime_with};
use error::Result;
use output::OutputOptions;
use repo_config::RepoConfig;

/// 環境変数の読み取り。テストは実プロセスの環境を見ない実装を差し込む。
type EnvLookup = Box<dyn Fn(&str) -> Option<String>>;

/// 設定ファイルと環境変数の出どころ。
///
/// テストが実プロセスの環境変数に左右されないよう、読み取り先を値で持つ。
pub struct Context {
    store: ConfigStore,
    env: EnvLookup,
    /// 新しい版の確認と `self-update` の相手。`Context::new` では `None`（外へ出る経路が無い）。
    /// 実プロセスだけが `from_process` で入れる
    updater: Option<update::Updater>,
    /// `.koyori-task.yaml` を探す起点。`None` は探さない（`Context::new` の既定。
    /// テストがこのリポジトリ自身のファイルに左右されないため）
    repo_dir: Option<PathBuf>,
    /// 読んだ `.koyori-task.yaml`。1 プロセスで 1 回だけ読み、以降は同じ内容を使う
    repo: OnceCell<Option<RepoConfig>>,
}

impl Context {
    pub fn from_process() -> Result<Self> {
        let mut context = Self::new(ConfigStore::discover()?, |key| std::env::var(key).ok());
        // cwd が取れない（消されたディレクトリなど）ときは git の外と同じに扱う。
        // ここで失敗させると `config set` まで動かなくなる
        context.repo_dir = std::env::current_dir().ok();
        context.updater = std::env::current_exe()
            .and_then(std::fs::canonicalize)
            .ok()
            .map(|exe| update::Updater {
                source: update::ReleaseSource::github(),
                installed: update::Installed {
                    version: env!("TASK_CLI_VERSION").to_string(),
                    target: env!("TASK_CLI_TARGET").to_string(),
                    exe,
                    from_release: env!("TASK_CLI_VERSION_SOURCE") == "env",
                },
            });
        Ok(context)
    }

    pub fn new(store: ConfigStore, env: impl Fn(&str) -> Option<String> + 'static) -> Self {
        Self {
            store,
            env: Box::new(env),
            updater: None,
            repo_dir: None,
            repo: OnceCell::new(),
        }
    }

    /// `.koyori-task.yaml` を探す起点を差し込む（テスト用。実プロセスは `from_process` が cwd を入れる）。
    pub fn with_repo_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.repo_dir = Some(dir.into());
        self
    }

    /// `.koyori-task.yaml` を（まだなら）読む。失敗は覚えずに返す。
    ///
    /// `profile` はまだ扱えない。黙って無視すると、別のトークンで送るつもりの要求が
    /// 既定のトークンで送られるので、要求を送る前に止める
    fn repo_config(&self) -> Result<Option<&RepoConfig>> {
        if let Some(config) = self.repo.get() {
            return Ok(config.as_ref());
        }
        let config = match &self.repo_dir {
            None => None,
            Some(dir) => repo_config::load(dir)?,
        };
        if let Some(config) = &config {
            for key in &config.unknown_keys {
                eprintln!(
                    "warning: unknown key \"{key}\" in {} (ignored)",
                    config.path.display()
                );
            }
            if config.profile.is_some() {
                return Err(error::CliError::validation(format!(
                    "`profile` in {} is not supported by this version of the CLI; remove it or update the CLI",
                    config.path.display()
                )));
            }
        }
        Ok(self.repo.get_or_init(|| config).as_ref())
    }

    /// 明示が無ければ `.koyori-task.yaml` の `project`。どちらも無ければ `None`。
    /// 既定値を使ったときだけ、由来を標準エラーに 1 行出す（`--json` の出力は変えない）
    pub(crate) fn project_or_default(&self, explicit: Option<String>) -> Result<Option<String>> {
        if explicit.is_some() {
            // ファイルはここでは読まない。接続（`runtime`）が必ず読む
            return Ok(explicit);
        }
        let Some(config) = self.repo_config()? else {
            return Ok(None);
        };
        let Some(project) = config.project.clone() else {
            return Ok(None);
        };
        eprintln!("{}", config.project_source_line(&project));
        Ok(Some(project))
    }

    /// `--project` を省ける口のプロジェクト。明示 > `.koyori-task.yaml` > エラー。
    pub(crate) fn project(&self, explicit: Option<String>) -> Result<String> {
        self.project_or_default(explicit)?.ok_or_else(|| {
            error::CliError::validation(format!(
                "--project is required: pass --project <key or UUID>, or put `project: <key or UUID>` in {} at the repository root",
                repo_config::FILE_NAME
            ))
        })
    }

    /// 新しい版の確認と `self-update` の相手を差し込む（テスト用。実プロセスは `from_process`）。
    pub fn with_updater(mut self, updater: update::Updater) -> Self {
        self.updater = Some(updater);
        self
    }

    pub fn store(&self) -> &ConfigStore {
        &self.store
    }

    fn runtime(&self) -> Result<RuntimeConfig> {
        // 要求を送る口は全部ここを通る。`--project` の明示や env の指定があっても、
        // 壊れたファイルや未対応の `profile` を見逃さないよう、設定の判定より先に読む
        self.repo_config()?;
        resolve_runtime_with(&self.store, |key| (self.env)(key))
    }

    /// テナント付きのパスを叩く API クライアントを作る。tenant_id が未設定なら
    /// `personal_tokens/me` を 1 回叩いて埋める。設定の不足を、引数の検証より先に
    /// 報告しないよう呼ぶのは各コマンドが実際に送る直前にする。
    pub(crate) async fn connect(&self) -> Result<ApiClient> {
        ApiClient::new(self.runtime()?)?.resolve_tenant().await
    }

    /// テナントを含まないパスだけを叩く口（`auth whoami` と通知）。tenant を引かない。
    pub(crate) fn connect_token_only(&self) -> Result<ApiClient> {
        ApiClient::new(RuntimeConfig {
            tenant: None,
            ..self.runtime()?
        })
    }
}

/// プロセスの終了コードを返す。0 以外はゲート不成立か失敗。
pub async fn run(cli: Cli, context: &Context) -> Result<i32> {
    let json = cli.json_request()?;
    let output = OutputOptions::new(json.enabled);
    let fields = json.fields;
    // 項目を選べるのは `tasks list` だけ。ほかで黙って無視すると、絞れたつもりで全部を読んでしまう
    let is_task_list = matches!(
        &cli.command,
        Command::Tasks { command } if matches!(**command, cli::TasksCommand::List { .. })
    );
    if fields.is_some() && !is_task_list {
        return Err(error::CliError::validation(
            "--json <fields> is supported only by `tasks list`; use --json without fields here",
        ));
    }

    // 設定を要らないコマンド（`config` と手元のトークン操作）を、設定不足で落とさない。
    // 落とすと、トークンを保存する前に何もできなくなる
    match cli.command {
        Command::Config { command } => commands::config::run(command, context.store(), output),
        Command::Auth { command } => commands::auth::run(command, context, output).await,
        Command::Projects { command } => commands::projects::run(context, command, output).await,
        Command::Tasks { command } => commands::tasks::run(context, *command, output, fields).await,
        Command::My { command } => commands::my::run(context, command, output).await,
        Command::Sprints { command } => commands::sprints::run(context, command, output).await,
        Command::Review { command } => commands::reviews::run(context, command, output).await,
        Command::Notifications { command } => {
            commands::notifications::run(context, command, output).await
        }
        Command::Tenants { command } => commands::tenants::run(context, command, output).await,
        Command::SelfUpdate => match &context.updater {
            Some(updater) => update::self_update(updater, output).await,
            None => Err(error::CliError::new(
                "self-update is not available in this build",
            )),
        },
    }
}

/// 新しい版があるときの 1 行（標準エラーに出す）。コマンドの結果にかかわらず、最後に呼ぶ。
/// 失敗は全部 `None` で、本体の結果を変えない。
pub async fn update_notice(context: &Context) -> Option<String> {
    let updater = context.updater.as_ref()?;
    update::notice(
        updater,
        &context.store.update_state_path(),
        chrono::Utc::now(),
        &|key| (context.env)(key),
    )
    .await
}
