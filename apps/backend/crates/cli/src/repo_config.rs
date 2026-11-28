//! リポジトリ内の設定ファイル（`.koyori-task.yaml`）の探索と解析。
//!
//! 出力（未知のキーの警告・既定値の由来）と `profile` の扱いはここに置かない。
//! `task init` が既存のファイルを示すのにも同じ関数を使うため、読む側は純粋な読み取りだけにする。
//! 書くのは `task init` の `write_new` だけで、既にあるファイルは書き換えない。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CliError, Result};

pub const FILE_NAME: &str = ".koyori-task.yaml";

/// 読み取った 1 ファイルの中身。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoConfig {
    pub path: PathBuf,
    pub project: Option<String>,
    pub profile: Option<String>,
    /// 知らないキー（名前の昇順）。値は読まない
    pub unknown_keys: Vec<String>,
}

impl RepoConfig {
    /// 既定値のプロジェクトをどこから取ったかの 1 行（標準エラーに出す）。
    pub fn project_source_line(&self, project: &str) -> String {
        format!("project: {project} (from {})", self.path.display())
    }
}

#[derive(Deserialize)]
struct Raw {
    project: Option<String>,
    profile: Option<String>,
    #[serde(flatten)]
    unknown: BTreeMap<String, serde_yaml_ng::Value>,
}

fn cannot_read(path: &Path, err: std::io::Error) -> CliError {
    CliError::new(format!("Cannot read {}: {err}", path.display()))
}

/// `path` に何かがあるか。リンクは辿らない（切れたリンクも「ある」）。
fn exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(cannot_read(path, err)),
    }
}

/// `start` から上へ見て、`.git`（ディレクトリでもファイルでも）のある最初の階層。
/// 根まで無ければ `None`。`start` は絶対パスで渡す。
pub fn git_root(start: &Path) -> Result<Option<PathBuf>> {
    for dir in start.ancestors() {
        if exists(&dir.join(".git"))? {
            return Ok(Some(dir.to_path_buf()));
        }
    }
    Ok(None)
}

/// `start` からリポジトリの根までで最も近い設定ファイル。git の外なら `None`。
pub fn locate(start: &Path) -> Result<Option<PathBuf>> {
    let Some(root) = git_root(start)? else {
        return Ok(None);
    };
    for dir in start.ancestors() {
        let path = dir.join(FILE_NAME);
        if exists(&path)? {
            return Ok(Some(path));
        }
        if dir == root {
            break;
        }
    }
    Ok(None)
}

/// 最も近い設定ファイルを読む。無ければ `None`。読み取りと解析の失敗は返す。
pub fn load(start: &Path) -> Result<Option<RepoConfig>> {
    let Some(path) = locate(start)? else {
        return Ok(None);
    };
    let raw = std::fs::read_to_string(&path).map_err(|err| cannot_read(&path, err))?;
    parse(&path, &raw).map(Some)
}

pub fn parse(path: &Path, raw: &str) -> Result<RepoConfig> {
    // 空ファイルは YAML の null になる。何も指定していない状態として扱う
    let raw = serde_yaml_ng::from_str::<Option<Raw>>(raw)
        .map_err(|err| CliError::validation(format!("Cannot parse {}: {err}", path.display())))?;
    let Some(raw) = raw else {
        return Ok(RepoConfig {
            path: path.to_path_buf(),
            project: None,
            profile: None,
            unknown_keys: Vec::new(),
        });
    };
    let value = |key: &str, value: Option<String>| -> Result<Option<String>> {
        match value.map(|value| value.trim().to_string()) {
            Some(value) if value.is_empty() => Err(CliError::validation(format!(
                "{key} in {} must not be empty",
                path.display()
            ))),
            other => Ok(other),
        }
    };
    Ok(RepoConfig {
        path: path.to_path_buf(),
        project: value("project", raw.project)?,
        profile: value("profile", raw.profile)?,
        unknown_keys: raw.unknown.into_keys().collect(),
    })
}

/// `task init` が書く中身。`parse` で同じ値に戻る形にする。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepoFileBody {
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

impl RepoFileBody {
    /// ファイルに書く文字列。手で組まず YAML として直列化する
    /// （`NO` や `NULL` のようなキーを、読む側が文字列のまま読めるようにする）
    pub fn to_yaml(&self) -> Result<String> {
        serde_yaml_ng::to_string(self)
            .map_err(|err| CliError::new(format!("Cannot serialize {FILE_NAME}: {err}")))
    }
}

