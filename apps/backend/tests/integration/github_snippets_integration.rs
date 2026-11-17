// テストケースの一覧
// - 未認証は 401
// - 連携プロジェクトに入れる利用者は 200 で行が合い、GitHub へは連携行の owner / repo・要求の SHA（`ref`）・
//   installation token の Bearer で取りに行く。250 行のファイルに `L1-L250` を求めると 200 行 + `truncated`
// - 同じ抜粋の 2 回目は GitHub へ取りに行かない（キャッシュ）。応答は `Cache-Control: private, max-age=300`
// - 別テナントの利用者は 404。キャッシュが温まった後でも 404（対照: 入れる利用者は 200）。
//   継承を切ったプロジェクトから外れているテナントメンバーも 404（対照: 継承中は 200）
// - 未連携のリポジトリは 404。owner / repo の大文字小文字違いは 200
// - sha がブランチ名、path に `..`、`start=0`、`end < start` は 400
// - GitHub が 404、ディレクトリ（配列）、行がファイルの外は 404。UTF-8 でない本文と `encoding: "none"` は 422
// - PAT は 403（対照: 同じ利用者のセッションは 200）
// - キャッシュに無い抜粋を FETCH_RATE_LIMIT 回取った次は 429。キャッシュ済みの抜粋は 200 のままで、
//   別の利用者の枠にも影響しない
//
//! 連携リポジトリのコードの抜粋（GET /v1/github/snippet）。

use crate::common::{
    TestApp, TestUser, ensure_tenant_member_for_project, stop_inheriting, unique_installation_id,
};
use crate::forge_manual_pull_request_links_integration::forget_github_mock;
use crate::forge_pull_request_links_integration::github_mock;
use axum::http::{StatusCode, header};
use entity::github_integrations;
use entity::scopes::Scope;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use service::github::snippets::{FETCH_RATE_LIMIT, MAX_SNIPPET_LINES, RATE_LIMIT_KEY_PREFIX};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REPO_OWNER: &str = "acme";
const SHA: &str = "a3f92c1e7b81d4000000000000000000000000aa";
const FILE_PATH: &str = "src/lib.rs";
/// モックのファイルの行数。抜粋の上限（200 行）を越える値にする。
const FILE_LINES: usize = 250;

struct Fixture {
    owner: TestUser,
    tenant_id: Uuid,
    project_id: Uuid,
    /// テストごとに一意なリポジトリ名。同じ DB に残る他のテストの連携行と混ざらないようにする
    repo: String,
}

async fn setup(app: &TestApp) -> Fixture {
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let repo = format!("repo-{}", &Uuid::new_v4().simple().to_string()[..12]);
    insert_integration(app, tp.project_id, &repo, owner.id).await;
    Fixture {
        owner,
        tenant_id: tp.tenant_id,
        project_id: tp.project_id,
        repo,
    }
}

async fn insert_integration(app: &TestApp, project_id: Uuid, repo: &str, created_by: Uuid) {
    github_integrations::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        installation_id: Set(unique_installation_id()),
        repo_owner: Set(REPO_OWNER.into()),
        repo_name: Set(repo.into()),
        access_token_enc: Set("unused".into()),
        token_expires_at: Set(chrono::Utc::now().into()),
        created_by: Set(created_by),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert integration");
}

async fn login(app: &mut TestApp, user: &TestUser) {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
}

fn contents_path(repo: &str, file: &str) -> String {
    format!("/repos/{REPO_OWNER}/{repo}/contents/{file}")
}

/// 標準の base64（パディングあり）。backend の bin クレートは base64 に依存しないので手で書く。
fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// GitHub の Contents API と同じく、base64 を 60 桁ごとに改行して返す。
fn file_body(bytes: &[u8]) -> serde_json::Value {
    let encoded = base64_encode(bytes);
    let wrapped: String = encoded
        .as_bytes()
        .chunks(60)
        .map(|chunk| format!("{}\n", std::str::from_utf8(chunk).expect("ascii")))
        .collect();
    serde_json::json!({
        "type": "file",
        "encoding": "base64",
        "size": bytes.len(),
        "content": wrapped,
    })
}

