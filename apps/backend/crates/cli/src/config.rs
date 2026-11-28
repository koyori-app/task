//! `~/.config/task/config.yaml` の読み書きと、実行時設定の解決。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CliError, Result};

/// `config get` / `set` / `unset` が触れるキー。
pub const CONFIG_KEYS: [&str; 3] = ["api_url", "token", "tenant_id"];

/// トップレベルを明示する名前。`profiles.default` は作れず、手で書かれていても使わない。
pub const DEFAULT_PROFILE: &str = "default";

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// 名前付きの接続。選んだら `api_url` / `token` / `tenant_id` を丸ごとここから取る
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "profiles_or_empty"
    )]
    pub profiles: BTreeMap<String, Profile>,
}

/// 名前付きプロファイル 1 つ。トップレベルの値は継がない。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

impl Profile {
    pub fn get(&self, key: &str) -> Option<&String> {
        match key {
            "api_url" => self.api_url.as_ref(),
            "token" => self.token.as_ref(),
            "tenant_id" => self.tenant_id.as_ref(),
            _ => None,
        }
    }

    pub fn set(&mut self, key: &str, value: Option<String>) {
        match key {
            "api_url" => self.api_url = value,
            "token" => self.token = value,
            "tenant_id" => self.tenant_id = value,
            _ => {}
        }
    }
}

/// `profiles:` だけの行（null）や `work:` だけの行（null）を空として読む。
fn profiles_or_empty<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, Profile>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<BTreeMap<String, Option<Profile>>>::deserialize(deserializer)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .map(|(name, profile)| (name, profile.unwrap_or_default()))
        .collect())
}

/// プロファイル名の検証。`[A-Za-z0-9_-]{1,64}` で、予約名 `default` は弾く。
pub fn check_profile_name(name: &str) -> Result<()> {
    if name == DEFAULT_PROFILE {
        return Err(CliError::validation(
            "Profile name \"default\" is reserved for the top level of the config",
        ));
    }
    let valid = (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !valid {
        return Err(CliError::validation(format!(
            "Invalid profile name {name:?}: use 1-64 characters of A-Z, a-z, 0-9, _ and -"
        )));
    }
    Ok(())
}

/// `--profile` の値を、書く操作・読む操作の対象にする。`default` はトップレベル（`None`）。
pub fn profile_target(flag: Option<&str>) -> Result<Option<&str>> {
    match flag {
        None | Some(DEFAULT_PROFILE) => Ok(None),
        Some(name) => check_profile_name(name).map(|()| Some(name)),
    }
}

/// 書く操作の出力に添える対象の名前。トップレベルは `default`。
pub fn profile_label(profile: Option<&str>) -> &str {
    profile.unwrap_or(DEFAULT_PROFILE)
}

fn profile_missing_for_write(name: &str, path: &Path) -> CliError {
    CliError::validation(format!(
        "Profile \"{name}\" is not defined in {}. Create it with `task config set --profile {name} api_url <url>`.",
        path.display()
    ))
}

impl TaskConfig {
    /// 対象（`None` はトップレベル）のキーの値。名前が無ければ終了コード 2。
    pub fn value(&self, profile: Option<&str>, key: &str, path: &Path) -> Result<Option<&String>> {
        match profile {
            None => Ok(self.get(key)),
            Some(name) => self
                .profiles
                .get(name)
                .map(|profile| profile.get(key))
                .ok_or_else(|| profile_missing_for_write(name, path)),
        }
    }

    /// 対象のキーを書き換える。`create` が偽なら、無い名前は終了コード 2（作らない）。
    /// 対象以外（トップレベルや他のプロファイル）は触らない。
    pub fn set_value(
        &mut self,
        profile: Option<&str>,
        key: &str,
        value: Option<String>,
        create: bool,
        path: &Path,
    ) -> Result<()> {
        let Some(name) = profile else {
            self.set(key, value);
            return Ok(());
        };
        if create {
            check_profile_name(name)?;
            self.profiles
                .entry(name.to_string())
                .or_default()
                .set(key, value);
            return Ok(());
        }
        self.profiles
            .get_mut(name)
            .ok_or_else(|| profile_missing_for_write(name, path))?
            .set(key, value);
        Ok(())
    }

    /// 名前付きプロファイルの名前（予約名 `default` は除く）。
    pub fn profile_names(&self) -> Vec<&str> {
        self.profiles
            .keys()
            .map(String::as_str)
            .filter(|name| *name != DEFAULT_PROFILE)
            .collect()
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        match key {
            "api_url" => self.api_url.as_ref(),
            "token" => self.token.as_ref(),
            "tenant_id" => self.tenant_id.as_ref(),
            _ => None,
        }
    }

    pub fn set(&mut self, key: &str, value: Option<String>) {
        match key {
            "api_url" => self.api_url = value,
            "token" => self.token = value,
            "tenant_id" => self.tenant_id = value,
            _ => {}
        }
    }
}

/// 設定ファイルの置き場所。テストが一時ディレクトリを指せるよう値で持つ。
#[derive(Debug, Clone)]
pub struct ConfigStore {
    dir: PathBuf,
}

impl ConfigStore {
    pub fn from_home(home: &Path) -> Self {
        Self {
            dir: home.join(".config").join("task"),
        }
    }

    pub fn discover() -> Result<Self> {
        Self::discover_with(|key| std::env::var(key).ok(), std::env::home_dir)
    }

    /// ホームディレクトリを決める。`from_process` が振り分けより先に呼ぶので、
    /// ここで失敗すると `config set` すら実行できない。
    fn discover_with(
        env: impl Fn(&str) -> Option<String>,
        from_os: impl FnOnce() -> Option<PathBuf>,
    ) -> Result<Self> {
        home_from_env(env)
            .or_else(from_os)
            .map(|home| Self::from_home(&home))
            .ok_or_else(|| {
                CliError::validation(
                    "Cannot locate the home directory (set HOME, or USERPROFILE on Windows)",
                )
            })
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join("config.yaml")
    }

    /// 新しい版を最後に確かめた時刻の置き場所。
    pub fn update_state_path(&self) -> PathBuf {
        self.dir.join("update-check.json")
    }

    pub fn load(&self) -> Result<TaskConfig> {
        let path = self.path();
        if !path.exists() {
            return Ok(TaskConfig::default());
        }
        let raw = std::fs::read_to_string(&path)
            .map_err(|err| CliError::new(format!("Cannot read {}: {err}", path.display())))?;
        // 空ファイルは YAML の null になる。設定が無い状態として扱う。
        serde_yaml_ng::from_str::<Option<TaskConfig>>(&raw)
            .map(Option::unwrap_or_default)
            .map_err(|err| CliError::validation(format!("Cannot parse {}: {err}", path.display())))
    }

    pub fn save(&self, config: &TaskConfig) -> Result<()> {
        create_private_dir(&self.dir)
            .map_err(|err| CliError::new(format!("Cannot create {}: {err}", self.dir.display())))?;
        let path = self.path();
        let body = serde_yaml_ng::to_string(config)
            .map_err(|err| CliError::new(format!("Cannot serialize config: {err}")))?;
        write_private(&path, &body)?;
        restrict_permissions(&path)
    }
}

/// ホームディレクトリを環境変数から引く。
///
/// Windows の PowerShell や cmd では `HOME` が無く `USERPROFILE` だけがあることが多い。
/// `HOME` しか見ないと、設定を作る `config set` も含めて全コマンドが起動できなくなる。
fn home_from_env(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    home_from_env_ordered(env, cfg!(windows))
}

/// 上と同じだが、優先順位を引数で選ぶ。実際の OS と切り離してどちらも試せるようにする。
///
/// 順番は移植元の Node（libuv の `uv_os_homedir`）に合わせる。Windows は
/// `USERPROFILE` が先で、POSIX は `HOME` が先。逆にすると、Git Bash や MSYS のように
/// 両方が別の値で設定されている Windows で、旧 CLI が保存したトークンを見失う。
/// 空文字はホームとして使えないので無いものとして扱う。
fn home_from_env_ordered(env: impl Fn(&str) -> Option<String>, windows: bool) -> Option<PathBuf> {
    let lookup = |key: &str| env(key).filter(|value| !value.is_empty());

    if !windows {
        // POSIX の Node は HOME だけを見て、無ければ passwd へ落ちる（呼び出し側の
        // `from_os` がその役)。Windows 用の変数はここでは見ない
        return lookup("HOME").map(PathBuf::from);
    }
    if let Some(profile) = lookup("USERPROFILE") {
        return Some(PathBuf::from(profile));
    }
    if let (Some(drive), Some(path)) = (lookup("HOMEDRIVE"), lookup("HOMEPATH")) {
        return Some(PathBuf::from(format!("{drive}{path}")));
    }
    // USERPROFILE が無い Windows は稀だが、MSYS 系が置いた HOME があれば拾う
    lookup("HOME").map(PathBuf::from)
}

/// 設定ディレクトリを（無ければ）所有者だけが入れる権限（0700）で作る。
/// 既にあるディレクトリの権限は変えない。
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// 新規作成の時点で所有者だけが読めるようにして書く。
///
/// `fs::write` が作るファイルは `0o666 & !umask`（多くの環境で 0644）なので、
/// あとから chmod するまでの間、同じマシンの他の利用者にトークンが読める。
/// 作成時のモードで塞ぐ。
#[cfg(unix)]
fn write_private(path: &Path, body: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|err| CliError::new(format!("Cannot write {}: {err}", path.display())))?;
    file.write_all(body.as_bytes())
        .map_err(|err| CliError::new(format!("Cannot write {}: {err}", path.display())))
}

