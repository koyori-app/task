//! `task init`。接続の設定（足りない分だけ）と、リポジトリの `.koyori-task.yaml` の作成。
//!
//! 書くのは 2 か所で、どちらも確かめてから 1 回だけ書く。
//! - ホームの設定: 疎通で確かめた `api_url` と、`/me` が 200 を返したトークンを、同じ層へ組で保存する。
//!   既にある値は変えない。
//! - `.koyori-task.yaml`: 探索経路に 1 つも無いときだけ、API で取れたプロジェクトを git の根に新しく作る。
//!
//! 問いと案内は標準エラー、結果は標準出力に出す（`--json` の出力を 1 つの JSON に保つ）。

use std::path::Path;

use payload::personal_tokens::PersonalTokenIdentityResponse;
use payload::projects::ProjectResponse;
use reqwest::Url;
use serde_json::json;

use crate::Context;
use crate::api::{ApiClient, web_base_of};
use crate::cli::InitArgs;
use crate::config::{ConnectionState, RuntimeConfig, profile_label};
use crate::error::{CliError, Result};
use crate::output::{OutputOptions, print};
use crate::prompt::Prompt;
use crate::repo_config::{self, FILE_NAME, RepoFileBody};
use crate::resolve::{
    RESTRICTED_TOKEN_HINT, is_uuid, list_projects, resolve_project, restricted_token_error,
};
use crate::{probe, remote};

/// 疎通を確かめられなかったときの案内。
const PROBE_HINT: &str = "Check the URL and run `task init` again.";

pub async fn run(context: &Context, args: InitArgs, output: OutputOptions) -> Result<i32> {
    // 1. 既存のファイル。あれば（読めなくても、解析できなくても）作らず、接続もしない。
    //    根に作っても、以後に使われるのは最も近いファイルなので意味が無い
    if let Some(dir) = context.repo_dir()
        && let Some(path) = repo_config::locate(dir)?
    {
        show_existing(&path, output);
        return Ok(0);
    }

    // 2. 接続。足りない分だけを聞いて、確かめてから保存する
    //    `used_profile` は、実際に接続を取った名前付きプロファイル（トップレベルと env の組は `None`）
    let (identity, used_profile) = match context.connection_state()? {
        ConnectionState::Ready(runtime) => {
            check_api_url_flag(
                args.api_url.as_deref(),
                &runtime.api_url,
                runtime.profile.as_deref(),
            )?;
            (None, runtime.profile)
        }
        ConnectionState::NeedsSetup {
            profile,
            api_url,
            has_token,
        } => {
            let identity =
                setup_connection(context, &args, profile.clone(), api_url, has_token).await?;
            (Some(identity), profile)
        }
    };

    // 3. トークンの素性（使えるテナントと、プロジェクトの制限）
    let identity = match identity {
        Some(identity) => identity,
        None => {
            context
                .connect_token_only()?
                .get(&["v1", "personal_tokens", "me"], &[])
                .await?
        }
    };
    eprintln!(
        "Token: {} ({}), scopes: {}",
        identity.name,
        identity.username,
        scopes_line(&identity)
    );

    // 4. git の根。外なら接続の設定だけで終える
    let root = match context.repo_dir() {
        Some(dir) => repo_config::git_root(dir)?,
        None => None,
    };
    let Some(root) = root else {
        if output.json {
            print(&json!({ "path": null, "created": false }), output);
        } else {
            println!("Not inside a git repository; skipped {FILE_NAME}.");
        }
        return Ok(0);
    };

    // 5. テナント
    let api = connect_with_tenant(context, &identity).await?;

    // 6. プロジェクト。決まらなければ、ここで止まって何も書かない
    let restricted = identity.allowed_project_ids.is_some();
    let project = choose_project(context, &api, &args, &identity, &root).await?;

    // 7. 書く。制限付きのトークンはキーを引けないので UUID を書く
    let body = RepoFileBody {
        project: if restricted {
            project.id.to_string()
        } else {
            project.key.clone()
        },
        // `--profile` を明示し、そのプロファイルから接続を取ったときだけ書く（`default` は
        // トップレベルなので書かない）。env の組で接続したときは `--profile` を使っていないので、
        // 書くと、確かめた接続と違う接続を指すファイルになる
        profile: context
            .profile_flag()
            .filter(|flag| used_profile.as_deref() == Some(*flag))
            .map(str::to_owned),
    };
    let path = repo_config::write_new(&root, &body)?;
    if output.json {
        print(
            &json!({
                "path": path.display().to_string(),
                "project": body.project,
                "profile": body.profile,
                "created": true,
            }),
            output,
        );
    } else {
        println!("Created {}", path.display());
        print!("{}", body.to_yaml()?);
    }
    Ok(0)
}

