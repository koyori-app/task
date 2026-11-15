use task_cli::Context;
use task_cli::cli::{Cli, Command};

fn main() -> std::process::ExitCode {
    // `#[tokio::main]` だと、ランタイムの drop が `spawn_blocking` の `getaddrinfo` の終わりを待つ。
    // DNS が応答しない環境では、通知の取得がタイムアウトしたあとも終了が遅れる。
    // 自分で作り、終わったら待たずに閉じる（本体の出力は `block_on` の中で書き終えている）
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to start the async runtime");
    let code = runtime.block_on(run());
    runtime.shutdown_background();
    code
}

async fn run() -> std::process::ExitCode {
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
