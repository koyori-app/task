use task_cli::Context;
use task_cli::cli::{Cli, Command};

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse_args(std::env::args()).unwrap_or_else(|err| err.exit());
    let context = match Context::from_process() {
        Ok(context) => context,
        Err(err) => return fail(err),
    };
    // `self-update` の直後に「新しい版があります」と言っても意味が無い
    let self_update = matches!(cli.command, Command::SelfUpdate);
    let result = task_cli::run(cli, &context).await;
    // 版のずれで失敗したときこそ通知が要るので、成功でも失敗でも出す。終了コードは変えない
    if !self_update && let Some(line) = task_cli::update_notice(&context).await {
        eprintln!("{line}");
    }
    match result {
        Ok(code) => std::process::ExitCode::from(code as u8),
        Err(err) => fail(err),
    }
}

fn fail(err: task_cli::error::CliError) -> std::process::ExitCode {
    eprintln!("{}", err.message);
    std::process::ExitCode::from(err.exit_code as u8)
}