fn show_existing(path: &Path, output: OutputOptions) {
    if output.json {
        print(
            &json!({ "path": path.display().to_string(), "created": false }),
            output,
        );
        return;
    }
    println!("{} already exists; left it unchanged.", path.display());
    match std::fs::read_to_string(path) {
        Ok(contents) => print!("{contents}"),
        Err(err) => println!("(cannot read it: {err})"),
    }
}

fn scopes_line(identity: &PersonalTokenIdentityResponse) -> String {
    let scopes: Vec<String> = identity
        .scopes
        .iter()
        .filter_map(|scope| serde_json::to_value(scope).ok())
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    if scopes.is_empty() {
        "(none)".to_string()
    } else {
        scopes.join(", ")
    }
}

fn profile_arg(profile: Option<&str>) -> String {
    profile
        .map(|name| format!(" --profile {name}"))
        .unwrap_or_default()
}

/// `--api-url` は接続が未設定のときにだけ使う。設定済みの値と違えば、黙って無視せずに止める
/// （既にある値は `init` では変えない）。
fn check_api_url_flag(flag: Option<&str>, current: &str, profile: Option<&str>) -> Result<()> {
    let Some(flag) = flag else {
        return Ok(());
    };
    let current = current.trim_end_matches('/');
    if probe::candidates(flag)?
        .iter()
        .any(|candidate| candidate == current)
    {
        return Ok(());
    }
    Err(CliError::validation(format!(
        "api_url is already set to {current}; --api-url is used only when it is not set. Change it with `task config set{} api_url <url>`.",
        profile_arg(profile)
    )))
}

fn ask(prompt: &dyn Prompt, label: &str) -> Result<Option<String>> {
    Ok(prompt.line(label)?.map(|answer| answer.trim().to_string()))
}

/// 足りない `api_url` と `token` を得て、`/me` で確かめてから 1 回で保存する。
/// 失敗（疎通の不成立・401・通信の失敗・不正な応答）では何も保存しない。
async fn setup_connection(
    context: &Context,
    args: &InitArgs,
    profile: Option<String>,
    existing_api_url: Option<String>,
    has_token: bool,
) -> Result<PersonalTokenIdentityResponse> {
    let prompt = context.prompt();
    let store = context.store();
    let path = store.path();
    let target = profile.as_deref();

    // 設定済みの api_url は疎通を確かめ直さない（変えもしない）
    let (api_url, new_api_url) = match existing_api_url {
        Some(current) => {
            check_api_url_flag(args.api_url.as_deref(), &current, target)?;
            (current.trim_end_matches('/').to_string(), false)
        }
        None => {
            let input = match &args.api_url {
                Some(url) => url.clone(),
                None if prompt.interactive() => {
                    ask(prompt, "Instance URL (e.g. https://task.example.com): ")?
                        .filter(|answer| !answer.is_empty())
                        .ok_or_else(|| {
                            CliError::validation("No instance URL given; nothing was saved")
                        })?
                }
                None => {
                    return Err(CliError::validation(format!(
                        "api_url is not set in {} (profile: {}). Pass --api-url <url>; stdin is not a terminal.",
                        path.display(),
                        profile_label(target)
                    )));
                }
            };
            // 認証なしの要求。トークンを聞くのは、行き先が確かめられた後
            let confirmed = probe::probe_api_url(&input, PROBE_HINT).await?;
            eprintln!("API: {confirmed}");
            (confirmed, true)
        }
    };

    let (token, new_token) = if has_token {
        let config = store.load()?;
        let saved = config.value(target, "token", &path)?.cloned();
        let token = saved.ok_or_else(|| {
            CliError::new(format!(
                "The token in {} changed while `task init` was running; run it again",
                path.display()
            ))
        })?;
        (token, false)
    } else {
        match Url::parse(&api_url).ok().and_then(|url| web_base_of(&url)) {
            Some(web) => eprintln!(
                "Create a personal access token at {}/settings/tokens",
                web.as_str().trim_end_matches('/')
            ),
            None => eprintln!(
                "Create a personal access token in the web app (account settings, access tokens)"
            ),
        }
        let token = if prompt.interactive() {
            prompt.secret("Token: ")?
        } else {
            prompt.piped("token")?
        };
        let token = token
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CliError::validation(if prompt.interactive() {
                    "No token given; nothing was saved"
                } else {
                    "No token given; nothing was saved. Pipe it in: `task init --api-url <url> < token.txt`"
                })
            })?;
        (token, true)
    };

    // 確かめた URL にだけトークンを送る。200 が返るまで保存しない
    let api = ApiClient::new(RuntimeConfig {
        api_url: api_url.clone(),
        token: token.clone(),
        tenant: None,
        profile: profile.clone(),
    })?;
    let identity: PersonalTokenIdentityResponse =
        api.get(&["v1", "personal_tokens", "me"], &[]).await?;

    let mut config = store.load()?;
    let mut saved = Vec::new();
    for (key, value, new) in [
        ("api_url", api_url, new_api_url),
        ("token", token, new_token),
    ] {
        if new {
            // `--profile` の名前は `run` の先頭で検証済み。無ければここで作る
            config.set_value(target, key, Some(value), true, &path)?;
            saved.push(key);
        }
    }
    store.save(&config)?;
    eprintln!(
        "Saved {} to {} (profile: {})",
        saved.join(" and "),
        path.display(),
        profile_label(target)
    );
    Ok(identity)
}