#[cfg(not(unix))]
fn write_private(path: &Path, body: &str) -> Result<()> {
    std::fs::write(path, body)
        .map_err(|err| CliError::new(format!("Cannot write {}: {err}", path.display())))
}

/// 既にあるファイルの権限を直す。
///
/// 作成時のモードは新規作成にしか効かないので、緩い権限で作られた既存の設定を
/// 引き継いだときのために、書いたあとにも付け直す。
#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|err| CliError::new(format!("Cannot chmod {}: {err}", path.display())))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

/// tenant_id をどこで明示したか。403 の案内で、どれを外せばよいかを出し分ける。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantSource {
    Env,
    Config,
}

impl TenantSource {
    /// 403 だけでは束縛の食い違いと断定できないので、条件付きの文にする。
    /// env は config に勝つので、env 由来に `config unset` を勧めても解消しない
    /// `profile` は接続を取った名前付きプロファイル（トップレベルなら `None`）。
    pub fn forbidden_hint(self, profile: Option<&str>) -> String {
        match (self, profile) {
            (Self::Env, _) => {
                "tenant_id is set by TASK_TENANT; if the token cannot be used in that tenant, unset it".to_string()
            }
            (Self::Config, None) => {
                "tenant_id is set in config; if the token cannot be used in that tenant, run `task config unset tenant_id`".to_string()
            }
            (Self::Config, Some(name)) => format!(
                "tenant_id is set in profile \"{name}\" of config; if the token cannot be used in that tenant, run `task config unset --profile {name} tenant_id`"
            ),
        }
    }
}

/// API を呼ぶのに必要な設定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub api_url: String,
    pub token: String,
    /// 明示された tenant_id と由来。無ければトークンの束縛先を `/me` から引く
    pub tenant: Option<(String, TenantSource)>,
    /// 接続を取った名前付きプロファイル。トップレベルと両 env は `None`
    pub profile: Option<String>,
}

/// プロファイルをどこで選んだか。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileOrigin {
    Flag,
    Env,
    RepoFile(PathBuf),
}

impl std::fmt::Display for ProfileOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Flag => f.write_str("--profile"),
            Self::Env => f.write_str("TASK_PROFILE"),
            Self::RepoFile(path) => write!(f, "{}", path.display()),
        }
    }
}

/// 選ばれた名前付きプロファイル。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSelection {
    pub name: String,
    pub origin: ProfileOrigin,
}

