// テストケースの一覧
// - 未認証は 401
// - 連携プロジェクトに入れる利用者は 200 で行が合い、GitHub へは連携行の owner / repo・要求の SHA（`ref`）・
//   installation token の Bearer で取りに行く。250 行のファイルに `L1-L250` を求めると 200 行 + `truncated`
// - 同じ抜粋の 2 回目は GitHub へ取りに行かない（キャッシュ）。応答は拒否も含めて `Cache-Control: no-store`
// - 別テナントの利用者は 404。キャッシュが温まった後でも 404（対照: 入れる利用者は 200）。
//   継承を切ったプロジェクトから外れているテナントメンバーも 404（対照: 継承中は 200）
// - 未連携のリポジトリは 404。owner / repo の大文字小文字違いは 200
// - sha がブランチ名、path に `..`、`start=0`、`end < start` は 400
// - GitHub が 404、ディレクトリ（配列）、行がファイルの外は 404。UTF-8 でない本文と `encoding: "none"` は 422
// - PAT は 403（対照: 同じ利用者のセッションは 200）
// - キャッシュに無い抜粋を FETCH_RATE_LIMIT 回取った次は 429。キャッシュ済みの抜粋は 200 のままで、
//   別の利用者の枠にも影響しない
// - 取れなかった結果（404・422）も覚え、2 回目は GitHub へ取りに行かない
// - GitHub の 500 / 429 は 503 + `Retry-After`。その間は同じインストールの別の抜粋も取りに行かず、
//   利用者の取得回数も数えない。止める時間が過ぎれば取れる
// - プロジェクトだけのゲストは 200、別プロジェクトのゲストは 404
// - 同じリポジトリを連携した 2 つのプロジェクトのうち、2 件目にだけ入れる利用者は 200（1 件目の拒否で止まらない）
// - 同じリポジトリを他のテナントが 21 件連携していても、自分のプロジェクトの連携で 200（上限 20 件に埋もれない）
// - Device Token は 200
//
//! 連携リポジトリのコードの抜粋（GET /v1/github/snippet）。

use crate::common::{
    TestApp, TestUser, ensure_tenant_member_for_project, stop_inheriting, unique_installation_id,
};
use crate::forge_manual_pull_request_links_integration::forget_github_mock;
use crate::forge_pull_request_links_integration::github_mock;
use axum::http::{StatusCode, header};
use entity::github_integrations;
use entity::project_members;
use entity::scopes::Scope;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use service::github::snippets::{
    DEFAULT_PAUSE_SECS, FETCH_RATE_LIMIT, MAX_SNIPPET_LINES, PAUSE_KEY_PREFIX,
    RATE_LIMIT_KEY_PREFIX,
};
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
    installation_id: i64,
}

async fn setup(app: &TestApp) -> Fixture {
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let repo = format!("repo-{}", &Uuid::new_v4().simple().to_string()[..12]);
    let installation_id = insert_integration(app, tp.project_id, &repo, owner.id).await;
    Fixture {
        installation_id,
        owner,
        tenant_id: tp.tenant_id,
        project_id: tp.project_id,
        repo,
    }
}