/// 行き先のテナントを決めて接続する。
///
/// 明示があるか、束縛が 1 つなら `connect` のまま。束縛が複数で明示が無いときは、端末で、
/// かつ書いてよい層のときだけ選ばせて、その層へ `tenant_id` を保存する。それ以外は
/// `connect` が今のエラー（明示を求める。終了コード 2）で止まる。
async fn connect_with_tenant(
    context: &Context,
    identity: &PersonalTokenIdentityResponse,
) -> Result<ApiClient> {
    let prompt = context.prompt();
    let bound = identity.bound_tenant_ids();
    let explicit = matches!(
        context.connection_state()?,
        ConnectionState::Ready(runtime) if runtime.tenant.is_some()
    );
    if explicit || bound.len() <= 1 || !prompt.interactive() || !context.connection_is_writable() {
        return context.connect().await;
    }

    // 名前は補足。`read:tenant` が無ければ（403）ID だけで選ばせる
    let names = match context.connect_token_only()?.list_tenants().await {
        Ok(tenants) => tenants,
        Err(err) if err.exit_code == 4 => Vec::new(),
        Err(err) => return Err(err),
    };
    eprintln!("This token can be used in {} tenants:", bound.len());
    for (index, id) in bound.iter().enumerate() {
        match names.iter().find(|tenant| tenant.id == *id) {
            Some(tenant) => eprintln!(
                "  {}  {}  {}  ({id})",
                index + 1,
                tenant.display_id,
                tenant.name
            ),
            None => eprintln!("  {}  {id}", index + 1),
        }
    }
    let answer = ask(prompt, &format!("Tenant [1-{}]: ", bound.len()))?;
    let picked = answer
        .and_then(|answer| answer.parse::<usize>().ok())
        .filter(|number| (1..=bound.len()).contains(number))
        .ok_or_else(|| {
            CliError::validation(format!(
                "No tenant chosen: expected a number from 1 to {}; nothing was written",
                bound.len()
            ))
        })?;

    let store = context.store();
    let path = store.path();
    let target = context.profile_flag();
    let mut config = store.load()?;
    // ID で保存する（名前は変えられるうえ、引き直すたびに `read:tenant` の一覧が要る）
    config.set_value(
        target,
        "tenant_id",
        Some(bound[picked - 1].to_string()),
        false,
        &path,
    )?;
    store.save(&config)?;
    eprintln!(
        "Saved tenant_id to {} (profile: {})",
        path.display(),
        profile_label(target)
    );
    context.connect().await
}

fn linked_label(project: &ProjectResponse) -> Option<String> {
    project.repository.as_ref().map(|repository| {
        repository
            .html_url
            .strip_prefix("https://")
            .unwrap_or(&repository.html_url)
            .to_string()
    })
}