/// `--profile` > `TASK_PROFILE` > `.koyori-task.yaml` の `profile` の順で 1 つ選ぶ。
///
/// 上位があれば下位は見ない。空文字の `TASK_PROFILE` は未設定。`default` は
/// トップレベルの明示なので `None` を返し、下位も見ない。
pub fn select_profile(
    flag: Option<&str>,
    env: impl Fn(&str) -> Option<String>,
    repo: Option<(&str, &Path)>,
) -> Option<ProfileSelection> {
    let (name, origin) = if let Some(name) = flag {
        (name.to_string(), ProfileOrigin::Flag)
    } else if let Some(name) = env("TASK_PROFILE").filter(|value| !value.is_empty()) {
        (name, ProfileOrigin::Env)
    } else if let Some((name, path)) = repo {
        (
            name.to_string(),
            ProfileOrigin::RepoFile(path.to_path_buf()),
        )
    } else {
        return None;
    };
    (name != DEFAULT_PROFILE).then_some(ProfileSelection { name, origin })
}

/// 接続の設定が揃っているか。`task init` が、足りない分を聞いてよいかを決めるのに使う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Ready(RuntimeConfig),
    /// `task init` が書いてよい層（トップレベルか `--profile` のプロファイル）で、
    /// `api_url` か `token` が足りない。`profile` が未定義のときも含む
    NeedsSetup {
        /// 書く対象。`None` はトップレベル
        profile: Option<String>,
        api_url: Option<String>,
        has_token: bool,
    },
}

/// 足りない設定と、`resolve_runtime_with` が返すエラー。
struct Shortfall {
    profile: Option<String>,
    api_url: Option<String>,
    has_token: bool,
    error: CliError,
}

