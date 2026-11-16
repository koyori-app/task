//! 新しい版の通知と `self-update`。GitHub の代わりに wiremock を使い、外へは通信しない。

mod common;

use chrono::{DateTime, Duration, Utc};
use common::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use task_cli::output::OutputOptions;
use task_cli::update::{Installed, ReleaseSource, Updater, notice, self_update};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TARGET: &str = "x86_64-unknown-linux-gnu";
const ORIGINAL: &[u8] = b"old binary";
const NEW_BINARY: &[u8] = b"#!/bin/sh\necho new\n";

fn updater(server_uri: &str, dir: &Path, version: &str, from_release: bool) -> Updater {
    let exe = dir.join("task");
    std::fs::write(&exe, ORIGINAL).unwrap();
    Updater {
        source: ReleaseSource {
            api_base: server_uri.to_string(),
            download_base: format!("{server_uri}/download"),
        },
        installed: Installed {
            version: version.into(),
            target: TARGET.into(),
            exe,
            from_release,
        },
    }
}

async fn mount_latest(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/releases/latest"))
        .respond_with(response)
        .mount(server)
        .await;
}

fn latest(tag: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({ "tag_name": tag }))
}

async fn requests_to(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

fn no_env(_: &str) -> Option<String> {
    None
}

fn state_in(dir: &Path) -> PathBuf {
    dir.join("state").join("update-check.json")
}

fn now() -> DateTime<Utc> {
    "2026-11-15T12:00:00Z".parse().unwrap()
}

// ---- 通知 ----

#[tokio::test]
async fn a_newer_release_is_announced_and_the_check_time_is_saved() {
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.2.0")).await;
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(&server.uri(), dir.path(), "0.1.51", true);
    let state = state_in(dir.path());

    let line = notice(&updater, &state, now(), &no_env).await.unwrap();

    assert_eq!(
        line,
        "A new release of task is available: 0.1.51 -> 0.2.0. Run `task self-update` to install it."
    );
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(saved["checked_at"], "2026-11-15T12:00:00Z");
}

/// 1 日に 1 回まで。境界は 24 時間ちょうど。
#[tokio::test]
async fn the_check_runs_at_most_once_a_day() {
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.2.0")).await;
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(&server.uri(), dir.path(), "0.1.51", true);
    let state = state_in(dir.path());

    assert!(notice(&updater, &state, now(), &no_env).await.is_some());
    assert_eq!(requests_to(&server).await, 1);

    // 同じ日の 2 回目は叩かない。23 時間 59 分でもまだ
    let almost = now() + Duration::hours(23) + Duration::minutes(59);
    assert!(notice(&updater, &state, almost, &no_env).await.is_none());
    assert_eq!(requests_to(&server).await, 1);

    // 24 時間 1 秒たてば叩く
    let after = now() + Duration::hours(24) + Duration::seconds(1);
    assert!(notice(&updater, &state, after, &no_env).await.is_some());
    assert_eq!(requests_to(&server).await, 2);
}

#[tokio::test]
async fn the_same_older_or_unreadable_version_is_not_announced() {
    for tag in ["v0.1.51", "v0.1.50", "v1.0.0-rc1", "nightly"] {
        let server = MockServer::start().await;
        mount_latest(&server, latest(tag)).await;
        let dir = tempfile::tempdir().unwrap();
        let updater = updater(&server.uri(), dir.path(), "0.1.51", true);

        let line = notice(&updater, &state_in(dir.path()), now(), &no_env).await;

        assert!(line.is_none(), "{tag}: {line:?}");
        assert_eq!(requests_to(&server).await, 1, "{tag}");
    }
}

/// 通信・制限・応答の形の失敗は、本体を止めず、何も言わない。
#[tokio::test]
async fn a_failed_check_is_silent() {
    let failures = [
        ResponseTemplate::new(403).set_body_string("rate limit exceeded"),
        ResponseTemplate::new(500),
        ResponseTemplate::new(200).set_body_string("not json"),
        ResponseTemplate::new(200).set_body_json(json!({ "other": 1 })),
    ];
    for response in failures {
        let server = MockServer::start().await;
        mount_latest(&server, response).await;
        let dir = tempfile::tempdir().unwrap();
        let updater = updater(&server.uri(), dir.path(), "0.1.51", true);

        assert!(
            notice(&updater, &state_in(dir.path()), now(), &no_env)
                .await
                .is_none()
        );
    }

    // 接続先が無い
    let dir = tempfile::tempdir().unwrap();
    let updater = updater("http://127.0.0.1:1", dir.path(), "0.1.51", true);
    assert!(
        notice(&updater, &state_in(dir.path()), now(), &no_env)
            .await
            .is_none()
    );
}

/// 止める条件では、通信しない。
#[tokio::test]
async fn the_check_can_be_turned_off() {
    let cases = [
        ("opt-out", true, Some(("TASK_NO_UPDATE_CHECK", "1"))),
        ("ci", true, Some(("CI", "true"))),
        // 手元のビルドは 0.1.0 を名乗るので、毎回通知になってしまう
        ("local build", false, None),
        ("fresh", true, None),
    ];
    for (name, from_release, env) in cases {
        let server = MockServer::start().await;
        mount_latest(&server, latest("v0.2.0")).await;
        let dir = tempfile::tempdir().unwrap();
        let updater = updater(&server.uri(), dir.path(), "0.1.51", from_release);
        let lookup = move |key: &str| {
            env.filter(|(name, _)| *name == key)
                .map(|(_, value)| value.to_string())
        };

        let line = notice(&updater, &state_in(dir.path()), now(), &lookup).await;

        if name == "fresh" {
            assert!(line.is_some());
        } else {
            assert!(line.is_none(), "{name}");
            assert_eq!(requests_to(&server).await, 0, "{name}");
        }
    }
}

/// 空の値は未設定として扱う（`TASK_NO_UPDATE_CHECK=` で止まらない）。
#[tokio::test]
async fn an_empty_opt_out_value_does_not_turn_the_check_off() {
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.2.0")).await;
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(&server.uri(), dir.path(), "0.1.51", true);
    let lookup = |key: &str| (key == "TASK_NO_UPDATE_CHECK" || key == "CI").then(String::new);

    assert!(
        notice(&updater, &state_in(dir.path()), now(), &lookup)
            .await
            .is_some()
    );
}

// ---- 更新 ----

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, content) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, *content).unwrap();
    }
    let mut encoder = builder.into_inner().unwrap();
    encoder.flush().unwrap();
    encoder.finish().unwrap()
}