/// 書くプロジェクトを 1 つに決める。どの経路でも、API で取れたものだけを返す。
async fn choose_project(
    context: &Context,
    api: &ApiClient,
    args: &InitArgs,
    identity: &PersonalTokenIdentityResponse,
    root: &Path,
) -> Result<ProjectResponse> {
    let prompt = context.prompt();
    let allowed = identity.allowed_project_ids.as_deref();

    if let Some(project) = &args.project {
        // 制限付きのトークンは一覧を読めないので、キーは引けない（一覧を叩く前に止める）
        if allowed.is_some() && !is_uuid(project) {
            return Err(restricted_token_error());
        }
        return resolve_project(api, project).await;
    }

    if let Some(allowed) = allowed {
        eprintln!("{RESTRICTED_TOKEN_HINT}");
        eprintln!("Projects this token can use:");
        for id in allowed {
            eprintln!("  {id}");
        }
        if !prompt.interactive() {
            return Err(CliError::validation(
                "Pass --project <UUID>: this token is limited to specific projects, and stdin is not a terminal",
            ));
        }
        let id = ask(prompt, "Project UUID: ")?
            .filter(|answer| is_uuid(answer))
            .ok_or_else(|| {
                CliError::validation(
                    "No project chosen: expected a project UUID; nothing was written",
                )
            })?;
        return resolve_project(api, &id).await;
    }

    let mut projects = list_projects(api).await.map_err(|mut err| {
        if err.exit_code == 4 {
            err.message
                .push_str(" Choosing a project needs the read:project scope.");
        }
        err
    })?;
    if projects.is_empty() {
        return Err(CliError::validation(
            "No projects are visible to this token; nothing was written",
        ));
    }
    projects.sort_by(|a, b| a.key.cmp(&b.key));

    // origin が github.com でなければ（他のホスト・origin 無し・git 無し）一致なしとして扱う
    let origin = remote::origin_url(root).and_then(|url| remote::parse_github_remote(&url));
    let matched: Vec<usize> = projects
        .iter()
        .enumerate()
        .filter(|(_, project)| match (&origin, &project.repository) {
            (Some(origin), Some(repository)) => remote::matches(origin, repository),
            _ => false,
        })
        .map(|(index, _)| index)
        .collect();

    if let [only] = matched[..] {
        let project = &projects[only];
        if !args.yes {
            let linked = linked_label(project).unwrap_or_default();
            if !prompt.interactive() {
                return Err(CliError::validation(format!(
                    "Project {} is linked to {linked}. Pass --yes to use it, or --project <key or UUID>; stdin is not a terminal.",
                    project.key
                )));
            }
            let answer = ask(
                prompt,
                &format!(
                    "Use project {} ({}) — linked to {linked}? [Y/n] ",
                    project.key, project.name
                ),
            )?
            .map(|answer| answer.to_ascii_lowercase());
            if !matches!(answer.as_deref(), Some("" | "y" | "yes")) {
                return Err(CliError::validation(
                    "No project chosen; nothing was written. Pass --project <key or UUID> to choose another.",
                ));
            }
        }
        return Ok(projects.swap_remove(only));
    }

    let found = format!("{} projects are linked to the origin remote", matched.len());
    if args.yes {
        return Err(CliError::validation(format!(
            "--yes needs exactly one project linked to the origin remote, but {found}. Pass --project <key or UUID>."
        )));
    }
    if !prompt.interactive() {
        return Err(CliError::validation(format!(
            "Pass --project <key or UUID>: {found}, and stdin is not a terminal"
        )));
    }

    let width = projects.len().to_string().len();
    for (index, project) in projects.iter().enumerate() {
        let mark = if matched.contains(&index) { "*" } else { " " };
        let linked = linked_label(project)
            .map(|label| format!("  [{label}]"))
            .unwrap_or_default();
        eprintln!(
            "{mark} {:>width$}  {}  {}{linked}",
            index + 1,
            project.key,
            project.name
        );
    }
    let answer = ask(prompt, "Project (number or key): ")?.unwrap_or_default();
    let picked = match answer.parse::<usize>() {
        Ok(number) => (1..=projects.len()).contains(&number).then(|| number - 1),
        Err(_) => projects
            .iter()
            .position(|project| project.key.eq_ignore_ascii_case(&answer)),
    };
    let picked = picked.ok_or_else(|| {
        CliError::validation(format!(
            "No project chosen: expected a number from 1 to {} or a project key; nothing was written",
            projects.len()
        ))
    })?;
    Ok(projects.swap_remove(picked))
}
