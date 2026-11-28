//! 認証。トークンの保存だけは API を呼ばずに済ませる。

use payload::personal_tokens::PersonalTokenIdentityResponse;

use crate::Context;
use crate::cli::AuthCommand;
use crate::config::profile_label;
use crate::error::{CliError, Result};
use crate::output::{OutputOptions, print};
use crate::text_input::read_stdin;

pub async fn run(command: AuthCommand, context: &Context, output: OutputOptions) -> Result<i32> {
    let store = context.store();
    let path = store.path();
    // 書く操作の対象は `--profile` かトップレベルだけ（`TASK_PROFILE` と `.koyori-task.yaml` は見ない）
    let profile = context.profile_flag();
    match command {
        AuthCommand::Whoami => {
            let api = context.connect_token_only()?;
            let token: PersonalTokenIdentityResponse =
                api.get(&["v1", "personal_tokens", "me"], &[]).await?;
            print(&token, output);
        }
        AuthCommand::Token { token } => {
            let mut config = store.load()?;
            // 無いプロファイルは標準入力を読む前に止める。作れるのは `config set --profile` だけ
            config.value(profile, "token", &path)?;
            let token = match token
                .map(|value| value.trim().to_string())
                .filter(|v| !v.is_empty())
            {
                Some(token) => token,
                None => read_stdin("token")?.unwrap_or_default().trim().to_string(),
            };
            if token.is_empty() {
                return Err(CliError::new("Token is required"));
            }
            config.set_value(profile, "token", Some(token), false, &path)?;
            store.save(&config)?;
            println!(
                "Token saved to {} (profile: {})",
                path.display(),
                profile_label(profile)
            );
        }
        AuthCommand::Logout => {
            let mut config = store.load()?;
            config.set_value(profile, "token", None, false, &path)?;
            store.save(&config)?;
            println!(
                "Token removed from {} (profile: {})",
                path.display(),
                profile_label(profile)
            );
        }
    }
    Ok(0)
}