async fn insert_integration(app: &TestApp, project_id: Uuid, repo: &str, created_by: Uuid) -> i64 {
    let installation_id = unique_installation_id();
    github_integrations::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        installation_id: Set(installation_id),
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
    installation_id
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
    // ブラウザーの HTTP キャッシュに残さない（同じブラウザーの次の利用者に、認可を通らずに渡さない）
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
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

async fn redis_del(app: &TestApp, key: String) {
    let mut conn = app.state.redis_client.conn.acquire().await.unwrap();
    let _: i64 = redis::cmd("DEL")
        .arg(key)
        .query_async(&mut conn)
        .await
        .unwrap();
}

async fn redis_get(app: &TestApp, key: String) -> Option<String> {
    let mut conn = app.state.redis_client.conn.acquire().await.unwrap();
    redis::cmd("GET")
        .arg(key)
        .query_async(&mut conn)
        .await
        .unwrap()
}

#[serial_test::serial]
#[tokio::test]
async fn failed_lookups_are_remembered() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    mount_file(
        &mock_server,
        &fx.repo,
        "bin.dat",
        file_body(&[0xff, 0xfe, 0x00, 0x80]),
    )
    .await;
    mount_file(&mock_server, &fx.repo, "ok.txt", file_body(b"one\ntwo\n")).await;
    login(&mut app, &fx.owner).await;

    // GitHub が 404 を返すファイル、表示できないファイル、行がファイルの外
    let cases = [
        ("missing.txt", "start=1", StatusCode::NOT_FOUND),
        ("bin.dat", "start=1", StatusCode::UNPROCESSABLE_ENTITY),
        ("ok.txt", "start=9", StatusCode::NOT_FOUND),
    ];
    for (file, range, expected) in cases {
        let url = snippet_url(REPO_OWNER, &fx.repo, SHA, file, range);
        let before = contents_requests(&mock_server, &fx.repo).await.len();
        for round in 0..3 {
            let res = app.get(&url).await;
            assert_eq!(res.status(), expected, "{file} round {round}");
            assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store", "{file}");
        }
        // 3 回読んでも GitHub へは 1 回
        assert_eq!(
            contents_requests(&mock_server, &fx.repo).await.len(),
            before + 1,
            "{file}"
        );
    }
    // 取れなかった結果は利用者の取得回数を 1 回ずつしか使わない
    assert_eq!(
        redis_get(&app, format!("{RATE_LIMIT_KEY_PREFIX}{}", fx.owner.id)).await,
        Some("3".into())
    );
    // 対照: 同じファイルの別の行は取れる
    let res = app
        .get(&snippet_url(REPO_OWNER, &fx.repo, SHA, "ok.txt", "start=2"))
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn upstream_failure_pauses_the_installation() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let fx = setup(&app).await;
    Mock::given(method("GET"))
        .and(path(contents_path(&fx.repo, "boom.txt")))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path(contents_path(&fx.repo, "limited.txt")))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
        .mount(&mock_server)
        .await;
    mount_file(&mock_server, &fx.repo, "ok.txt", file_body(b"one\ntwo\n")).await;
    login(&mut app, &fx.owner).await;
    let pause_key = format!("{PAUSE_KEY_PREFIX}{}", fx.installation_id);
    let url = |file: &str| snippet_url(REPO_OWNER, &fx.repo, SHA, file, "start=1");

    // GitHub の 500 は内部エラーではなく 503。Retry-After は既定の秒数
    let res = app.get(&url("boom.txt")).await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        res.headers()[header::RETRY_AFTER],
        DEFAULT_PAUSE_SECS.to_string().as_str()
    );
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
    let body: serde_json::Value = res.json().await.expect("json");
    assert_eq!(body["message"], "github-unavailable");
    assert_eq!(contents_requests(&mock_server, &fx.repo).await.len(), 1);

    // 止めている間は、同じインストールの読めるファイルも取りに行かない
    let res = app.get(&url("ok.txt")).await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(contents_requests(&mock_server, &fx.repo).await.len(), 1);
    // 止めている間の要求は利用者の取得回数に数えない
    assert_eq!(
        redis_get(&app, format!("{RATE_LIMIT_KEY_PREFIX}{}", fx.owner.id)).await,
        Some("1".into())
    );

    // 止める時間が過ぎれば取れる（失敗そのものは覚えていない）
    redis_del(&app, pause_key.clone()).await;
    let res = app.get(&url("ok.txt")).await;
    assert_eq!(res.status(), StatusCode::OK);

    // GitHub の 429 は Retry-After の秒数だけ止める
    let res = app.get(&url("limited.txt")).await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(res.headers()[header::RETRY_AFTER], "120");
    let before = contents_requests(&mock_server, &fx.repo).await.len();
    let res = app.get(&url("limited.txt")).await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        contents_requests(&mock_server, &fx.repo).await.len(),
        before
    );
    // キャッシュ済みの抜粋は止めている間も返る
    assert_eq!(app.get(&url("ok.txt")).await.status(), StatusCode::OK);

    redis_del(&app, pause_key).await;
    forget_github_mock();
}