fn sha256_line(bytes: &[u8], name: &str) -> String {
    format!("{}  {name}\n", hex::encode(Sha256::digest(bytes)))
}

async fn mount_asset(server: &MockServer, version: &str, archive: &[u8], checksum: String) {
    let name = format!("task-{version}-{TARGET}.tar.gz");
    Mock::given(method("GET"))
        .and(path(format!("/download/v{version}/{name}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(archive.to_vec()))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/download/v{version}/{name}.sha256")))
        .respond_with(ResponseTemplate::new(200).set_body_string(checksum))
        .mount(server)
        .await;
}

fn no_temp_files_left(dir: &Path) -> bool {
    std::fs::read_dir(dir).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".task-update")
    })
}

#[tokio::test]
async fn self_update_replaces_the_binary_after_verifying_the_checksum() {
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.2.0")).await;
    let archive = archive(&[("task", NEW_BINARY)]);
    mount_asset(
        &server,
        "0.2.0",
        &archive,
        sha256_line(&archive, "task-0.2.0-x86_64-unknown-linux-gnu.tar.gz"),
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(&server.uri(), dir.path(), "0.1.51", true);
    // 元の権限を引き継ぐ（常に 0755 にしない）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            &updater.installed.exe,
            std::fs::Permissions::from_mode(0o750),
        )
        .unwrap();
    }

    let code = self_update(&updater, OutputOptions::new(false))
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(std::fs::read(&updater.installed.exe).unwrap(), NEW_BINARY);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&updater.installed.exe)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o750);
    }
    assert!(no_temp_files_left(dir.path()));
}

#[tokio::test]
async fn self_update_refuses_a_mismatching_checksum_and_leaves_the_binary_alone() {
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.2.0")).await;
    let archive = archive(&[("task", NEW_BINARY)]);
    mount_asset(
        &server,
        "0.2.0",
        &archive,
        sha256_line(b"something else", "x"),
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(&server.uri(), dir.path(), "0.1.51", true);

    let err = self_update(&updater, OutputOptions::new(false))
        .await
        .unwrap_err();

    assert_eq!(err.exit_code, 1);
    assert!(err.message.contains("Checksum mismatch"), "{}", err.message);
    assert_eq!(std::fs::read(&updater.installed.exe).unwrap(), ORIGINAL);
    assert!(no_temp_files_left(dir.path()));
}

#[tokio::test]
async fn self_update_downloads_nothing_when_already_the_latest() {
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.1.51")).await;
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(&server.uri(), dir.path(), "0.1.51", true);

    let code = self_update(&updater, OutputOptions::new(false))
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(requests_to(&server).await, 1, "releases/latest だけを引く");
    assert_eq!(std::fs::read(&updater.installed.exe).unwrap(), ORIGINAL);
}

#[tokio::test]
async fn self_update_fails_cleanly_when_the_release_cannot_be_read() {
    // 制限（403）
    let server = MockServer::start().await;
    mount_latest(&server, ResponseTemplate::new(403)).await;
    let dir = tempfile::tempdir().unwrap();
    let updater_a = updater(&server.uri(), dir.path(), "0.1.51", true);
    let err = self_update(&updater_a, OutputOptions::new(false))
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 1);
    assert!(
        err.message.contains("Cannot check the latest release"),
        "{}",
        err.message
    );
    assert_eq!(std::fs::read(&updater_a.installed.exe).unwrap(), ORIGINAL);

    // 配布物が無い（404）
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.2.0")).await;
    let dir = tempfile::tempdir().unwrap();
    let updater_b = updater(&server.uri(), dir.path(), "0.1.51", true);
    let err = self_update(&updater_b, OutputOptions::new(false))
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 1);
    assert!(
        err.message.contains("No release asset for"),
        "{}",
        err.message
    );
    assert_eq!(std::fs::read(&updater_b.installed.exe).unwrap(), ORIGINAL);
}

