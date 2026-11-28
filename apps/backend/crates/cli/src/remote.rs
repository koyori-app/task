//! git の `origin` と、プロジェクトの連携先（GitHub）の照合。
//!
//! `task init` が候補を出すのにだけ使う。`github.com` 以外のホストと解釈できない形は
//! 「一致なし」で、エラーにしない。owner / name だけで比べると、別のホストの同名の
//! リポジトリを GitHub の連携と取り違える。

use std::path::Path;

use payload::forge::TaskForgeRepository;
use reqwest::Url;

/// `github.com` のリポジトリ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubRepo {
    pub owner: String,
    pub name: String,
}

const SCP_PREFIX: &str = "git@github.com:";

fn is_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// `<owner>/<name>`（末尾の `/` 1 つと `.git` 1 つは落とす）。ちょうど 2 要素でなければ `None`。
fn owner_and_name(path: &str) -> Option<GithubRepo> {
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    (is_name(owner) && is_name(name)).then(|| GithubRepo {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

/// `https://github.com/<o>/<n>(.git)` と `git@github.com:<o>/<n>(.git)` だけを読む。
pub fn parse_github_remote(url: &str) -> Option<GithubRepo> {
    if let Some(rest) = url.strip_prefix(SCP_PREFIX) {
        return owner_and_name(rest);
    }
    // ホストは文字列の前方一致ではなく、URL として解釈して比べる
    // （`https://github.com@evil.example/o/n` の host は `evil.example`）
    let parsed = Url::parse(url).ok()?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("github.com")
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    owner_and_name(parsed.path().strip_prefix('/')?)
}

/// プロジェクトの連携先が、その GitHub のリポジトリか。owner / name は大文字小文字を区別しない。
pub fn matches(repo: &GithubRepo, candidate: &TaskForgeRepository) -> bool {
    candidate.host == "github"
        && candidate.host_url == "https://github.com"
        && candidate.repo_owner.eq_ignore_ascii_case(&repo.owner)
        && candidate.repo_name.eq_ignore_ascii_case(&repo.name)
}

/// `origin` の URL。git が無い・origin が無い・失敗は `None`。シェルは通さない。
pub fn origin_url(git_root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(git_root)
        .args(["remote", "get-url", "origin"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!url.is_empty()).then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(owner: &str, name: &str) -> GithubRepo {
        GithubRepo {
            owner: owner.into(),
            name: name.into(),
        }
    }

    #[test]
    fn reads_the_https_and_scp_forms_of_github_com() {
        for url in [
            "https://github.com/o/n",
            "https://github.com/o/n.git",
            "https://github.com/o/n/",
            "https://user:pw@github.com/o/n.git",
            "git@github.com:o/n.git",
            "git@github.com:o/n",
        ] {
            assert_eq!(parse_github_remote(url), Some(repo("o", "n")), "{url}");
        }
        assert_eq!(
            parse_github_remote("https://github.com/koyori-app/task.git"),
            Some(repo("koyori-app", "task"))
        );
    }

    #[test]
    fn refuses_other_hosts_and_shapes() {
        for url in [
            "https://gitlab.com/o/n",
            "https://github.com.evil.example/o/n",
            "https://github.com@evil.example/o/n",
            "https://evil.example/github.com/o/n",
            "https://www.github.com/o/n",
            "https://github.com:8443/o/n",
            "http://github.com/o/n",
            "ssh://git@github.com/o/n",
            "git://github.com/o/n",
            "git@github.com.evil.example:o/n",
            "git@gitlab.com:o/n",
            "https://github.com/o",
            "https://github.com/o/n/extra",
            "https://github.com/o/n?x=1",
            "https://github.com/o/n#x",
            "https://github.com/o/.git",
            "git@github.com:o/n/extra",
            "git@github.com:../n",
            "git@github.com:o/n x",
            "",
            "../o/n",
            "o/n",
        ] {
            assert_eq!(parse_github_remote(url), None, "{url}");
        }
    }

    fn linked(host: &str, host_url: &str, owner: &str, name: &str) -> TaskForgeRepository {
        TaskForgeRepository {
            host: host.into(),
            host_url: host_url.into(),
            repo_owner: owner.into(),
            repo_name: name.into(),
            html_url: format!("{host_url}/{owner}/{name}"),
        }
    }

    #[test]
    fn matches_owner_and_name_without_case_only_on_github_com() {
        let origin = repo("Koyori-App", "Task");
        assert!(matches(
            &origin,
            &linked("github", "https://github.com", "koyori-app", "task")
        ));
        assert!(!matches(
            &origin,
            &linked("github", "https://github.com", "koyori-app", "other")
        ));
        assert!(!matches(
            &origin,
            &linked("gitlab", "https://github.com", "koyori-app", "task")
        ));
        assert!(!matches(
            &origin,
            &linked("github", "https://github.example.com", "koyori-app", "task")
        ));
    }
}
