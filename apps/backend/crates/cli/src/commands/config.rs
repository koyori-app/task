//! `~/.config/task/config.yaml` の操作。

use serde::Serialize;
use serde_json::json;

use crate::cli::ConfigCommand;
use crate::config::{CONFIG_KEYS, ConfigStore, profile_label};
use crate::error::{CliError, Result};
use crate::output::{OutputOptions, print};

/// `config list`（`--profile` なし）の出力。プロファイルは名前だけを出す。
#[derive(Serialize)]
struct ConfigListing<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    api_url: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_id: Option<&'a String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<&'a str>,
}

/// `profile` は対象（`None` はトップレベル）。`TASK_PROFILE` と `.koyori-task.yaml` は見ない。
pub fn run(
    command: ConfigCommand,
    store: &ConfigStore,
    profile: Option<&str>,
    output: OutputOptions,
) -> Result<i32> {
    let path = store.path();
    let label = profile_label(profile);
    match command {
        ConfigCommand::List => {
            let config = store.load()?;
            match profile {
                None => print(
                    &ConfigListing {
                        api_url: config.api_url.as_ref(),
                        token: config.token.as_ref(),
                        tenant_id: config.tenant_id.as_ref(),
                        profiles: config.profile_names(),
                    },
                    output,
                ),
                Some(name) => {
                    // 無い名前は `value` と同じエラーにする
                    config.value(Some(name), "api_url", &path)?;
                    print(&config.profiles[name], output);
                }
            }
        }
        ConfigCommand::Get { key } => {
            let key = check_key(&key)?;
            let config = store.load()?;
            let value = config.value(profile, key, &path)?;
            if output.json {
                print(&json!({ "key": key, "value": value }), output);
            } else {
                println!("{}", value.map(String::as_str).unwrap_or_default());
            }
        }
        ConfigCommand::Set { key, value } => {
            let key = check_key(&key)?;
            let mut config = store.load()?;
            // プロファイルを作れるのはここだけ（不完全でよい。完全性は接続時に見る）
            config.set_value(profile, key, Some(value), true, &path)?;
            store.save(&config)?;
            println!("Set {key} in {} (profile: {label})", path.display());
        }
        ConfigCommand::Unset { key } => {
            let key = check_key(&key)?;
            let mut config = store.load()?;
            // 空になってもプロファイルは残す（`work: {}`）
            config.set_value(profile, key, None, false, &path)?;
            store.save(&config)?;
            println!("Removed {key} from {} (profile: {label})", path.display());
        }
    }
    Ok(0)
}

fn check_key(key: &str) -> Result<&'static str> {
    CONFIG_KEYS
        .into_iter()
        .find(|allowed| *allowed == key)
        .ok_or_else(|| CliError::validation(format!("Unknown config key: {key}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_key_the_config_file_does_not_have() {
        let err = check_key("api-url").unwrap_err();
        assert!(err.message.contains("Unknown config key: api-url"));
        assert_eq!(err.exit_code, 2);
    }

    #[test]
    fn accepts_every_documented_key() {
        for key in CONFIG_KEYS {
            assert_eq!(check_key(key).unwrap(), key);
        }
    }
}
