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
pub mod resolve;
pub mod text_input;
pub mod update;

use api::ApiClient;
use cli::{Cli, Command};
use config::{ConfigStore, RuntimeConfig, resolve_runtime_with};
use error::Result;
use output::OutputOptions;

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
}

impl Context {
    pub fn from_process() -> Result<Self> {
        let mut context = Self::new(ConfigStore::discover()?, |key| std::env::var(key).ok());
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
        }
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
