//! ユーザーアクセストークンで、インストールが見えるリポジトリと利用者の権限を REST で取る。
//!
//! 別のテナントで連携済みのインストールを連携してよいかの判定に使う。連携後は
//! インストールが見える全リポジトリから連携先を選べるので、その全部に利用者が admin 権限を
//! 持つことを GitHub 自身に答えさせる。`forge-github` はリポジトリごとの権限を返さないため、
//! `commits.rs` と同じくこちら側に置いている。

use reqwest::Client;
use serde::Deserialize;

use super::client::api_base;

/// 1 ページあたりの取得件数（GitHub の上限）。
const PER_PAGE: usize = 100;
/// 読むページ数の上限。超えた分は「見えない」扱いになり、判定は拒否側に倒れる。
const MAX_PAGES: usize = 100;

/// 利用者から見た、インストール配下のリポジトリ 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInstallationRepository {
    pub owner: String,
    pub name: String,
    pub admin: bool,
}

/// `GET /user/installations/{id}/repositories` の応答（使う分だけ）。
#[derive(Debug, Deserialize)]
struct RepositoriesPage {
    total_count: usize,
    repositories: Vec<RepositoryItem>,
}

#[derive(Debug, Deserialize)]
struct RepositoryItem {
    name: String,
    owner: RepositoryOwner,
    /// 無ければ権限なしとして扱う（閉じる側に倒す）
    #[serde(default)]
    permissions: Option<RepositoryPermissions>,
}

#[derive(Debug, Deserialize)]
struct RepositoryOwner {
    login: String,
}

#[derive(Debug, Deserialize)]
struct RepositoryPermissions {
    #[serde(default)]
    admin: bool,
}

/// 利用者がそのインストール越しに見えるリポジトリを、admin 権限の有無つきで全件取る。
///
/// App JWT でも Installation Access Token でもなく、ユーザーアクセストークンで呼ぶ。
/// エラーの文言は `commits.rs` の `get_json`（`{context} failed: {status}: {body}`）に揃えてあり、
/// 呼び出し側がステータスを文言から読む。
pub async fn list_user_installation_repositories(
    http: &Client,
    user_access_token: &str,
    installation_id: i64,
) -> Result<Vec<UserInstallationRepository>, anyhow::Error> {
    let mut repositories = Vec::new();

    for page in 1..=MAX_PAGES {
        let url = format!(
            "{}/user/installations/{installation_id}/repositories?per_page={PER_PAGE}&page={page}",
            api_base()
        );
        let body: RepositoriesPage = super::commits::get_json(
            http,
            user_access_token,
            &url,
            "list user installation repositories",
        )
        .await?;

        let fetched = body.repositories.len();
        repositories.extend(
            body.repositories
                .into_iter()
                .map(|item| UserInstallationRepository {
                    owner: item.owner.login,
                    name: item.name,
                    admin: item.permissions.is_some_and(|p| p.admin),
                }),
        );

        if repositories.len() >= body.total_count || fetched < PER_PAGE {
            break;
        }
    }

    Ok(repositories)
}

/// インストールが見える全リポジトリに、利用者が admin 権限を持つか。
///
/// `installation` が空なら偽（任せられる範囲が確かめられない）。owner と name は GitHub が
/// 大文字小文字を区別しないので、ASCII の大文字小文字を無視して突き合わせる。
pub fn covers_all_repositories(
    installation: &[forge_core::Repository],
    user: &[UserInstallationRepository],
) -> bool {
    if installation.is_empty() {
        return false;
    }
    installation.iter().all(|repo| {
        user.iter().any(|candidate| {
            candidate.admin
                && candidate.owner.eq_ignore_ascii_case(&repo.owner)
                && candidate.name.eq_ignore_ascii_case(&repo.name)
        })
    })
}

#[cfg(test)]
mod tests {
    use forge_core::Repository;

    use super::{UserInstallationRepository, covers_all_repositories};

    fn user(owner: &str, name: &str, admin: bool) -> UserInstallationRepository {
        UserInstallationRepository {
            owner: owner.to_owned(),
            name: name.to_owned(),
            admin,
        }
    }

    #[test]
    fn empty_installation_is_not_covered() {
        assert!(!covers_all_repositories(&[], &[]));
        assert!(!covers_all_repositories(&[], &[user("acme", "a", true)]));
    }

    #[test]
    fn all_admin_is_covered() {
        let installation = [Repository::new("acme", "a"), Repository::new("acme", "b")];
        let user = [user("acme", "a", true), user("acme", "b", true)];
        assert!(covers_all_repositories(&installation, &user));
    }

    #[test]
    fn one_non_admin_is_not_covered() {
        let installation = [Repository::new("acme", "a"), Repository::new("acme", "b")];
        let user = [user("acme", "a", true), user("acme", "b", false)];
        assert!(!covers_all_repositories(&installation, &user));
    }

    #[test]
    fn repository_missing_on_user_side_is_not_covered() {
        let installation = [Repository::new("acme", "a"), Repository::new("acme", "b")];
        let user = [user("acme", "a", true)];
        assert!(!covers_all_repositories(&installation, &user));
    }

    #[test]
    fn owner_and_name_match_case_insensitively() {
        let installation = [Repository::new("Acme", "Repo")];
        let user = [user("acme", "repo", true)];
        assert!(covers_all_repositories(&installation, &user));
    }

    #[test]
    fn extra_repositories_on_user_side_do_not_matter() {
        let installation = [Repository::new("acme", "a")];
        let user = [
            user("acme", "a", true),
            user("acme", "b", false),
            user("other", "a", false),
        ];
        assert!(covers_all_repositories(&installation, &user));
    }

    #[test]
    fn same_name_under_another_owner_does_not_cover() {
        let installation = [Repository::new("acme", "a")];
        let user = [user("other", "a", true)];
        assert!(!covers_all_repositories(&installation, &user));
    }
}