/// `root` に設定ファイルを新しく作る。既にあれば（切れたリンクでも）書かずに終了コード 1。
///
/// `create_new`（O_EXCL）で開くので、リンクを辿らず、同時に走った別の `init` とも競合しない。
/// 書き込みに失敗したら、作りかけのファイルを消す。
pub fn write_new(root: &Path, body: &RepoFileBody) -> Result<PathBuf> {
    use std::io::Write;

    let path = root.join(FILE_NAME);
    let yaml = body.to_yaml()?;
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(CliError::new(format!(
                "{} already exists; left it unchanged",
                path.display()
            )));
        }
        Err(err) => {
            return Err(CliError::new(format!(
                "Cannot write {}: {err}",
                path.display()
            )));
        }
    };
    if let Err(err) = file
        .write_all(yaml.as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        // 消せなくても、元の失敗を報告する
        let _removed = std::fs::remove_file(&path);
        return Err(CliError::new(format!(
            "Cannot write {}: {err}",
            path.display()
        )));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn project_at(start: &Path) -> Option<String> {
        load(start).unwrap().and_then(|config| config.project)
    }

    fn parsed(raw: &str) -> Result<RepoConfig> {
        parse(Path::new("/repo/.koyori-task.yaml"), raw)
    }

    #[test]
    fn reads_the_file_at_the_repository_root_from_a_nested_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        write(&dir.path().join(FILE_NAME), "project: APP\n");
        let start = dir.path().join("a/b");
        std::fs::create_dir_all(&start).unwrap();

        let config = load(&start).unwrap().unwrap();
        assert_eq!(config.project.as_deref(), Some("APP"));
        assert_eq!(config.path, dir.path().join(FILE_NAME));
    }

    #[test]
    fn a_git_file_of_a_worktree_or_submodule_is_also_the_boundary() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join(FILE_NAME), "project: OUTER\n");
        let repo = dir.path().join("repo");
        write(&repo.join(".git"), "gitdir: /elsewhere\n");
        std::fs::create_dir_all(repo.join("src")).unwrap();

        assert_eq!(git_root(&repo.join("src")).unwrap(), Some(repo.clone()));
        assert_eq!(project_at(&repo.join("src")), None);
    }

    #[test]
    fn only_the_nearest_file_is_used_and_not_merged_with_the_parent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        write(&dir.path().join(FILE_NAME), "project: ROOT\n");
        write(&dir.path().join("sub").join(FILE_NAME), "profile: work\n");

        let config = load(&dir.path().join("sub")).unwrap().unwrap();
        assert_eq!(config.path, dir.path().join("sub").join(FILE_NAME));
        assert_eq!(config.project, None);
        assert_eq!(config.profile.as_deref(), Some("work"));
    }

    #[test]
    fn files_above_the_git_directory_are_not_read() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join(FILE_NAME), "project: ABOVE\n");
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        assert_eq!(locate(&repo).unwrap(), None);
    }

    #[test]
    fn a_nested_repository_stops_at_its_own_git_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        write(&dir.path().join(FILE_NAME), "project: OUTER\n");
        let inner = dir.path().join("vendor/inner");
        std::fs::create_dir_all(inner.join(".git")).unwrap();

        assert_eq!(project_at(&inner), None);
        assert_eq!(project_at(dir.path()), Some("OUTER".into()));
    }

    #[test]
    fn outside_git_a_file_is_not_read_even_if_it_is_broken() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join(FILE_NAME), "project: [broken\n");
        let start = dir.path().join("a");
        std::fs::create_dir_all(&start).unwrap();

        // 一時ディレクトリの上に `.git` があるとこの前提が崩れる
        if git_root(&start).unwrap().is_none() {
            assert_eq!(load(&start).unwrap(), None);
        }
    }

    #[test]
    fn a_directory_named_like_the_file_fails_to_read() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::create_dir(dir.path().join(FILE_NAME)).unwrap();

        let err = load(dir.path()).unwrap_err();
        assert_eq!(err.exit_code, 1);
        assert!(err.message.starts_with("Cannot read "), "{}", err.message);
    }

    #[test]
    fn parses_the_keys() {
        let empty = parsed("").unwrap();
        assert_eq!((empty.project, empty.profile), (None, None));
        assert!(empty.unknown_keys.is_empty());

        let only_project = parsed("project: APP\n").unwrap();
        assert_eq!(only_project.project.as_deref(), Some("APP"));
        assert_eq!(only_project.profile, None);

        let only_profile = parsed("profile: work\n").unwrap();
        assert_eq!(only_profile.project, None);
        assert_eq!(only_profile.profile.as_deref(), Some("work"));

        let both = parsed("project: APP\nprofile: work\n").unwrap();
        assert_eq!(both.project.as_deref(), Some("APP"));
        assert_eq!(both.profile.as_deref(), Some("work"));

        let unknown = parsed("zeta: 1\nproject: APP\nalpha: [x]\n").unwrap();
        assert_eq!(unknown.unknown_keys, ["alpha", "zeta"]);

        let trimmed = parsed("project: '  APP  '\n").unwrap();
        assert_eq!(trimmed.project.as_deref(), Some("APP"));

        let null = parsed("project: null\nprofile: ~\n").unwrap();
        assert_eq!((null.project, null.profile), (None, None));
    }

    #[test]
    fn rejects_invalid_contents_with_the_path() {
        for (raw, needle) in [
            (
                "project: '  '\n",
                "project in /repo/.koyori-task.yaml must not be empty",
            ),
            (
                "profile: ''\n",
                "profile in /repo/.koyori-task.yaml must not be empty",
            ),
            ("project: [broken\n", "Cannot parse /repo/.koyori-task.yaml"),
            ("- project\n- APP\n", "Cannot parse /repo/.koyori-task.yaml"),
            (
                "project: A\nproject: B\n",
                "Cannot parse /repo/.koyori-task.yaml",
            ),
        ] {
            let err = parsed(raw).unwrap_err();
            assert_eq!(err.exit_code, 2, "{raw:?}");
            assert!(err.message.contains(needle), "{raw:?}: {}", err.message);
        }
    }

    #[test]
    fn the_source_line_names_the_project_and_the_file() {
        let config = parsed("project: APP\n").unwrap();
        assert_eq!(
            config.project_source_line("APP"),
            "project: APP (from /repo/.koyori-task.yaml)"
        );
    }

    fn body(project: &str, profile: Option<&str>) -> RepoFileBody {
        RepoFileBody {
            project: project.into(),
            profile: profile.map(Into::into),
        }
    }

    /// 書いたものを読むと同じ値に戻る。
    fn round_trip(project: &str, profile: Option<&str>) {
        let dir = tempfile::tempdir().unwrap();
        let path = write_new(dir.path(), &body(project, profile)).unwrap();
        assert_eq!(path, dir.path().join(FILE_NAME));
        let raw = std::fs::read_to_string(&path).unwrap();
        let read = parse(&path, &raw).unwrap_or_else(|err| panic!("{raw:?}: {}", err.message));
        assert_eq!(read.project.as_deref(), Some(project), "{raw:?}");
        assert_eq!(read.profile.as_deref(), profile, "{raw:?}");
        assert!(read.unknown_keys.is_empty(), "{raw:?}");
    }

    /// YAML が真偽値・null・数として読みうる綴りのキーも、文字列のまま戻る。
    #[test]
    fn what_init_writes_reads_back_as_the_same_values() {
        for key in [
            "NO", "ON", "YES", "Y", "N", "TRUE", "FALSE", "NULL", "OFF", "A1", "E1", "TASK", "INF",
            "NAN",
        ] {
            round_trip(key, None);
        }
        round_trip("11111111-1111-4111-8111-111111111111", None);
        round_trip("00000000-0000-4000-8000-000000000000", Some("work-1_a"));
        round_trip("TASK", Some("null"));
        round_trip("TASK", Some("123"));
        round_trip("TASK", Some("1e3"));
    }

    /// プロジェクトのキーの規則（先頭が大文字英字、残りが大文字英数、2〜10 文字）に沿った
    /// 値を、固定の種の線形合同で 1000 個作って往復させる。
    #[test]
    fn random_project_keys_round_trip() {
        const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        const REST: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut seed: u64 = 0x5DEECE66D;
        let mut next = move |bound: usize| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % bound
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        for _ in 0..1000 {
            let len = 2 + next(9);
            let mut key = String::new();
            key.push(LETTERS[next(LETTERS.len())] as char);
            for _ in 1..len {
                key.push(REST[next(REST.len())] as char);
            }
            let raw = body(&key, None).to_yaml().unwrap();
            let read = parse(&path, &raw).unwrap_or_else(|err| panic!("{raw:?}: {}", err.message));
            assert_eq!(read.project.as_deref(), Some(key.as_str()), "{raw:?}");
        }
    }

    #[test]
    fn omits_the_profile_line_when_there_is_no_profile() {
        assert_eq!(body("TASK", None).to_yaml().unwrap(), "project: TASK\n");
        assert_eq!(
            body("TASK", Some("work")).to_yaml().unwrap(),
            "project: TASK\nprofile: work\n"
        );
    }

    #[test]
    fn does_not_overwrite_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, "profile: work\nextra: 1\n").unwrap();

        let err = write_new(dir.path(), &body("TASK", None)).unwrap_err();
        assert_eq!(err.exit_code, 1);
        assert!(
            err.message.contains("already exists; left it unchanged"),
            "{}",
            err.message
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "profile: work\nextra: 1\n"
        );
    }

    /// 切れたリンクの先には作らない（`create_new` はリンクを辿らない）。
    #[cfg(unix)]
    #[test]
    fn does_not_write_through_a_dangling_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("elsewhere.yaml");
        std::os::unix::fs::symlink(&target, dir.path().join(FILE_NAME)).unwrap();

        let err = write_new(dir.path(), &body("TASK", None)).unwrap_err();
        assert_eq!(err.exit_code, 1);
        assert!(!target.exists(), "リンクの先に書いていない");
    }
}