#[tokio::test]
async fn self_update_refuses_an_archive_without_the_binary() {
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.2.0")).await;
    let archive = archive(&[("README.md", b"hi")]);
    mount_asset(
        &server,
        "0.2.0",
        &archive,
        sha256_line(&archive, "task-0.2.0-x86_64-unknown-linux-gnu.tar.gz"),
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(&server.uri(), dir.path(), "0.1.51", true);

    let err = self_update(&updater, OutputOptions::new(false))
        .await
        .unwrap_err();

    assert_eq!(err.exit_code, 1);
    assert_eq!(std::fs::read(&updater.installed.exe).unwrap(), ORIGINAL);
    assert!(no_temp_files_left(dir.path()));
}

fn archive_with_entry(name: &str, kind: tar::EntryType, link: Option<&str>) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(kind);
    header.set_size(0);
    header.set_mode(0o755);
    if let Some(link) = link {
        header.set_link_name(link).unwrap();
    }
    builder
        .append_data(&mut header, name, std::io::empty())
        .unwrap();
    let mut encoder = builder.into_inner().unwrap();
    encoder.flush().unwrap();
    encoder.finish().unwrap()
}

/// `task` という名前でも、通常ファイルでない要素や空の中身で実行ファイルを置き換えない。
#[tokio::test]
async fn self_update_refuses_a_task_entry_that_is_not_a_regular_non_empty_file() {
    let cases = [
        (
            "symlink",
            archive_with_entry("task", tar::EntryType::Symlink, Some("/bin/sh")),
            "not a regular file",
        ),
        (
            "directory",
            archive_with_entry("task", tar::EntryType::Directory, None),
            "not a regular file",
        ),
        ("empty", archive(&[("task", b"")]), "empty"),
    ];
    for (name, archive, expected) in cases {
        let server = MockServer::start().await;
        mount_latest(&server, latest("v0.2.0")).await;
        mount_asset(
            &server,
            "0.2.0",
            &archive,
            sha256_line(&archive, "task-0.2.0-x86_64-unknown-linux-gnu.tar.gz"),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let updater = updater(&server.uri(), dir.path(), "0.1.51", true);

        let err = self_update(&updater, OutputOptions::new(false))
            .await
            .unwrap_err();

        assert_eq!(err.exit_code, 1, "{name}");
        assert!(err.message.contains(expected), "{name}: {}", err.message);
        assert_eq!(
            std::fs::read(&updater.installed.exe).unwrap(),
            ORIGINAL,
            "{name}"
        );
        assert!(no_temp_files_left(dir.path()), "{name}");
    }
}

/// 手元のビルドでは、実行ファイルを置き換えず、通信もしない。
#[tokio::test]
async fn self_update_stops_on_a_local_build_without_touching_the_binary() {
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.2.0")).await;
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(&server.uri(), dir.path(), "0.1.0", false);

    let err = self_update(&updater, OutputOptions::new(false))
        .await
        .unwrap_err();

    assert_eq!(err.exit_code, 1);
    assert!(err.message.contains("local build"), "{}", err.message);
    assert_eq!(requests_to(&server).await, 0);
    assert_eq!(std::fs::read(&updater.installed.exe).unwrap(), ORIGINAL);
}

/// 通知の状態ファイルの置き場（設定ディレクトリ）は、他人に入れない権限で作る。
#[cfg(unix)]
#[tokio::test]
async fn the_check_state_directory_is_created_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.2.0")).await;
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(&server.uri(), dir.path(), "0.1.51", true);
    let state = state_in(dir.path());

    notice(&updater, &state, now(), &no_env).await;

    let mode = std::fs::metadata(state.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

// ---- 本体を止めない ----

/// 通知の失敗は本体の結果に影響しない。
#[tokio::test]
async fn a_failing_check_does_not_stop_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let updater = updater("http://127.0.0.1:1", dir.path(), "0.1.51", true);
    let harness = harness().await;
    let harness = harness.with_updater(updater);
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/projects")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([project_json()])))
        .mount(&harness.server)
        .await;

    assert_eq!(harness.run(&["task", "projects", "list"]).await.unwrap(), 0);
    assert!(harness.update_notice().await.is_none());
}

#[tokio::test]
async fn a_future_check_time_is_checked_again() {
    let server = MockServer::start().await;
    mount_latest(&server, latest("v0.2.0")).await;
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(&server.uri(), dir.path(), "0.1.51", true);
    let state = state_in(dir.path());
    std::fs::create_dir_all(state.parent().unwrap()).unwrap();
    std::fs::write(
        &state,
        json!({"checked_at": now() + Duration::hours(48)}).to_string(),
    )
    .unwrap();
    assert!(notice(&updater, &state, now(), &no_env).await.is_some());
    assert_eq!(requests_to(&server).await, 1);
    assert!(notice(&updater, &state, now(), &no_env).await.is_none());
    assert_eq!(requests_to(&server).await, 1);
}