/// 環境変数を設定ファイルより優先して解決する。
///
/// `TASK_API_URL` と `TASK_TOKEN` は組で扱う。項目ごとに上書きすると、config の
/// トークンが env の別インスタンスへ送られる。両方が env にあれば config は読まない
/// （プロファイルの指定も見ない）。env が無ければ、選ばれたプロファイルか、無ければ
/// トップレベルから `api_url` / `token` / `tenant_id` を丸ごと取る。層をまたいで補わない。
/// `TASK_TENANT` は送り先を変えないので単独で効く。
///
/// `Ok(Err(..))` は「`task init` が書いてよい層で足りない」。それ以外の不足・不存在・
/// 片側 env は `Err`（`init` も聞かずに止まる）。
fn evaluate(
    store: &ConfigStore,
    env: impl Fn(&str) -> Option<String>,
    selection: Option<&ProfileSelection>,
) -> Result<std::result::Result<RuntimeConfig, Box<Shortfall>>> {
    // 空文字は未設定として扱う（`TASK_TOKEN=` で外したつもりの値を拾わない）
    let var = |key: &str| env(key).filter(|value| !value.is_empty());
    let mut profile = None;
    let (api_url, token, file_tenant) = match (var("TASK_API_URL"), var("TASK_TOKEN")) {
        // config は tenant_id も含めて読まない。env のトークンは別テナントのことが多く、
        // config の tenant_id を残すと毎回 403 になる。壊れた config でも止めない
        (Some(api_url), Some(token)) => (api_url, token, None),
        (None, None) => {
            let file = store.load()?;
            match selection {
                None => match (file.api_url, file.token) {
                    (Some(api_url), Some(token)) => (api_url, token, file.tenant_id),
                    (api_url, token) => {
                        let missing: Vec<&str> = [
                            api_url.is_none().then_some("api_url (TASK_API_URL)"),
                            token.is_none().then_some("token (TASK_TOKEN)"),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        return Ok(Err(Box::new(Shortfall {
                            profile: None,
                            has_token: token.is_some(),
                            api_url,
                            error: CliError::validation(format!(
                                "Missing required configuration: {}. Set both TASK_API_URL and TASK_TOKEN, or both api_url and token in {}. Run `task init` to set them up.",
                                missing.join(", "),
                                store.path().display(),
                            )),
                        })));
                    }
                },
                Some(selection) => {
                    let name = &selection.name;
                    // 聞いて書いてよいのは `--profile` で選んだときだけ。`TASK_PROFILE` と
                    // `.koyori-task.yaml` 由来の選択は、書く操作の対象にならない
                    let shortfall = |api_url: Option<String>, has_token: bool, error: CliError| {
                        if selection.origin == ProfileOrigin::Flag {
                            Ok(Err(Box::new(Shortfall {
                                profile: Some(name.clone()),
                                api_url,
                                has_token,
                                error,
                            })))
                        } else {
                            Err(error)
                        }
                    };
                    let Some(found) = file.profiles.get(name) else {
                        return shortfall(
                            None,
                            false,
                            CliError::validation(format!(
                                "Profile \"{name}\" (from {}) is not defined in {}. Create it with `task config set --profile {name} api_url <url>` and `task auth token --profile {name}`. Or run `task --profile {name} init`.",
                                selection.origin,
                                store.path().display(),
                            )),
                        );
                    };
                    let (Some(api_url), Some(token)) = (&found.api_url, &found.token) else {
                        let missing: Vec<&str> = [
                            found.api_url.is_none().then_some("api_url"),
                            found.token.is_none().then_some("token"),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        return shortfall(
                            found.api_url.clone(),
                            found.token.is_some(),
                            CliError::validation(format!(
                                "Profile \"{name}\" (from {}) is missing {} in {}. Set them with `task config set --profile {name} api_url <url>` and `task auth token --profile {name}`; they are not taken from the top level. Or run `task --profile {name} init`.",
                                selection.origin,
                                missing.join(", "),
                                store.path().display(),
                            )),
                        );
                    };
                    profile = Some(name.clone());
                    (api_url.clone(), token.clone(), found.tenant_id.clone())
                }
            }
        }
        (api_url, _) => {
            let (set, unset) = if api_url.is_some() {
                ("TASK_API_URL", "TASK_TOKEN")
            } else {
                ("TASK_TOKEN", "TASK_API_URL")
            };
            return Err(CliError::validation(format!(
                "{set} is set without {unset}; the API URL and token must come from the same place. \
                 Set {unset} too, or unset {set} and keep both api_url and token in {}.",
                store.path().display(),
            )));
        }
    };
    let tenant = match var("TASK_TENANT") {
        Some(id) => Some((id, TenantSource::Env)),
        None => file_tenant.map(|id| (id, TenantSource::Config)),
    };

    Ok(Ok(RuntimeConfig {
        // 末尾の `/` を落とさないとパスを繋いだ URL が `//v1/...` になる
        api_url: api_url.trim_end_matches('/').to_string(),
        token,
        tenant,
        profile,
    }))
}

/// 接続の設定を解決する。`task init` が書いてよい層で足りないときは、エラーにせず
/// `NeedsSetup` を返す。
pub fn connection_state(
    store: &ConfigStore,
    env: impl Fn(&str) -> Option<String>,
    selection: Option<&ProfileSelection>,
) -> Result<ConnectionState> {
    Ok(match evaluate(store, env, selection)? {
        Ok(runtime) => ConnectionState::Ready(runtime),
        Err(shortfall) => ConnectionState::NeedsSetup {
            profile: shortfall.profile,
            api_url: shortfall.api_url,
            has_token: shortfall.has_token,
        },
    })
}

/// 接続の設定を解決する。足りなければ終了コード 2（規則は `evaluate` のとおり）。
pub fn resolve_runtime_with(
    store: &ConfigStore,
    env: impl Fn(&str) -> Option<String>,
    selection: Option<&ProfileSelection>,
) -> Result<RuntimeConfig> {
    evaluate(store, env, selection)?.map_err(|shortfall| shortfall.error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ConfigStore) {
        let home = tempfile::tempdir().unwrap();
        let store = ConfigStore::from_home(home.path());
        (home, store)
    }

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        }
    }

    fn none_from_os() -> Option<PathBuf> {
        None
    }

    const WINDOWS: bool = true;
    const POSIX: bool = false;

    /// Windows の PowerShell / cmd では HOME が無く USERPROFILE だけがある。
    ///
    /// `discover` は振り分けより先に走るので、ここで落ちると設定を作る
    /// `config set` すら実行できない。
    #[test]
    fn finds_the_home_directory_on_windows_where_only_userprofile_is_set() {
        let home = home_from_env_ordered(env_of(&[("USERPROFILE", "C:\\Users\\yupix")]), WINDOWS);
        assert_eq!(home, Some(PathBuf::from("C:\\Users\\yupix")));
    }

    /// 優先順位は移植元の Node（libuv）と同じにする。
    ///
    /// Windows で両方が別の値だと、HOME を先に見た時点で旧 CLI が保存したトークンを
    /// 見失う。逆に POSIX で USERPROFILE を拾うと、Node が使っていた場所とずれる。
    #[test]
    fn follows_the_platforms_own_precedence_when_both_are_set() {
        let both = [("HOME", "/home/yupix"), ("USERPROFILE", "C:\\Users\\yupix")];

        assert_eq!(
            home_from_env_ordered(env_of(&both), WINDOWS),
            Some(PathBuf::from("C:\\Users\\yupix"))
        );
        assert_eq!(
            home_from_env_ordered(env_of(&both), POSIX),
            Some(PathBuf::from("/home/yupix"))
        );
    }

    /// POSIX では Windows 用の変数を見ない（Node も見ない）。
    #[test]
    fn ignores_the_windows_variables_on_posix() {
        let windows_only = [
            ("USERPROFILE", "C:\\Users\\yupix"),
            ("HOMEDRIVE", "C:"),
            ("HOMEPATH", "\\Users\\yupix"),
        ];
        assert_eq!(home_from_env_ordered(env_of(&windows_only), POSIX), None);
    }

    #[test]
    fn falls_back_to_homedrive_and_homepath_on_windows() {
        let home = home_from_env_ordered(
            env_of(&[("HOMEDRIVE", "C:"), ("HOMEPATH", "\\Users\\yupix")]),
            WINDOWS,
        );
        assert_eq!(home, Some(PathBuf::from("C:\\Users\\yupix")));

        // 片方だけでは組み立てられない
        assert_eq!(
            home_from_env_ordered(env_of(&[("HOMEDRIVE", "C:")]), WINDOWS),
            None
        );
        assert_eq!(
            home_from_env_ordered(env_of(&[("HOMEPATH", "\\Users\\yupix")]), WINDOWS),
            None
        );
    }

    /// 空文字はホームとして使えない。設定がおかしな場所へ落ちるのを避ける。
    #[test]
    fn treats_an_empty_variable_as_unset() {
        assert_eq!(
            home_from_env_ordered(
                env_of(&[("USERPROFILE", ""), ("HOME", "C:\\msys\\home")]),
                WINDOWS
            ),
            Some(PathBuf::from("C:\\msys\\home"))
        );
        assert_eq!(
            home_from_env_ordered(env_of(&[("HOME", ""), ("USERPROFILE", "")]), WINDOWS),
            None
        );
        assert_eq!(home_from_env_ordered(env_of(&[("HOME", "")]), POSIX), None);
    }

    /// 環境変数が無ければ OS に聞く（Unix なら passwd）。
    #[test]
    fn asks_the_operating_system_when_no_variable_is_set() {
        let store =
            ConfigStore::discover_with(env_of(&[]), || Some(PathBuf::from("/var/lib/task")))
                .unwrap();
        assert_eq!(
            store.path(),
            Path::new("/var/lib/task/.config/task/config.yaml")
        );
    }

    #[test]
    fn reports_that_the_home_directory_could_not_be_found() {
        let err = ConfigStore::discover_with(env_of(&[]), none_from_os).unwrap_err();

        assert_eq!(err.exit_code, 2);
        assert!(err.message.contains("USERPROFILE"), "{}", err.message);
    }

    #[test]
    fn reads_and_writes_only_below_the_given_home() {
        let (home, store) = store();
        assert_eq!(store.path(), home.path().join(".config/task/config.yaml"));
        assert_eq!(store.load().unwrap(), TaskConfig::default());

        store
            .save(&TaskConfig {
                api_url: Some("https://task.invalid/".into()),
                token: Some("secret".into()),
                tenant_id: Some("tenant-1".into()),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(store.load().unwrap().token.as_deref(), Some("secret"));
    }

    /// 作成した瞬間から 0600 であること。
    ///
    /// `fs::write` で作ると 0644 で生まれ、chmod が走るまでの間、同じマシンの
    /// 他の利用者にトークンが読める。書き込みだけを直接呼んで、あとからの
    /// chmod に頼らずに閉じていることを確かめる。
    #[cfg(unix)]
    #[test]
    fn creates_the_config_directory_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let (_home, store) = store();
        store.save(&TaskConfig::default()).unwrap();

        let dir = store.path().parent().unwrap().to_path_buf();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn creates_the_token_file_already_restricted_without_relying_on_chmod() {
        use std::os::unix::fs::PermissionsExt;

        let (_home, store) = store();
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();

        write_private(&store.path(), "token: secret\n").unwrap();

        let mode = std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// 緩い権限で作られていた既存の設定も、書き込み後の chmod で直す。
    #[cfg(unix)]
    #[test]
    fn tightens_a_config_file_that_was_already_world_readable() {
        use std::os::unix::fs::PermissionsExt;

        let (_home, store) = store();
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(store.path(), "token: old\n").unwrap();
        std::fs::set_permissions(store.path(), std::fs::Permissions::from_mode(0o644)).unwrap();

        store
            .save(&TaskConfig {
                token: Some("new".into()),
                ..Default::default()
            })
            .unwrap();

        let mode = std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn keeps_the_token_file_readable_only_by_its_owner_across_rewrites() {
        use std::os::unix::fs::PermissionsExt;

        let (_home, store) = store();
        store
            .save(&TaskConfig {
                token: Some("a".into()),
                ..Default::default()
            })
            .unwrap();
        // 上書き時にモード指定が効かない経路を踏ませる（2 回目でも 0600 のままか）
        store
            .save(&TaskConfig {
                token: Some("b".into()),
                ..Default::default()
            })
            .unwrap();

        let mode = std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn treats_an_empty_config_file_as_no_configuration() {
        let (_home, store) = store();
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(store.path(), "").unwrap();

        assert_eq!(store.load().unwrap(), TaskConfig::default());
    }

    #[test]
    fn prefers_environment_values_and_removes_a_trailing_api_slash() {
        let (_home, store) = store();
        store
            .save(&TaskConfig {
                api_url: Some("https://file.invalid".into()),
                token: Some("file-token".into()),
                tenant_id: Some("file-tenant".into()),
                ..Default::default()
            })
            .unwrap();

        let resolved = resolve_runtime_with(
            &store,
            |key| match key {
                "TASK_API_URL" => Some("https://api.invalid/".into()),
                "TASK_TOKEN" => Some("env-token".into()),
                "TASK_TENANT" => Some("env-tenant".into()),
                _ => None,
            },
            None,
        )
        .unwrap();

        assert_eq!(
            resolved,
            RuntimeConfig {
                api_url: "https://api.invalid".into(),
                token: "env-token".into(),
                tenant: Some(("env-tenant".into(), TenantSource::Env)),
                profile: None,
            }
        );
    }

    fn save_complete_file(store: &ConfigStore) {
        store
            .save(&TaskConfig {
                api_url: Some("https://file.invalid".into()),
                token: Some("file-token".into()),
                tenant_id: Some("file-tenant".into()),
                ..Default::default()
            })
            .unwrap();
    }

    /// 片側だけを config で補うと、config のトークンが env の別インスタンスへ飛ぶ。
    /// config が完全でも止める。
    #[test]
    fn rejects_the_url_or_the_token_set_alone_in_the_environment() {
        let (_home, store) = store();
        save_complete_file(&store);

        for (set, unset) in [
            ("TASK_API_URL", "TASK_TOKEN"),
            ("TASK_TOKEN", "TASK_API_URL"),
        ] {
            let err =
                resolve_runtime_with(&store, env_of(&[(set, "env-value")]), None).unwrap_err();

            assert_eq!(err.exit_code, 2, "{set}");
            assert!(
                err.message
                    .starts_with(&format!("{set} is set without {unset};")),
                "{}",
                err.message
            );
            assert!(
                err.message.contains(&store.path().display().to_string()),
                "{}",
                err.message
            );
        }
    }

    /// `TASK_TENANT` は送り先を変えないので、URL とトークンが config でも単独で効く。
    #[test]
    fn takes_the_tenant_alone_from_the_environment() {
        let (_home, store) = store();
        save_complete_file(&store);

        let resolved =
            resolve_runtime_with(&store, env_of(&[("TASK_TENANT", "env-tenant")]), None).unwrap();

        assert_eq!(
            resolved,
            RuntimeConfig {
                api_url: "https://file.invalid".into(),
                token: "file-token".into(),
                tenant: Some(("env-tenant".into(), TenantSource::Env)),
                profile: None,
            }
        );
    }

    #[test]
    fn treats_an_empty_variable_as_unset_in_the_runtime_settings() {
        let (_home, store) = store();
        save_complete_file(&store);

        let resolved = resolve_runtime_with(
            &store,
            env_of(&[
                ("TASK_API_URL", ""),
                ("TASK_TOKEN", ""),
                ("TASK_TENANT", ""),
            ]),
            None,
        )
        .unwrap();
        assert_eq!(
            resolved,
            RuntimeConfig {
                api_url: "https://file.invalid".into(),
                token: "file-token".into(),
                tenant: Some(("file-tenant".into(), TenantSource::Config)),
                profile: None,
            }
        );

        // 片方が空なら、もう片方だけが渡された状態
        let err = resolve_runtime_with(
            &store,
            env_of(&[("TASK_API_URL", "https://api.invalid"), ("TASK_TOKEN", "")]),
            None,
        )
        .unwrap_err();
        assert_eq!(err.exit_code, 2);
        assert!(
            err.message
                .starts_with("TASK_API_URL is set without TASK_TOKEN;"),
            "{}",
            err.message
        );
    }

    #[test]
    fn names_every_missing_setting_and_exits_two() {
        let (_home, store) = store();
        let err = resolve_runtime_with(&store, |_| None, None).unwrap_err();

        assert_eq!(err.exit_code, 2);
        for expected in ["api_url (TASK_API_URL)", "token (TASK_TOKEN)"] {
            assert!(err.message.contains(expected), "{}", err.message);
        }
        // tenant_id は任意（トークンから引く）なので不足として挙げない
        assert!(!err.message.contains("tenant_id"), "{}", err.message);
    }

    #[test]
    fn resolves_without_tenant_id() {
        let (_home, store) = store();
        store
            .save(&TaskConfig {
                api_url: Some("https://api.invalid".into()),
                token: Some("token-1".into()),
                tenant_id: None,
                ..Default::default()
            })
            .unwrap();

        let resolved = resolve_runtime_with(&store, |_| None, None).unwrap();

        assert_eq!(
            resolved,
            RuntimeConfig {
                api_url: "https://api.invalid".into(),
                token: "token-1".into(),
                tenant: None,
                profile: None,
            }
        );
    }

    #[test]
    fn drops_a_key_from_the_file_when_it_is_unset() {
        let (_home, store) = store();
        let mut config = TaskConfig {
            api_url: Some("https://task.invalid".into()),
            token: Some("secret".into()),
            tenant_id: None,
            ..Default::default()
        };
        store.save(&config).unwrap();

        config.set("token", None);
        store.save(&config).unwrap();

        let raw = std::fs::read_to_string(store.path()).unwrap();
        assert!(!raw.contains("token"), "{raw}");
        assert_eq!(store.load().unwrap().token, None);
    }

    fn profile(api_url: Option<&str>, token: Option<&str>, tenant_id: Option<&str>) -> Profile {
        Profile {
            api_url: api_url.map(Into::into),
            token: token.map(Into::into),
            tenant_id: tenant_id.map(Into::into),
        }
    }

    /// トップレベルは A、`work` は B を指す config。
    fn save_with_profiles(store: &ConfigStore, profiles: &[(&str, Profile)]) -> TaskConfig {
        let config = TaskConfig {
            api_url: Some("https://top.invalid".into()),
            token: Some("top-token".into()),
            tenant_id: Some("top-tenant".into()),
            profiles: profiles
                .iter()
                .map(|(name, profile)| (name.to_string(), profile.clone()))
                .collect(),
        };
        store.save(&config).unwrap();
        config
    }

    fn selected(name: &str, origin: ProfileOrigin) -> ProfileSelection {
        ProfileSelection {
            name: name.into(),
            origin,
        }
    }

    #[test]
    fn round_trips_profiles_and_omits_an_empty_map() {
        let (_home, store) = store();
        let config = save_with_profiles(
            &store,
            &[
                ("work", profile(Some("https://b.invalid"), Some("b"), None)),
                ("empty", Profile::default()),
            ],
        );
        assert_eq!(store.load().unwrap(), config);

        store
            .save(&TaskConfig {
                token: Some("t".into()),
                ..Default::default()
            })
            .unwrap();
        let raw = std::fs::read_to_string(store.path()).unwrap();
        assert!(!raw.contains("profiles"), "{raw}");
    }

    #[test]
    fn reads_null_profiles_and_the_old_top_level_only_form() {
        let (_home, store) = store();
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();

        std::fs::write(store.path(), "token: t\nprofiles:\n").unwrap();
        let config = store.load().unwrap();
        assert_eq!(config.token.as_deref(), Some("t"));
        assert!(config.profiles.is_empty());

        std::fs::write(store.path(), "profiles:\n  work:\n").unwrap();
        assert_eq!(store.load().unwrap().profiles["work"], Profile::default());

        std::fs::write(
            store.path(),
            "api_url: https://a.invalid\ntoken: t\ntenant_id: x\n",
        )
        .unwrap();
        let config = store.load().unwrap();
        assert_eq!(config.tenant_id.as_deref(), Some("x"));
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn selects_the_flag_then_the_env_then_the_repository_file() {
        let repo = Path::new("/repo/.koyori-task.yaml");
        let env = env_of(&[("TASK_PROFILE", "env")]);

        assert_eq!(
            select_profile(Some("flag"), &env, Some(("file", repo))),
            Some(selected("flag", ProfileOrigin::Flag))
        );
        assert_eq!(
            select_profile(None, &env, Some(("file", repo))),
            Some(selected("env", ProfileOrigin::Env))
        );
        assert_eq!(
            select_profile(None, env_of(&[("TASK_PROFILE", "")]), Some(("file", repo))),
            Some(selected("file", ProfileOrigin::RepoFile(repo.into())))
        );
        assert_eq!(select_profile(None, env_of(&[]), None), None);
    }

    /// `default` はトップレベルの明示。下位の指定を見ない。
    #[test]
    fn default_selects_the_top_level_and_hides_lower_choices() {
        let repo = Path::new("/repo/.koyori-task.yaml");
        let env = env_of(&[("TASK_PROFILE", "env")]);
        assert_eq!(
            select_profile(Some("default"), &env, Some(("file", repo))),
            None
        );
        assert_eq!(
            select_profile(
                None,
                env_of(&[("TASK_PROFILE", "default")]),
                Some(("file", repo))
            ),
            None
        );
        assert_eq!(
            select_profile(None, env_of(&[]), Some(("default", repo))),
            None
        );
    }

    /// プロファイルを選ぶと接続を丸ごとそこから取り、トップレベルの tenant_id を継がない。
    #[test]
    fn a_selected_profile_supplies_the_whole_connection() {
        let (_home, store) = store();
        save_with_profiles(
            &store,
            &[
                (
                    "work",
                    profile(Some("https://b.invalid/"), Some("b-token"), None),
                ),
                (
                    "pinned",
                    profile(Some("https://c.invalid"), Some("c-token"), Some("c-tenant")),
                ),
            ],
        );

        let resolved = resolve_runtime_with(
            &store,
            env_of(&[]),
            Some(&selected("work", ProfileOrigin::Flag)),
        )
        .unwrap();
        assert_eq!(
            resolved,
            RuntimeConfig {
                api_url: "https://b.invalid".into(),
                token: "b-token".into(),
                tenant: None,
                profile: Some("work".into()),
            }
        );

        let pinned = resolve_runtime_with(
            &store,
            env_of(&[]),
            Some(&selected("pinned", ProfileOrigin::Env)),
        )
        .unwrap();
        assert_eq!(
            pinned.tenant,
            Some(("c-tenant".into(), TenantSource::Config))
        );

        // TASK_TENANT はプロファイルの tenant_id に勝つ
        let env_tenant = resolve_runtime_with(
            &store,
            env_of(&[("TASK_TENANT", "env-tenant")]),
            Some(&selected("pinned", ProfileOrigin::Flag)),
        )
        .unwrap();
        assert_eq!(
            env_tenant.tenant,
            Some(("env-tenant".into(), TenantSource::Env))
        );
        assert_eq!(env_tenant.api_url, "https://c.invalid");
    }

    /// 欠けた値をトップレベルから補わない（トップレベルが完全でも止める）。
    #[test]
    fn an_incomplete_profile_is_not_filled_from_the_top_level() {
        let (_home, store) = store();
        save_with_profiles(
            &store,
            &[
                ("no-token", profile(Some("https://b.invalid"), None, None)),
                ("no-url", profile(None, Some("b-token"), None)),
            ],
        );

        for (name, missing) in [("no-token", "token"), ("no-url", "api_url")] {
            let err = resolve_runtime_with(
                &store,
                env_of(&[]),
                Some(&selected(name, ProfileOrigin::Flag)),
            )
            .unwrap_err();
            assert_eq!(err.exit_code, 2, "{name}");
            assert!(
                err.message.contains(&format!(
                    "Profile \"{name}\" (from --profile) is missing {missing} in"
                )),
                "{}",
                err.message
            );
            assert!(
                err.message.contains("not taken from the top level"),
                "{}",
                err.message
            );
        }
    }

    #[test]
    fn an_undefined_profile_names_where_it_was_chosen() {
        let (_home, store) = store();
        save_with_profiles(&store, &[]);

        for (origin, shown) in [
            (ProfileOrigin::Flag, "--profile".to_string()),
            (ProfileOrigin::Env, "TASK_PROFILE".to_string()),
            (
                ProfileOrigin::RepoFile("/repo/.koyori-task.yaml".into()),
                "/repo/.koyori-task.yaml".to_string(),
            ),
        ] {
            let err = resolve_runtime_with(&store, env_of(&[]), Some(&selected("gone", origin)))
                .unwrap_err();
            assert_eq!(err.exit_code, 2);
            assert!(
                err.message.starts_with(&format!(
                    "Profile \"gone\" (from {shown}) is not defined in {}",
                    store.path().display()
                )),
                "{}",
                err.message
            );
        }
    }

    /// 両 env なら名前の存在も完全性も見ず、config が壊れていても動く。
    #[test]
    fn both_environment_values_ignore_the_profile_choice() {
        let (_home, store) = store();
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(store.path(), "profiles: [broken\n").unwrap();

        let resolved = resolve_runtime_with(
            &store,
            env_of(&[
                ("TASK_API_URL", "https://env.invalid"),
                ("TASK_TOKEN", "env-token"),
            ]),
            Some(&selected("missing", ProfileOrigin::Flag)),
        )
        .unwrap();
        assert_eq!(resolved.api_url, "https://env.invalid");
        assert_eq!(resolved.profile, None);
    }

    /// 片側 env のエラーはプロファイルの選択より先（プロファイルが完全でも止める）。
    #[test]
    fn one_sided_environment_values_fail_before_the_profile_is_used() {
        let (_home, store) = store();
        save_with_profiles(
            &store,
            &[("work", profile(Some("https://b.invalid"), Some("b"), None))],
        );
        let err = resolve_runtime_with(
            &store,
            env_of(&[("TASK_TOKEN", "env-token")]),
            Some(&selected("work", ProfileOrigin::Flag)),
        )
        .unwrap_err();
        assert_eq!(err.exit_code, 2);
        assert!(
            err.message
                .starts_with("TASK_TOKEN is set without TASK_API_URL;"),
            "{}",
            err.message
        );
    }

    #[test]
    fn set_value_touches_only_its_target() {
        let path = Path::new("/home/me/.config/task/config.yaml");
        let original = TaskConfig {
            api_url: Some("https://top.invalid".into()),
            token: Some("top-token".into()),
            tenant_id: None,
            profiles: [
                (
                    "work".to_string(),
                    profile(Some("https://b.invalid"), Some("b"), None),
                ),
                (
                    "other".to_string(),
                    profile(Some("https://c.invalid"), Some("c"), None),
                ),
            ]
            .into(),
        };

        let mut config = original.clone();
        config
            .set_value(Some("work"), "token", None, false, path)
            .unwrap();
        assert_eq!(config.token, original.token);
        assert_eq!(config.profiles["other"], original.profiles["other"]);
        assert_eq!(config.profiles["work"].token, None);
        assert_eq!(
            config.profiles["work"].api_url,
            original.profiles["work"].api_url
        );

        let mut config = original.clone();
        config.set_value(None, "token", None, false, path).unwrap();
        assert_eq!(config.token, None);
        assert_eq!(config.profiles, original.profiles);

        let mut config = original.clone();
        let err = config
            .set_value(Some("new"), "token", Some("x".into()), false, path)
            .unwrap_err();
        assert_eq!(err.exit_code, 2);
        assert!(
            err.message.contains("Profile \"new\" is not defined"),
            "{}",
            err.message
        );
        assert_eq!(config, original);

        config
            .set_value(
                Some("new"),
                "api_url",
                Some("https://d.invalid".into()),
                true,
                path,
            )
            .unwrap();
        assert_eq!(
            config.profiles["new"],
            profile(Some("https://d.invalid"), None, None)
        );

        assert_eq!(
            config
                .value(Some("work"), "api_url", path)
                .unwrap()
                .map(String::as_str),
            Some("https://b.invalid")
        );
        assert_eq!(
            config
                .value(Some("missing"), "api_url", path)
                .unwrap_err()
                .exit_code,
            2
        );
    }

    #[test]
    fn checks_profile_names() {
        let long = "a".repeat(64);
        for name in ["work-1_a", long.as_str(), "A"] {
            assert!(check_profile_name(name).is_ok(), "{name}");
        }
        let too_long = "a".repeat(65);
        for name in ["", " ", "a b", "a/b", "é", too_long.as_str(), "default"] {
            let err = check_profile_name(name).unwrap_err();
            assert_eq!(err.exit_code, 2, "{name:?}");
        }
        // `--profile default` はトップレベル。作るのではなく対象を示す
        assert_eq!(profile_target(Some("default")).unwrap(), None);
        assert_eq!(profile_target(Some("work")).unwrap(), Some("work"));
        assert_eq!(profile_target(Some("a/b")).unwrap_err().exit_code, 2);
    }

    #[test]
    fn the_forbidden_hint_names_the_profile_to_unset() {
        assert_eq!(
            TenantSource::Config.forbidden_hint(None),
            "tenant_id is set in config; if the token cannot be used in that tenant, run `task config unset tenant_id`"
        );
        assert!(
            TenantSource::Config
                .forbidden_hint(Some("work"))
                .ends_with("run `task config unset --profile work tenant_id`")
        );
        assert_eq!(
            TenantSource::Env.forbidden_hint(Some("work")),
            TenantSource::Env.forbidden_hint(None)
        );
    }

    /// `task init` が聞いて書いてよいのは、トップレベルと `--profile` の層だけ。
    #[test]
    fn needs_setup_only_where_init_may_write() {
        let (_home, store) = store();
        let state = |env: &[(&str, &str)], selection: Option<ProfileSelection>| {
            connection_state(&store, env_of(env), selection.as_ref())
        };
        let needs = |profile: Option<&str>, api_url: Option<&str>, has_token: bool| {
            ConnectionState::NeedsSetup {
                profile: profile.map(Into::into),
                api_url: api_url.map(Into::into),
                has_token,
            }
        };

        // 設定なし
        assert_eq!(state(&[], None).unwrap(), needs(None, None, false));
        // `--profile` の未定義の名前
        assert_eq!(
            state(&[], Some(selected("work", ProfileOrigin::Flag))).unwrap(),
            needs(Some("work"), None, false)
        );

        store
            .save(&TaskConfig {
                api_url: Some("https://top.invalid".into()),
                profiles: [("work".to_string(), profile(None, Some("work-token"), None))].into(),
                ..Default::default()
            })
            .unwrap();
        // 片方だけある層は、ある方をそのまま返す
        assert_eq!(
            state(&[], None).unwrap(),
            needs(None, Some("https://top.invalid"), false)
        );
        assert_eq!(
            state(&[], Some(selected("work", ProfileOrigin::Flag))).unwrap(),
            needs(Some("work"), None, true)
        );

        // `TASK_PROFILE` と `.koyori-task.yaml` 由来の不足・未定義は、聞かずに止まる
        for origin in [
            ProfileOrigin::Env,
            ProfileOrigin::RepoFile("/repo/.koyori-task.yaml".into()),
        ] {
            for name in ["work", "gone"] {
                let err = state(&[], Some(selected(name, origin.clone()))).unwrap_err();
                assert_eq!(err.exit_code, 2, "{origin:?} {name}");
            }
        }
        // 片側 env も止まる
        for env in [
            &[("TASK_API_URL", "https://env.invalid")][..],
            &[("TASK_TOKEN", "env-token")],
        ] {
            let err = state(env, None).unwrap_err();
            assert_eq!(err.exit_code, 2);
            assert!(err.message.contains("is set without"), "{}", err.message);
        }
        // env の組は、config が足りなくても揃っている
        let ready = state(
            &[
                ("TASK_API_URL", "https://env.invalid/"),
                ("TASK_TOKEN", "env-token"),
            ],
            Some(selected("gone", ProfileOrigin::Flag)),
        )
        .unwrap();
        assert_eq!(
            ready,
            ConnectionState::Ready(RuntimeConfig {
                api_url: "https://env.invalid".into(),
                token: "env-token".into(),
                tenant: None,
                profile: None,
            })
        );
    }

    /// 不足の文は、末尾に `task init` の案内が付くだけで変わらない。
    #[test]
    fn missing_settings_point_at_task_init() {
        let (_home, store) = store();
        let path = store.path();
        let err = resolve_runtime_with(&store, env_of(&[]), None).unwrap_err();
        assert_eq!(err.exit_code, 2);
        assert_eq!(
            err.message,
            format!(
                "Missing required configuration: api_url (TASK_API_URL), token (TASK_TOKEN). Set both TASK_API_URL and TASK_TOKEN, or both api_url and token in {}. Run `task init` to set them up.",
                path.display()
            )
        );

        let err = resolve_runtime_with(
            &store,
            env_of(&[]),
            Some(&selected("work", ProfileOrigin::Env)),
        )
        .unwrap_err();
        assert_eq!(
            err.message,
            format!(
                "Profile \"work\" (from TASK_PROFILE) is not defined in {}. Create it with `task config set --profile work api_url <url>` and `task auth token --profile work`. Or run `task --profile work init`.",
                path.display()
            )
        );

        store
            .save(&TaskConfig {
                profiles: [(
                    "work".to_string(),
                    profile(Some("https://b.invalid"), None, None),
                )]
                .into(),
                ..Default::default()
            })
            .unwrap();
        let err = resolve_runtime_with(
            &store,
            env_of(&[]),
            Some(&selected("work", ProfileOrigin::Flag)),
        )
        .unwrap_err();
        assert_eq!(
            err.message,
            format!(
                "Profile \"work\" (from --profile) is missing token in {}. Set them with `task config set --profile work api_url <url>` and `task auth token --profile work`; they are not taken from the top level. Or run `task --profile work init`.",
                path.display()
            )
        );
    }
}
