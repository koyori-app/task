//! 新しい版の通知と `self-update`。
//!
//! 配布物は `task-<版>-<target>.tar.gz` と、その SHA-256（`<配布物>.sha256`）。GitHub の Releases から取る。
//! **task のトークンは GitHub へ送らない**（API 用のクライアントとは別の `reqwest::Client` を使う）。

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::error::{CliError, Result};
use crate::output::{OutputOptions, print};

/// 通知のための確認の間隔。
const CHECK_INTERVAL_HOURS: i64 = 24;
/// 通知のための確認の待ち時間。コマンドの最後に待つので短くする。
const NOTICE_TIMEOUT: Duration = Duration::from_secs(2);
/// `self-update` が最新の版を確かめる待ち時間。
const LATEST_TIMEOUT: Duration = Duration::from_secs(60);
/// 配布物を落とす待ち時間。
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// 配布物の置き場所。テストは wiremock を指す。
#[derive(Debug, Clone)]
pub struct ReleaseSource {
    /// `{api_base}/releases/latest` が最新のリリースを返す
    pub api_base: String,
    /// `{download_base}/v{版}/{ファイル}` が配布物
    pub download_base: String,
}

impl ReleaseSource {
    pub fn github() -> Self {
        Self {
            api_base: "https://api.github.com/repos/koyori-app/task".into(),
            download_base: "https://github.com/koyori-app/task/releases/download".into(),
        }
    }

    fn client(&self, timeout: Duration) -> Result<reqwest::Client> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            "application/vnd.github+json".parse().unwrap(),
        );
        reqwest::Client::builder()
            .user_agent(concat!("task-cli/", env!("TASK_CLI_VERSION")))
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .map_err(CliError::from)
    }

    /// 最新のリリースの版（タグの先頭の `v` を外したもの）。
    async fn latest_version(&self, timeout: Duration) -> Result<String> {
        let response = self
            .client(timeout)?
            .get(format!("{}/releases/latest", self.api_base))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(CliError::new(format!("the release API returned {status}")));
        }
        let body: serde_json::Value = response.json().await?;
        let tag = body
            .get("tag_name")
            .and_then(|tag| tag.as_str())
            .ok_or_else(|| CliError::new("the release has no tag_name"))?;
        Ok(tag.strip_prefix('v').unwrap_or(tag).to_string())
    }

    /// 配布物のファイル。404 は `None`。
    async fn download(&self, version: &str, file: &str) -> Result<Option<Vec<u8>>> {
        let response = self
            .client(DOWNLOAD_TIMEOUT)?
            .get(format!("{}/v{version}/{file}", self.download_base))
            .send()
            .await?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(CliError::new(format!(
                "Cannot download {file}: the server returned {status}"
            )));
        }
        Ok(Some(response.bytes().await?.to_vec()))
    }
}

/// 今動いている `task`。
#[derive(Debug, Clone)]
pub struct Installed {
    pub version: String,
    /// 配布物の名前に入る target（`x86_64-unknown-linux-gnu` など）
    pub target: String,
    pub exe: PathBuf,
    /// タグからのリリースか。手元のビルドは版を名乗れない（0.1.0）ので通知しない
    pub from_release: bool,
}

#[derive(Debug, Clone)]
pub struct Updater {
    pub source: ReleaseSource,
    pub installed: Installed,
}

/// 先頭の `v` を外した `数.数.数` だけを読む。`1.0.0-rc1` などは読まない（通知も更新もしない）。
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.strip_prefix('v').unwrap_or(text);
    let mut parts = text.split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}

