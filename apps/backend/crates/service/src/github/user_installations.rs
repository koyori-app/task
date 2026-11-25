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
/// 読むページ数の上限。超えて残りがあるときは、部分的な一覧を返さずエラーにする。
const MAX_PAGES: usize = 100;

/// 全件を読み切れなかったときのエラーの文言。[`is_incomplete_listing`] が見る。
const INCOMPLETE_LISTING: &str = "list user installation repositories incomplete";

/// [`list_user_installation_repositories`] が、全件を読み切れずに止めたエラーか。
pub fn is_incomplete_listing(error: &anyhow::Error) -> bool {
    error.to_string().starts_with(INCOMPLETE_LISTING)
}

/// `GET /installation/repositories` の応答のうち、件数だけ。
#[derive(Debug, Deserialize)]
struct RepositoryCount {
    total_count: usize,
}

/// インストールが見えるリポジトリの総数を、GitHub の `total_count` で取る。
///
/// `forge-github` の一覧は 100 件 × 100 ページで止まり、残りがあっても部分的な一覧を
/// 成功として返す。「全リポジトリの admin」の判定にその一覧を使うときは、この総数と
/// 件数が一致することを確かめ、読み切れていない一覧では認可しない。
pub async fn installation_repository_count(
    http: &Client,
    installation_access_token: &str,
) -> Result<usize, anyhow::Error> {
    let url = format!("{}/installation/repositories?per_page=1&page=1", api_base());
    let body: RepositoryCount = super::commits::get_json(
        http,
        installation_access_token,
        &url,
        "count installation repositories",
    )
    .await?;
    Ok(body.total_count)
}

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
    let mut total_count = 0;

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

        total_count = body.total_count;
        if repositories.len() >= total_count || fetched < PER_PAGE {
            break;
        }
    }

    // 残りがあるのに止めた（ページ数の上限、または途中で短いページが返った）。部分的な一覧を
    // 成功として返すと、呼び出し側が「これで全部」として権限の判定に使ってしまう。
    if repositories.len() < total_count {
        anyhow::bail!(
            "{INCOMPLETE_LISTING}: fetched {} of {total_count}",
            repositories.len()
        );
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