async fn insert_project_member(app: &TestApp, project_id: Uuid, user_id: Uuid) {
    project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        user_id: Set(user_id),
        role: Set(project_members::ProjectRole::Member),
    }
    .insert(&app.state.db)
    .await
    .expect("insert project member");
}

#[serial_test::serial]
#[tokio::test]
async fn project_guest_can_read_only_their_project() {
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
    let url = snippet_url(REPO_OWNER, &fx.repo, SHA, FILE_PATH, "start=1");

    // テナントには居らず、連携プロジェクトにだけ招かれた利用者
    let guest = app.insert_user(false, false).await;
    insert_project_member(&app, fx.project_id, guest.id).await;
    login(&mut app, &guest).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::OK);

    // 同じテナントの、連携していない別プロジェクトにだけ招かれた利用者は 404
    let other_project = crate::common::insert_extra_project(&app, fx.tenant_id).await;
    let other_guest = app.insert_user(false, false).await;
    insert_project_member(&app, other_project, other_guest.id).await;
    login(&mut app, &other_guest).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::NOT_FOUND);

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn later_candidate_is_tried_after_an_earlier_one_is_refused() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    // 1 件目（古い）: 継承を切ったプロジェクト。テナントメンバーは入れない
    let fx = setup(&app).await;
    stop_inheriting(&app.state.db, fx.project_id).await;
    // 2 件目: 同じテナントの、継承中の別プロジェクト。同じリポジトリを連携している
    let open_project = crate::common::insert_extra_project(&app, fx.tenant_id).await;
    insert_integration(&app, open_project, &fx.repo, fx.owner.id).await;
    mount_file(
        &mock_server,
        &fx.repo,
        FILE_PATH,
        file_body(file_text().as_bytes()),
    )
    .await;
    let url = snippet_url(REPO_OWNER, &fx.repo, SHA, FILE_PATH, "start=1");

    let member = app.insert_user(false, false).await;
    ensure_tenant_member_for_project(&app.state.db, fx.project_id, member.id).await;
    login(&mut app, &member).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::OK);

    // 対照: 2 件目も継承を切ると、どの候補にも入れず 404
    stop_inheriting(&app.state.db, open_project).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::NOT_FOUND);

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn unrelated_integrations_do_not_crowd_out_the_users_project() {
    let mock_server = github_mock().await;
    let mut app = TestApp::new_with_github().await;
    let repo = format!("repo-{}", &Uuid::new_v4().simple().to_string()[..12]);
    // 同じリポジトリを、先に他のテナントが 21 件連携している（候補の上限 20 を越える値）
    for _ in 0..21 {
        let other = app.insert_user(false, false).await;
        let tp = app.insert_tenant_project(other.id).await;
        insert_integration(&app, tp.project_id, &repo, other.id).await;
    }
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    insert_integration(&app, tp.project_id, &repo, owner.id).await;
    mount_file(
        &mock_server,
        &repo,
        FILE_PATH,
        file_body(file_text().as_bytes()),
    )
    .await;
    let url = snippet_url(REPO_OWNER, &repo, SHA, FILE_PATH, "start=1");

    login(&mut app, &owner).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::OK);

    // 対照: どのテナントにも関わりの無い利用者は 404 のまま
    let outsider = app.insert_user(false, false).await;
    login(&mut app, &outsider).await;
    assert_eq!(app.get(&url).await.status(), StatusCode::NOT_FOUND);

    forget_github_mock();
}

#[serial_test::serial]
#[tokio::test]
async fn device_token_is_accepted() {
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
    let (token, _) = service::desktop_auth::create_device_token(
        &app.state.db,
        &app.state.settings.personal_token_secret,
        fx.owner.id,
        "snippet test".into(),
        chrono::Utc::now(),
    )
    .await
    .expect("device token");

    app.reset_session_client();
    let res = app
        .get_with_bearer(
            &snippet_url(REPO_OWNER, &fx.repo, SHA, FILE_PATH, "start=1"),
            &token,
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    forget_github_mock();
}