/// `.sha256`（`sha256sum` の形式 `<hex>  <ファイル名>`）の hex。
fn checksum_of(text: &str) -> Option<&str> {
    text.split_whitespace().next()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CheckState {
    checked_at: DateTime<Utc>,
}

/// 新しい版があるときの 1 行。標準エラーに出す。
///
/// 何も叩かずに `None`: `TASK_NO_UPDATE_CHECK` か `CI` が空でない、手元のビルド、24 時間以内に確かめた。
///
/// **失敗は全部 `None`**（通信・HTTP の状態（制限の 403 を含む）・JSON・状態ファイルの読み書き）。
/// 通知はおまけで、本体のコマンドの結果を変えてはいけない。ここは例外的に握り潰す。
pub async fn notice(
    updater: &Updater,
    state_path: &Path,
    now: DateTime<Utc>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let set = |key: &str| env(key).is_some_and(|value| !value.is_empty());
    if set("TASK_NO_UPDATE_CHECK") || set("CI") || !updater.installed.from_release {
        return None;
    }
    if let Some(state) = std::fs::read_to_string(state_path)
        .ok()
        .and_then(|text| serde_json::from_str::<CheckState>(&text).ok())
        && now.signed_duration_since(state.checked_at)
            < chrono::Duration::hours(CHECK_INTERVAL_HOURS)
    {
        return None;
    }
    // 叩く前に書く。失敗しても 1 日 1 回に収まる。書けないときは叩かない（毎回叩いてしまう）
    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    let state = serde_json::to_string(&CheckState { checked_at: now }).ok()?;
    std::fs::write(state_path, state).ok()?;

    let latest = updater.source.latest_version(NOTICE_TIMEOUT).await.ok()?;
    let installed = parse_version(&updater.installed.version)?;
    (parse_version(&latest)? > installed).then(|| {
        format!(
            "A new release of task is available: {} -> {latest}. Run `task self-update` to install it.",
            updater.installed.version
        )
    })
}

/// 最新の配布物を、SHA-256 を確かめてから今の `task` と入れ替える。
pub async fn self_update(updater: &Updater, output: OutputOptions) -> Result<i32> {
    if cfg!(windows) {
        return Err(CliError::new(
            "self-update is not available on Windows; download the .zip from https://github.com/koyori-app/task/releases/latest",
        ));
    }
    let installed = &updater.installed;
    let latest = updater
        .source
        .latest_version(LATEST_TIMEOUT)
        .await
        .map_err(|err| {
            CliError::new(format!("Cannot check the latest release: {}", err.message))
        })?;
    let latest_parsed = parse_version(&latest)
        .ok_or_else(|| CliError::new(format!("Unexpected release version: {latest}")))?;
    // 今の版が読めないときは、頼まれた更新を進める
    if parse_version(&installed.version).is_some_and(|current| latest_parsed <= current) {
        report(
            output,
            false,
            &installed.version,
            &installed.version,
            &installed.exe,
            || format!("task {} is already the latest", installed.version),
        );
        return Ok(0);
    }

    let asset = format!("task-{latest}-{}.tar.gz", installed.target);
    let archive = updater
        .source
        .download(&latest, &asset)
        .await?
        .ok_or_else(|| missing_asset(&installed.target, &asset))?;
    let checksum = updater
        .source
        .download(&latest, &format!("{asset}.sha256"))
        .await?
        .ok_or_else(|| missing_asset(&installed.target, &format!("{asset}.sha256")))?;
    let checksum = String::from_utf8_lossy(&checksum);
    let expected = checksum_of(&checksum)
        .ok_or_else(|| CliError::new(format!("The checksum file for {asset} is empty")))?;
    let actual = hex::encode(Sha256::digest(&archive));
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(CliError::new(format!(
            "Checksum mismatch for {asset}: expected {expected}, got {actual}"
        )));
    }

    let binary = extract_binary(&archive)?;
    replace_binary(&installed.exe, &binary)?;
    report(
        output,
        true,
        &installed.version,
        &latest,
        &installed.exe,
        || {
            format!(
                "Updated task {} -> {latest} ({})",
                installed.version,
                installed.exe.display()
            )
        },
    );
    Ok(0)
}

fn missing_asset(target: &str, asset: &str) -> CliError {
    CliError::new(format!("No release asset for {target} ({asset})"))
}

fn report(
    output: OutputOptions,
    updated: bool,
    from: &str,
    to: &str,
    exe: &Path,
    message: impl FnOnce() -> String,
) {
    if output.json {
        print(
            &json!({ "updated": updated, "from": from, "to": to, "path": exe }),
            output,
        );
    } else {
        println!("{}", message());
    }
}

/// tar.gz の中の `task`（パスが `task` だけの要素）の中身。
fn extract_binary(archive: &[u8]) -> Result<Vec<u8>> {
    let corrupt = |err: std::io::Error| CliError::new(format!("Cannot read the archive: {err}"));
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    for entry in tar.entries().map_err(corrupt)? {
        let mut entry = entry.map_err(corrupt)?;
        let path = entry.path().map_err(corrupt)?.into_owned();
        // `./task` も `task` として読む
        let mut parts = path
            .components()
            .filter(|part| !matches!(part, Component::CurDir));
        let is_binary = matches!(parts.next(), Some(Component::Normal(name)) if name == "task")
            && parts.next().is_none();
        if is_binary {
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes).map_err(corrupt)?;
            return Ok(bytes);
        }
    }
    Err(CliError::new(
        "The archive does not contain the task binary",
    ))
}

/// 同じディレクトリの一時ファイルに書いてから `rename` で置き換える。
/// 途中で失敗しても、動いている `task` は壊れない。
fn replace_binary(exe: &Path, bytes: &[u8]) -> Result<()> {
    let temp = exe.with_file_name(format!(".task-update-{}", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        std::fs::write(&temp, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&temp, exe)
    })();
    result.map_err(|err| {
        let _ = std::fs::remove_file(&temp);
        CliError::new(format!("Cannot replace {}: {err}", exe.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_plain_three_part_versions() {
        assert_eq!(parse_version("0.1.51"), Some((0, 1, 51)));
        assert_eq!(parse_version("v1.20.3"), Some((1, 20, 3)));
        for unreadable in ["1.0.0-rc1", "1.0", "1.0.0.1", "v", "", "latest", "1.x.0"] {
            assert_eq!(parse_version(unreadable), None, "{unreadable}");
        }
        // 数として比べる（文字列なら 0.1.9 > 0.1.10 になる）
        assert!(parse_version("0.1.10") > parse_version("0.1.9"));
    }

    #[test]
    fn reads_the_hash_from_a_sha256sum_line_with_or_without_a_name() {
        assert_eq!(checksum_of("abc123  task-1.0.0-x.tar.gz\n"), Some("abc123"));
        assert_eq!(checksum_of("abc123\n"), Some("abc123"));
        assert_eq!(checksum_of("  \n"), None);
    }
}
