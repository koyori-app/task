//! レビュー指摘のコマンド。仕様は `docs/dev/features/review-findings/index.md` の「CLI」。
//!
//! ゲートとして使われるので、終了コードと「送信前に弾いたか」を軸に確かめる。

// 受け入れ条件（docs/dev/features/review-findings/index.md から移した。テストの無い条件はページの「未実装と既知の制限」に残す）
// - CLI の鮮度検査: 最新ラウンドの `head_sha` と照合対象の HEAD が違えば不成立。HEAD が決まらないときも不成立（`--no-head-check` を付けたときだけ通す）。判定は crates/cli/src/commands/reviews.rs の gate_failure の単体テストで見る
// - CLI のリポジトリ検査: 連携の無いプロジェクトでは不成立（`--allow-unlinked` を付けたときだけ通す）。同じく gate_failure の単体テストで見る
// - 引き継ぎ: `review carry-over` は `reviews/carry-over` へ元と先の PR 番号（と任意の元リポジトリ）を送る。PR 番号が 1 未満・`--from-repo` が owner/name でない・元と先が同じ PR（`--from-repo` 無し）は、何も送らず終了コード 2
// - 指摘数の境界: 201 件は CLI の事前検証で弾く（crates/cli/src/commands/reviews.rs の単体テスト）
// - 指摘の直し（TASK-577）: `review edit` は渡した欄だけを `review-findings/{id}/content` へ送る。`--clear-file` / `--clear-line` は null で消す。`--body-file` は本文を file から読む。1 未満の行・直す欄が無い・空の題は、何も送らず終了コード 2。重大度は直せないので `--severity` を持たない

mod common;

use common::*;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

async fn mount_project_lookup(harness: &Harness) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/projects")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([project_json()])))
        .mount(&harness.server)
        .await;
}

fn write_round(dir: &std::path::Path, body: serde_json::Value) -> String {
    let file = dir.join("findings.json");
    std::fs::write(&file, body.to_string()).unwrap();
    file.to_string_lossy().into_owned()
}

#[tokio::test]
async fn submit_reads_the_file_and_creates_the_round_in_one_call() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    let dir = tempfile::tempdir().unwrap();
    let file = write_round(
        dir.path(),
        json!({
            "pr": 618,
            "head_sha": HEAD_SHA,
            "findings": [{ "severity": "high", "title": "t", "body": "b" }],
        }),
    );

    Mock::given(method("POST"))
        .and(path(project_path("reviews")))
        .and(body_json(json!({
            "pr_number": 618,
            "head_sha": HEAD_SHA,
            "summary": "",
            "findings": [{
                "severity": "high",
                "title": "t",
                "body": "b",
                "file": null,
                "line": null,
            }],
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "55555555-5555-4555-8555-555555555555",
            "project_id": PROJECT_ID,
            "pr_number": 618,
            "round": 1,
            "head_sha": HEAD_SHA,
            "reviewer": { "id": "77777777-7777-4777-8777-777777777777", "username": "yupix", "avatar_url": null },
            "reviewer_left_tenant": false,
            "summary": "",
            "pr_title": null,
            "pr_author": null,
            "created_at": "2026-01-01T00:00:00Z",
            "finding_count": 1,
            "findings": [finding_json()],
        })))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&["task", "review", "submit", &file, "--project", "APP"])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn submit_reports_invalid_json_with_the_source_it_came_from() {
    let harness = harness().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("findings.json");
    std::fs::write(&file, "{ not json").unwrap();

    let err = harness
        .run(&[
            "task",
            "review",
            "submit",
            file.to_str().unwrap(),
            "--project",
            "APP",
        ])
        .await
        .unwrap_err();

    assert_eq!(err.exit_code, 2);
    assert!(err.message.contains("invalid JSON in"), "{}", err.message);
}

#[tokio::test]
async fn list_sends_the_state_and_severity_filters() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("GET"))
        .and(path(project_path("review-findings")))
        .and(query_param("pr", "618"))
        .and(query_param("state", "open,fixed"))
        .and(query_param("severity", "high"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([finding_json()])))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "review",
            "list",
            "--project",
            "APP",
            "--pr",
            "618",
            "--state",
            "open,fixed",
            "--severity",
            "high",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