fn file_text() -> String {
    (1..=FILE_LINES).map(|n| format!("line {n}\n")).collect()
}

async fn mount_file(mock_server: &MockServer, repo: &str, file: &str, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(contents_path(repo, file)))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(mock_server)
        .await;
}

fn snippet_url(owner: &str, repo: &str, sha: &str, file: &str, range: &str) -> String {
    format!("/v1/github/snippet?owner={owner}&repo={repo}&sha={sha}&path={file}&{range}")
}

async fn contents_requests(mock_server: &MockServer, repo: &str) -> Vec<wiremock::Request> {
    let prefix = format!("/repos/{REPO_OWNER}/{repo}/contents/");
    mock_server
        .received_requests()
        .await
        .expect("request recording")
        .into_iter()
        .filter(|req| req.url.path().starts_with(&prefix))
        .collect()
}

#[serial_test::serial]
#[tokio::test]
async fn unauthenticated_request_is_rejected() {
    let app = TestApp::new_with_github().await;
    let res = app
        .get(&snippet_url(
            REPO_OWNER, "backend", SHA, FILE_PATH, "start=1",
        ))
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[serial_test::serial]
#[tokio::test]
async fn member_gets_lines_and_second_read_is_cached() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    mount_file(
        &mock_server,
        &fx.repo,
        FILE_PATH,
        file_body(file_text().as_bytes()),
    )
    .await;
    login(&mut app, &fx.owner).await;

    let url = snippet_url(REPO_OWNER, &fx.repo, SHA, FILE_PATH, "start=3&end=5");
    let res = app.get(&url).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "private, max-age=300");
    let body: serde_json::Value = res.json().await.expect("json");
    assert_eq!(
        body,
        serde_json::json!({
            "repo_owner": REPO_OWNER,
            "repo_name": fx.repo,
            "sha": SHA,
            "path": FILE_PATH,
            "start_line": 3,
            "end_line": 5,
            "lines": ["line 3", "line 4", "line 5"],
            "truncated": false,
        })
    );

    let requests = contents_requests(&mock_server, &fx.repo).await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url.query_pairs().collect::<Vec<_>>(),
        vec![("ref".into(), SHA.into())]
    );
    assert_eq!(
        requests[0].headers["authorization"],
        "Bearer ghs_test_installation_token"
    );

    // 2 回目は GitHub へ取りに行かない
    let res = app.get(&url).await;
    assert_eq!(res.status(), StatusCode::OK);
    let again: serde_json::Value = res.json().await.expect("json");
    assert_eq!(again, body);
    assert_eq!(contents_requests(&mock_server, &fx.repo).await.len(), 1);

    // 1 行だけ（end を省く）
    let res = app
        .get(&snippet_url(
            REPO_OWNER,
            &fx.repo,
            SHA,
            FILE_PATH,
            "start=250",
        ))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = res.json().await.expect("json");
    assert_eq!(body["lines"], serde_json::json!(["line 250"]));
    assert_eq!(body["end_line"], 250);

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn long_range_is_truncated_to_the_line_limit() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    mount_file(
        &mock_server,
        &fx.repo,
        FILE_PATH,
        file_body(file_text().as_bytes()),
    )
    .await;
    login(&mut app, &fx.owner).await;

    let res = app
        .get(&snippet_url(
            REPO_OWNER,
            &fx.repo,
            SHA,
            FILE_PATH,
            &format!("start=1&end={FILE_LINES}"),
        ))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = res.json().await.expect("json");
    let lines = body["lines"].as_array().expect("lines");
    assert_eq!(lines.len(), MAX_SNIPPET_LINES);
    assert_eq!(lines[MAX_SNIPPET_LINES - 1], "line 200");
    assert_eq!(body["end_line"], MAX_SNIPPET_LINES);
    assert_eq!(body["truncated"], true);

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn outsiders_get_not_found_even_after_the_cache_is_warm() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    mount_file(
        &mock_server,
        &fx.repo,
        FILE_PATH,
        file_body(file_text().as_bytes()),
    )
    .await;
    let url = snippet_url(REPO_OWNER, &fx.repo, SHA, FILE_PATH, "start=1&end=2");

    // 別テナントの利用者（自分のテナントとプロジェクトは持っている）
    let outsider = app.insert_user(false, false).await;
    app.insert_tenant_project(outsider.id).await;
    login(&mut app, &outsider).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::NOT_FOUND);
    assert!(contents_requests(&mock_server, &fx.repo).await.is_empty());

    // 対照: 入れる利用者は 200（これでキャッシュが温まる）
    login(&mut app, &fx.owner).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::OK);

    // キャッシュが温まった後でも、入れない利用者は 404
    login(&mut app, &outsider).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::NOT_FOUND);

    // テナントメンバーは、継承中のプロジェクトなら 200
    let member = app.insert_user(false, false).await;
    ensure_tenant_member_for_project(&app.state.db, fx.project_id, member.id).await;
    login(&mut app, &member).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::OK);

    // 継承を切ると、project_members に居ないテナントメンバーは 404
    stop_inheriting(&app.state.db, fx.project_id).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::NOT_FOUND);
    // 対照: オーナーは 200 のまま
    login(&mut app, &fx.owner).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::OK);

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn unlinked_repository_is_not_found_and_case_is_ignored() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    mount_file(
        &mock_server,
        &fx.repo,
        FILE_PATH,
        file_body(file_text().as_bytes()),
    )
    .await;
    login(&mut app, &fx.owner).await;

    let unlinked = format!("{}-x", fx.repo);
    let res = app
        .get(&snippet_url(
            REPO_OWNER, &unlinked, SHA, FILE_PATH, "start=1",
        ))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // owner / repo の大文字小文字が違っても連携行に当たり、GitHub へは連携行の表記で取りに行く
    let res = app
        .get(&snippet_url(
            "ACME",
            &fx.repo.to_uppercase(),
            &SHA.to_uppercase(),
            FILE_PATH,
            "start=1",
        ))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = res.json().await.expect("json");
    assert_eq!(body["repo_owner"], REPO_OWNER);
    assert_eq!(body["repo_name"], fx.repo);
    assert_eq!(body["sha"], SHA);
    assert_eq!(body["lines"], serde_json::json!(["line 1"]));

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn malformed_queries_are_bad_requests() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    mount_file(
        &mock_server,
        &fx.repo,
        FILE_PATH,
        file_body(file_text().as_bytes()),
    )
    .await;
    login(&mut app, &fx.owner).await;

    let cases = [
        snippet_url(REPO_OWNER, &fx.repo, "main", FILE_PATH, "start=1"),
        snippet_url(REPO_OWNER, &fx.repo, SHA, "src/../lib.rs", "start=1"),
        snippet_url(REPO_OWNER, &fx.repo, SHA, FILE_PATH, "start=0"),
        snippet_url(REPO_OWNER, &fx.repo, SHA, FILE_PATH, "start=5&end=4"),
    ];
    for url in cases {
        let res = app.get(&url).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{url}");
    }
    assert!(contents_requests(&mock_server, &fx.repo).await.is_empty());

    // 対照: 正しい形は 200
    let res = app
        .get(&snippet_url(
            REPO_OWNER,
            &fx.repo,
            SHA,
            FILE_PATH,
            "start=5&end=5",
        ))
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn missing_and_undisplayable_files_map_to_404_and_422() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    mount_file(&mock_server, &fx.repo, "ok.txt", file_body(b"one\ntwo\n")).await;
    // ディレクトリは配列で返る
    mount_file(
        &mock_server,
        &fx.repo,
        "src",
        serde_json::json!([{ "type": "file", "name": "lib.rs" }]),
    )
    .await;
    mount_file(
        &mock_server,
        &fx.repo,
        "bin.dat",
        file_body(&[0xff, 0xfe, 0x00, 0x80]),
    )
    .await;
    // 1 MB を超えるファイルは本文が付かない
    mount_file(
        &mock_server,
        &fx.repo,
        "huge.txt",
        serde_json::json!({ "type": "file", "encoding": "none", "size": 5_000_000, "content": "" }),
    )
    .await;
    login(&mut app, &fx.owner).await;

    let get = |file: &'static str, range: &'static str| {
        let url = snippet_url(REPO_OWNER, &fx.repo, SHA, file, range);
        let app = &app;
        async move { app.get(&url).await.status() }
    };
    // モックに無いパスは GitHub が 404 を返す
    assert_eq!(get("missing.txt", "start=1").await, StatusCode::NOT_FOUND);
    assert_eq!(get("src", "start=1").await, StatusCode::NOT_FOUND);
    // 行がファイルの外
    assert_eq!(get("ok.txt", "start=3").await, StatusCode::NOT_FOUND);
    assert_eq!(
        get("bin.dat", "start=1").await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        get("huge.txt", "start=1").await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // 対照: 同じリポジトリの読めるファイルは 200
    assert_eq!(get("ok.txt", "start=2").await, StatusCode::OK);

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn personal_token_is_forbidden() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    mount_file(
        &mock_server,
        &fx.repo,
        FILE_PATH,
        file_body(file_text().as_bytes()),
    )
    .await;
    let token = app
        .insert_pat(fx.owner.id, fx.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let url = snippet_url(REPO_OWNER, &fx.repo, SHA, FILE_PATH, "start=1");

    app.reset_session_client();
    let res = app.get_with_bearer(&url, &token).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert!(contents_requests(&mock_server, &fx.repo).await.is_empty());

    // 対照: 同じ利用者のセッションは 200
    login(&mut app, &fx.owner).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::OK);

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn uncached_fetches_are_rate_limited_per_user() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    mount_file(
        &mock_server,
        &fx.repo,
        FILE_PATH,
        file_body(file_text().as_bytes()),
    )
    .await;
    login(&mut app, &fx.owner).await;

    let line_url = |line: i64| {
        snippet_url(
            REPO_OWNER,
            &fx.repo,
            SHA,
            FILE_PATH,
            &format!("start={line}"),
        )
    };
    for line in 1..=FETCH_RATE_LIMIT {
        let res = app.get(&line_url(line)).await;
        assert_eq!(res.status(), StatusCode::OK, "line {line}");
    }
    let fetched = contents_requests(&mock_server, &fx.repo).await.len();
    assert_eq!(fetched as i64, FETCH_RATE_LIMIT);

    // 上限の次は 429 で、GitHub へは取りに行かない
    let res = app.get(&line_url(FETCH_RATE_LIMIT + 1)).await;
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        contents_requests(&mock_server, &fx.repo).await.len(),
        fetched
    );
    // キャッシュ済みの抜粋は 200 のまま
    assert_eq!(app.get(&line_url(1)).await.status(), StatusCode::OK);

    // 別の利用者の枠には影響しない
    let member = app.insert_user(false, false).await;
    ensure_tenant_member_for_project(&app.state.db, fx.project_id, member.id).await;
    login(&mut app, &member).await;
    let res = app.get(&line_url(FETCH_RATE_LIMIT + 1)).await;
    assert_eq!(res.status(), StatusCode::OK);

    let mut conn = app.state.redis_client.conn.acquire().await.unwrap();
    let _: i64 = redis::cmd("DEL")
        .arg(format!("{RATE_LIMIT_KEY_PREFIX}{}", fx.owner.id))
        .query_async(&mut conn)
        .await
        .unwrap();
    forget_github_mock();
}