/// 連携を差し替えると旧リポジトリのラウンドが既定の視界から外れる。読み取りの
/// 3 コマンドすべてから過去の連携先を指せること（仕様 §5）。
#[tokio::test]
async fn every_read_command_can_point_at_a_previous_repository() {
    let cases: [(&str, &str, Vec<&str>, serde_json::Value); 3] = [
        ("list", "review-findings", vec![], json!([finding_json()])),
        ("rounds", "reviews", vec![], json!([])),
        (
            "summary",
            "reviews/summary",
            vec!["--no-head-check"],
            summary_json(1, 0, Some("acme/old"), true),
        ),
    ];

    for (command, suffix, extra, body) in cases {
        let harness = harness().await;
        mount_project_lookup(&harness).await;
        Mock::given(method("GET"))
            .and(path(project_path(suffix)))
            .and(query_param("pr", "618"))
            .and(query_param("repo", "acme/old"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&harness.server)
            .await;

        let mut args = vec![
            "task",
            "review",
            command,
            "--project",
            "APP",
            "--pr",
            "618",
            "--repo",
            "acme/old",
        ];
        args.extend(extra);
        assert_eq!(harness.run(&args).await.unwrap(), 0, "{command}");
    }
}

#[tokio::test]
async fn a_repository_that_is_not_owner_name_is_rejected_before_sending() {
    let harness = harness().await;
    let err = harness
        .run(&[
            "task",
            "review",
            "list",
            "--project",
            "APP",
            "--pr",
            "618",
            "--repo",
            "acme",
        ])
        .await
        .unwrap_err();

    assert_eq!(err.exit_code, 2);
    assert!(
        err.message.starts_with("--repo must be owner/name"),
        "{}",
        err.message
    );
    assert!(harness.sent_nothing().await);
}

#[tokio::test]
async fn resolve_sends_the_new_state_and_the_reason() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("PATCH"))
        .and(path(project_path("review-findings/finding-1")))
        .and(body_json(
            json!({ "state": "deferred", "note": "後で直す" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(finding_json()))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "review",
            "resolve",
            "finding-1",
            "--project",
            "APP",
            "--state",
            "deferred",
            "--note",
            "後で直す",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

/// ゲートの本体。通してよい理由が揃ったときだけ 0 で終わる。
async fn summary_exit_code(body: serde_json::Value, extra: &[&str]) -> i32 {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("GET"))
        .and(path(project_path("reviews/summary")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&harness.server)
        .await;

    let mut args = vec![
        "task",
        "review",
        "summary",
        "--project",
        "APP",
        "--pr",
        "618",
    ];
    args.extend(extra);
    harness.run(&args).await.unwrap()
}

/// 判定の場合分けは `commands::reviews` の `gate_failure` のユニットが持つ。
/// ここは「可なら 0 / 不可なら 1」で終わる配線だけを見る。
#[tokio::test]
async fn summary_exits_zero_only_when_the_gate_passes() {
    let clean = summary_json(1, 0, Some("acme/app"), true);
    assert_eq!(summary_exit_code(clean, &["--head", HEAD_SHA]).await, 0);

    let blocked = summary_json(2, 1, Some("acme/app"), false);
    assert_eq!(summary_exit_code(blocked, &["--head", HEAD_SHA]).await, 1);
}

/// 設定が無いときでも、直せる引数の誤りを先に報告する。
///
/// 設定不足とゲート不成立はどちらも非 0 で終わるので、順番が入れ替わると
/// 「`--pr` の綴りが違う」ことに気づけないまま設定を疑うことになる。
#[tokio::test]
async fn an_invalid_argument_is_reported_before_missing_configuration() {
    let (_home, context) = harness_without_config();
    let cli = parse(&["task", "review", "list", "--project", "APP", "--pr", "abc"]);

    let err = task_cli::run(cli, &context).await.unwrap_err();
    assert!(
        err.message.contains("--pr must be a positive integer"),
        "{}",
        err.message
    );
}

fn carry_over_response() -> serde_json::Value {
    json!({
        "rounds": [{
            "id": "66666666-6666-4666-8666-666666666666",
            "project_id": PROJECT_ID,
            "pr_number": 1119,
            "round": 1,
            "head_sha": HEAD_SHA,
            "reviewer": { "id": "77777777-7777-4777-8777-777777777777", "username": "yupix", "avatar_url": null },
            "reviewer_left_tenant": false,
            "summary": "PR #790 R1 から未解決の指摘 1 件を引き継ぎ",
            "pr_title": null,
            "pr_author": null,
            "created_at": "2026-01-01T00:00:00Z",
            "finding_count": 1,
            "carried_from": {
                "review_id": "55555555-5555-4555-8555-555555555555",
                "pr_number": 790,
                "round": 1,
                "repository": null,
            },
            "carried_by": { "id": "88888888-8888-4888-8888-888888888888", "username": "dev", "avatar_url": null },
            "findings": [finding_json()],
        }],
    })
}

#[tokio::test]
async fn carry_over_posts_the_source_and_destination_pull_requests() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("POST"))
        .and(path(project_path("reviews/carry-over")))
        .and(body_json(json!({
            "from_pr_number": 790,
            "from_repo": null,
            "to_pr_number": 1119,
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(carry_over_response()))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "review",
            "carry-over",
            "--project",
            "APP",
            "--from-pr",
            "790",
            "--to-pr",
            "1119",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

/// 別リポジトリ（過去の連携先）を指すときは、同じ PR 番号でも送る。
#[tokio::test]
async fn carry_over_sends_the_source_repository() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("POST"))
        .and(path(project_path("reviews/carry-over")))
        .and(body_json(json!({
            "from_pr_number": 711,
            "from_repo": "acme/old",
            "to_pr_number": 711,
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(carry_over_response()))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&[
            "task",
            "review",
            "carry-over",
            "--project",
            "APP",
            "--from-pr",
            "711",
            "--to-pr",
            "711",
            "--from-repo",
            "acme/old",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn carry_over_rejects_bad_arguments_before_sending() {
    let cases: [(&[&str], &str); 4] = [
        (
            &["--from-pr", "0", "--to-pr", "1119"],
            "--from-pr must be a positive integer",
        ),
        (
            &["--from-pr", "790", "--to-pr", "abc"],
            "--to-pr must be a positive integer",
        ),
        (
            &["--from-pr", "790", "--to-pr", "1119", "--from-repo", "x"],
            "--from-repo must be owner/name",
        ),
        (
            &["--from-pr", "790", "--to-pr", "790"],
            "--from-pr and --to-pr must be different pull requests",
        ),
    ];
    for (args, expected) in cases {
        let harness = harness().await;
        let mut argv = vec!["task", "review", "carry-over", "--project", "APP"];
        argv.extend(args);
        let err = harness.run(&argv).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{args:?}");
        assert!(err.message.starts_with(expected), "{}", err.message);
        assert!(harness.sent_nothing().await, "{args:?}");
    }
}

async fn mount_edit(harness: &Harness, body: serde_json::Value) {
    Mock::given(method("PATCH"))
        .and(path(project_path("review-findings/finding-1/content")))
        .and(body_json(body))
        .respond_with(ResponseTemplate::new(200).set_body_json(finding_json()))
        .expect(1)
        .mount(&harness.server)
        .await;
}

#[tokio::test]
async fn edit_sends_only_the_given_fields_to_the_content_path() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_edit(&harness, json!({ "title": "直した題", "line": 7 })).await;

    let code = harness
        .run(&[
            "task",
            "review",
            "edit",
            "finding-1",
            "--project",
            "APP",
            "--title",
            "直した題",
            "--line",
            "7",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn edit_clears_the_location_with_null() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_edit(&harness, json!({ "file": null, "line": null })).await;

    let code = harness
        .run(&[
            "task",
            "review",
            "edit",
            "finding-1",
            "--project",
            "APP",
            "--clear-file",
            "--clear-line",
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn edit_reads_the_body_from_a_file() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_edit(&harness, json!({ "body": "## 直した本文\n\n手順" })).await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("body.md");
    std::fs::write(&file, "## 直した本文\n\n手順\n").unwrap();

    let code = harness
        .run(&[
            "task",
            "review",
            "edit",
            "finding-1",
            "--project",
            "APP",
            "--body-file",
            file.to_str().unwrap(),
        ])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn edit_rejects_bad_arguments_before_sending() {
    let cases: [(&[&str], &str); 3] = [
        (&["--line", "0"], "--line must be a positive integer"),
        (&[], "the edit does not satisfy the API's limits"),
        (
            &["--title", ""],
            "the edit does not satisfy the API's limits",
        ),
    ];
    for (args, expected) in cases {
        let harness = harness().await;
        let mut argv = vec!["task", "review", "edit", "finding-1", "--project", "APP"];
        argv.extend(args);
        let err = harness.run(&argv).await.unwrap_err();
        assert_eq!(err.exit_code, 2, "{args:?}");
        assert!(err.message.starts_with(expected), "{}", err.message);
        assert!(harness.sent_nothing().await, "{args:?}");
    }
}

#[test]
fn edit_does_not_take_a_value_and_its_clear_flag_together() {
    for argv in [
        vec![
            "task",
            "review",
            "edit",
            "f",
            "--project",
            "APP",
            "--file",
            "a.rs",
            "--clear-file",
        ],
        vec![
            "task",
            "review",
            "edit",
            "f",
            "--project",
            "APP",
            "--line",
            "3",
            "--clear-line",
        ],
        vec![
            "task",
            "review",
            "edit",
            "f",
            "--project",
            "APP",
            "--body",
            "b",
            "--body-file",
            "x",
        ],
    ] {
        assert!(
            <task_cli::cli::Cli as clap::Parser>::try_parse_from(argv.clone()).is_err(),
            "{argv:?}"
        );
    }
}

/// 重大度は直せない（TASK-577）。旗そのものを持たない。
#[test]
fn edit_has_no_severity_flag() {
    let argv = [
        "task",
        "review",
        "edit",
        "f",
        "--project",
        "APP",
        "--severity",
        "low",
    ];
    assert!(<task_cli::cli::Cli as clap::Parser>::try_parse_from(argv).is_err());
}
