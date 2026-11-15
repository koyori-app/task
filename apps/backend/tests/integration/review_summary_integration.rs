// 受け入れ条件（docs/dev/features/review-findings/github-comment.md から移した。テストの無い条件はページの「未実装と既知の制限」に残す）
// - 正常系: GitHub に要約コメントが 1 本だけ作られ、既存のコメントは同じものが更新される
// - リポジトリの同定: 要約コメントは保存したリポジトリが現在の連携先と違えば投稿しない
// - 要約コメント: 連携なしプロジェクトでは投稿だけスキップして起票は成功する
// - Approve: マージ可の PR は、レビュワー本人のトークンで、レビューした commit を指定して 1 回だけ承認される（対照: High / Medium の未解決・レビュー後の push・閉じた PR では承認しない）。レビュワーが連携していなければ承認しない（bot でも承認しない）。トークンの期限が切れていれば更新してから承認し、回転したトークンを保存し直す。同じ commit への本人の承認が既にあれば、2 ページ目にあっても承認し直さない。承認が通らなくても（422 / 403）要約コメントは出て、ジョブは成功する
// - 承認の取り消し: 承認済みの PR に同じ head で High のラウンドが出ると、承認者のトークンで REQUEST_CHANGES が 1 回だけ出る（commit_id と目印付き）。承認者が前のラウンドの作成者で、High を出したのが別の人でも取り消す。取り消し済みなら重ねて出さない。head が違う・閉じた PR・承認していない・取り下げ済み・連携なしでは取り消さない。403 / 422 でも要約コメントは出てジョブは成功する
// - 変更要求: High / Medium の未解決があり head が新しい PR では、今の head にラウンドを出した人のうち自分のラウンドに未解決の High / Medium がある人の名義で、承認していなくても REQUEST_CHANGES が 1 回だけ出る（commit_id と目印付き）。承認済みなら取り消しの文面で出す。前の head の承認だけが残っていても出す。同じ commit に変更要求（目印の有無を問わない）か取り下げがあれば出さない。前の head の変更要求が残っていれば重ねない。連携なし・head が違う・閉じた PR では出さない。レビュー一覧の 2 ページ目にある変更要求も見落とさない
// - 作者が本人: PR の作者がレビュワー本人のとき、GitHub が 422 で拒むので Approve・変更要求・承認の取り消しを試みない（作者が別人なら出す）。他のレビュワーには出す。1 人の投稿が 403 / 422 で失敗しても、残りのレビュワーへ進む
// - 承認し直し: 目印付きの取り消しが最後の決定なら、マージ可に戻ったとき承認し直す。人が取り下げた承認・人の変更要求の後は承認し直さない
// - マーカーの先取り: 第三者が同じマーカーを含むコメントを先に投稿していても、それを要約コメントと誤認せず、自分のコメントを新規に作成する
// - 要約コメントの合流: 連続した状態遷移でも更新ジョブは 1 本に合流し、コメントは 1 本のまま最新の件数・マージ可否になる
// - 要約コメントの直列化: 同じ PR の更新ジョブが走っている間、後続のジョブは投稿せずに積み直す（古い状態での上書きを起こさない）
// - 持ち主が消えたときの復帰: フラグとロックに TTL が付く。古いジョブは別のジョブのロックを解放しない

use crate::common::{TEST_OAUTH_ENCRYPTION_KEY, TestApp, unique_installation_id};
use axum::http::StatusCode;
use entity::{github_integrations, oauth_connections, project_statuses};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    TransactionTrait, prelude::Uuid,
};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// レビュー要約コメント（仕様 §7）の統合テスト。
//
// 「1 PR に 1 本だけ置き、2 回目以降は同じコメントを編集する」ことと、
// 連携の無いプロジェクトでは何もしないことを、モックサーバー相手に固定する。

const REPO_OWNER: &str = "acme";
const REPO_NAME: &str = "backend";
const PR_NUMBER: i32 = 618;
const COMMENT_ID: i64 = 4242;
/// 鍵の単位に使うリポジトリ（連携先を差し替えると別の鍵になる）
const REPO_KEY: &str = "acme/backend";
/// テストの GitHub App 名（`GITHUB_APP_NAME`）から作られる bot の login
const BOT_LOGIN: &str = "task-app[bot]";
/// ラウンドが見た commit。PR メタの head と揃えると「鮮度あり」になる
const REVIEWED_HEAD: &str = "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e";

async fn mount_mocks(server: &MockServer, existing_comment: bool, marker: &str) {
    Mock::given(method("POST"))
        .and(path_regex(r"^/app/installations/\d+/access_tokens$"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "token": "ghs_test_installation_token",
            "expires_at": "2030-01-01T00:00:00Z"
        })))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{PR_NUMBER}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "title": "feat: レビュー指摘管理",
            "user": { "login": "yupix" },
            "head": { "sha": REVIEWED_HEAD },
            "state": "open"
        })))
        .mount(server)
        .await;

    // Approve: 既存のレビューは無く、投稿は通る
    Mock::given(method("GET"))
        .and(path(reviews_api_path()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(reviews_api_path()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": 7 })))
        .mount(server)
        .await;

    // 既存コメントの有無で「新規投稿」と「編集」を出し分ける
    let comments = if existing_comment {
        serde_json::json!([
            { "id": 1, "body": "無関係なコメント" },
            {
                "id": COMMENT_ID,
                "body": format!("{}\n古い要約", marker),
                "user": { "login": BOT_LOGIN, "type": "Bot" }
            },
        ])
    } else {
        serde_json::json!([{ "id": 1, "body": "無関係なコメント" }])
    };
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/{REPO_OWNER}/{REPO_NAME}/issues/{PR_NUMBER}/comments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(comments))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path(format!(
            "/repos/{REPO_OWNER}/{REPO_NAME}/issues/{PR_NUMBER}/comments"
        )))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(serde_json::json!({ "id": COMMENT_ID })),
        )
        .mount(server)
        .await;

    Mock::given(method("PATCH"))
        .and(path(format!(
            "/repos/{REPO_OWNER}/{REPO_NAME}/issues/comments/{COMMENT_ID}"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": COMMENT_ID })),
        )
        .mount(server)
        .await;
}

fn reviews_api_path() -> String {
    format!("/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{PR_NUMBER}/reviews")
}

/// PR へ送った Approve の本文と、使ったトークン（Authorization ヘッダー）を集める。
async fn approvals(server: &MockServer) -> Vec<(serde_json::Value, String)> {
    server
        .received_requests()
        .await
        .expect("received requests")
        .iter()
        .filter(|r| r.method == wiremock::http::Method::POST && r.url.path() == reviews_api_path())
        .map(|r| {
            (
                serde_json::from_slice::<serde_json::Value>(&r.body).expect("approve body"),
                r.headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

/// 指定メソッドで送られたリクエストの本文を集める。
async fn bodies_of(server: &MockServer, wanted: wiremock::http::Method) -> Vec<serde_json::Value> {
    server
        .received_requests()
        .await
        .expect("received requests")
        .iter()
        .filter(|r| r.method == wanted)
        .filter_map(|r| serde_json::from_slice::<serde_json::Value>(&r.body).ok())
        .collect()
}

async fn seed_statuses(app: &TestApp, project_id: Uuid) {
    for (name, position, is_default, is_done) in
        [("Todo", 0i16, true, false), ("Done", 1i16, false, true)]
    {
        project_statuses::ActiveModel {
            id: Set(Uuid::new_v4()),
            project_id: Set(project_id),
            name: Set(name.into()),
            color: Set("#888888".into()),
            position: Set(position),
            is_default: Set(is_default),
            is_done_state: Set(is_done),
            is_default_done: Set(is_done),
            created_at: Set(chrono::Utc::now().into()),
        }
        .insert(&app.state.db)
        .await
        .expect("insert status");
    }
}

async fn link_integration(app: &TestApp, project_id: Uuid, created_by: Uuid) {
    github_integrations::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        installation_id: Set(unique_installation_id()),
        repo_owner: Set(REPO_OWNER.into()),
        repo_name: Set(REPO_NAME.into()),
        // ジョブ側はトークンを毎回取り直すため、この列の中身は使われない
        access_token_enc: Set("unused".into()),
        token_expires_at: Set(chrono::Utc::now().into()),
        created_by: Set(created_by),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert integration");
}

/// 積まれている要約更新ジョブの件数。ペイロードは JSON なので project_id で絞る
/// （apalis.jobs.job は bytea。SeaORM の生 SQL では `?` ではなく `$N` を使う）。
async fn queued_summary_jobs(app: &TestApp, project_id: Uuid) -> i64 {
    let row = app
        .state
        .db
        .query_one_raw(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            "SELECT COUNT(*) AS count FROM apalis.jobs \
             WHERE job_type = $1 AND convert_from(job, 'UTF8') LIKE $2",
            [
                job::review_summary::QUEUE_NAME.into(),
                format!("%{project_id}%").into(),
            ],
        ))
        .await
        .expect("count review summary jobs")
        .expect("count row");
    row.try_get::<i64>("", "count").expect("count column")
}

fn job_state(app: &TestApp) -> job::JobState {
    job::JobState {
        settings: app.state.settings.clone(),
        db: app.state.db.clone(),
        redis_client: app.state.redis_client.clone(),
        smtp_client: app.state.smtp_client.clone(),
        http_client: app.state.http_client.clone(),
        review_summary_storage: app.state.review_summary_storage.clone(),
        github_issue_sync_storage: app.state.github_issue_sync_storage.clone(),
        pg_pool: app.state.pg_pool.clone(),
        oauth_settings: app.state.oauth_settings.clone(),
    }
}

// serial: GITHUB_API_BASE_URL を差し替えるため、他の GitHub テストと並列に走らせない。
#[serial_test::serial]
#[tokio::test]
async fn review_summary_comment_is_created_once_and_then_edited() {
    let mock_server = MockServer::start().await;
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    let mut app = TestApp::new_with_github().await;
    let reviewer = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;
    seed_statuses(&app, tp.project_id).await;
    link_integration(&app, tp.project_id, reviewer.id).await;
    let marker = service::github::pr_comments::summary_marker(tp.project_id);
    mount_mocks(&mock_server, false, &marker).await;

    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;

    let reviews_path = format!(
        "/v1/tenants/{}/projects/{}/reviews",
        tp.tenant_id, tp.project_id
    );
    let res = app
        .post_json_with_session(
            &reviews_path,
            serde_json::json!({
                "pr_number": PR_NUMBER,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "summary": "総評",
                "findings": [
                    { "severity": "high", "title": "認可漏れ", "body": "本文" },
                    { "severity": "low", "title": "命名", "body": "本文" },
                ],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // ワーカーはテストハーネスで動かさないので、ジョブ本体を直接呼ぶ
    let state = job_state(&app);
    job::review_summary::process(
        job::ReviewSummaryJob {
            project_id: tp.project_id,
            pr_number: PR_NUMBER,
            repo_owner: REPO_OWNER.into(),
            repo_name: REPO_NAME.into(),
        },
        apalis::prelude::Data::new(state.clone()),
    )
    .await
    .expect("post review summary");

    // 既存コメントが無いので新規投稿。本文の中身は service::reviews のユニットが見る
    let posted = bodies_of(&mock_server, wiremock::http::Method::POST).await;
    let posted: Vec<&serde_json::Value> =
        posted.iter().filter(|b| b.get("body").is_some()).collect();
    assert_eq!(posted.len(), 1, "要約コメントは 1 本だけ作る");

    // PR メタ（タイトル・作者）がラウンドにキャッシュされる
    let rounds = app
        .get_with_session(&format!("{reviews_path}?pr={PR_NUMBER}"))
        .await
        .json::<serde_json::Value>()
        .await
        .expect("rounds json");
    assert_eq!(rounds[0]["pr_title"], "feat: レビュー指摘管理");
    assert_eq!(rounds[0]["pr_author"], "yupix");

    app.cleanup_user(reviewer.id).await;
}

/// 2 回目以降は POST せず、同じコメントを PATCH で更新する。
#[serial_test::serial]
#[tokio::test]
async fn an_existing_summary_comment_is_updated_in_place() {
    let mock_server = MockServer::start().await;
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    let mut app = TestApp::new_with_github().await;
    let reviewer = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;
    seed_statuses(&app, tp.project_id).await;
    link_integration(&app, tp.project_id, reviewer.id).await;
    let marker = service::github::pr_comments::summary_marker(tp.project_id);
    mount_mocks(&mock_server, true, &marker).await;

    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;

    let res = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({
                "pr_number": PR_NUMBER,
                "head_sha": REVIEWED_HEAD,
                "summary": "指摘なし",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    job::review_summary::process(
        job::ReviewSummaryJob {
            project_id: tp.project_id,
            pr_number: PR_NUMBER,
            repo_owner: REPO_OWNER.into(),
            repo_name: REPO_NAME.into(),
        },
        apalis::prelude::Data::new(job_state(&app)),
    )
    .await
    .expect("update review summary");

    let posted = bodies_of(&mock_server, wiremock::http::Method::POST).await;
    assert!(
        posted.iter().all(|b| b.get("body").is_none()),
        "既存コメントがあれば新規投稿しない（PR にコメントを積み上げない）"
    );

    let patched = bodies_of(&mock_server, wiremock::http::Method::PATCH).await;
    assert_eq!(patched.len(), 1, "同じコメントを 1 回だけ編集する");

    app.cleanup_user(reviewer.id).await;
}

/// 連続した状態遷移でも更新ジョブは 1 本に合流し、コメントは 1 本のまま最新になる。
///
/// 遷移のたびにジョブを積むと、同じコメントへ短時間に何度も書き込みに行き、
/// GitHub の secondary rate limit に当たる（仕様 §7）。
#[serial_test::serial]
#[tokio::test]
async fn consecutive_transitions_coalesce_into_one_summary_update() {
    let mock_server = MockServer::start().await;
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    let mut app = TestApp::new_with_github().await;
    let reviewer = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;
    seed_statuses(&app, tp.project_id).await;
    link_integration(&app, tp.project_id, reviewer.id).await;
    let marker = service::github::pr_comments::summary_marker(tp.project_id);
    mount_mocks(&mock_server, false, &marker).await;

    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;

    let reviews_path = format!(
        "/v1/tenants/{}/projects/{}/reviews",
        tp.tenant_id, tp.project_id
    );
    let res = app
        .post_json_with_session(
            &reviews_path,
            serde_json::json!({
                "pr_number": PR_NUMBER,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "summary": "総評",
                "findings": [
                    { "severity": "high", "title": "認可漏れ", "body": "本文" },
                    { "severity": "high", "title": "検証漏れ", "body": "本文" },
                    { "severity": "medium", "title": "境界値", "body": "本文" },
                ],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let finding_ids: Vec<String> = res.json::<serde_json::Value>().await.expect("json")["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|f| f["id"].as_str().expect("finding id").to_string())
        .collect();

    // 起票で 1 本積まれている。ここから 3 件を続けて fixed にしても増えない
    let findings_path = format!(
        "/v1/tenants/{}/projects/{}/review-findings",
        tp.tenant_id, tp.project_id
    );
    for finding_id in &finding_ids {
        let res = app
            .patch_json_with_session(
                &format!("{findings_path}/{finding_id}"),
                serde_json::json!({ "state": "fixed" }),
            )
            .await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    assert!(
        !service::github::review_summary_queue::try_mark_pending(
            &app.state.redis_client,
            tp.project_id,
            REPO_KEY,
            PR_NUMBER
        )
        .await
        .expect("pending flag"),
        "起票と 3 件の遷移は 1 本の更新に合流する"
    );

    // 積まれている 1 本を走らせると、最新状態（3 件とも fixed）で 1 回だけ投稿する
    let state = job_state(&app);
    job::review_summary::process(
        job::ReviewSummaryJob {
            project_id: tp.project_id,
            pr_number: PR_NUMBER,
            repo_owner: REPO_OWNER.into(),
            repo_name: REPO_NAME.into(),
        },
        apalis::prelude::Data::new(state.clone()),
    )
    .await
    .expect("post review summary");

    let posted = bodies_of(&mock_server, wiremock::http::Method::POST).await;
    let posted: Vec<&serde_json::Value> =
        posted.iter().filter(|b| b.get("body").is_some()).collect();
    assert_eq!(posted.len(), 1, "書き込みは 1 回だけ");
    let body = posted[0]["body"].as_str().expect("comment body");
    assert!(
        body.contains("| High | Fixed | 2 |"),
        "最新の件数が出る: {body}"
    );
    assert!(
        body.contains("| Medium | Fixed | 1 |"),
        "最新の件数が出る: {body}"
    );
    assert!(
        body.contains("マージ不可"),
        "fixed は未解決として数える: {body}"
    );

    // ジョブが走ったので、次の遷移はまた積める
    assert!(
        service::github::review_summary_queue::try_mark_pending(
            &app.state.redis_client,
            tp.project_id,
            REPO_KEY,
            PR_NUMBER
        )
        .await
        .expect("pending flag"),
        "ジョブの実行後は次の更新を受け付ける"
    );

    app.cleanup_user(reviewer.id).await;
}

/// PR の作者は、要約ジョブが `pr_author` を埋めたときに 1 度だけ通知を受ける。
///
/// 起票時点では作者が分からない（このジョブが GitHub から取ってくる）ので、
/// `create_review` の宛先には入らない。ジョブを何度走らせても増えないことも固定する。
/// 起票トランザクションとジョブが重なっても、確定したラウンドを取りこぼさない。
#[serial_test::serial]
#[tokio::test]
async fn the_pull_request_author_is_notified_once() {
    let mock_server = MockServer::start().await;
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    let mut app = TestApp::new_with_github().await;
    let reviewer = app.insert_user_default().await;
    let author = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;
    seed_statuses(&app, tp.project_id).await;
    link_integration(&app, tp.project_id, reviewer.id).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, author.id).await;

    // モックの PR 作者（`yupix`）を Task の利用者に結ぶ接続
    let now = chrono::Utc::now();
    entity::oauth_connections::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(author.id),
        provider: Set("github".into()),
        provider_user_id: Set(format!("gh-{}", author.id)),
        provider_email: Set(None),
        instance_url: Set(None),
        access_token_enc: Set(None),
        refresh_token_enc: Set(None),
        token_expires_at: Set(None),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        provider_login: Set(Some("yupix".into())),
    }
    .insert(&app.state.db)
    .await
    .expect("insert oauth connection");

    let marker = service::github::pr_comments::summary_marker(tp.project_id);
    mount_mocks(&mock_server, false, &marker).await;

    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;
    let res = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({
                "pr_number": PR_NUMBER,
                "head_sha": REVIEWED_HEAD,
                "summary": "総評",
                "findings": [{ "severity": "high", "title": "認可漏れ", "body": "本文" }],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(
        author_notifications(&app, author.id).await,
        0,
        "起票の時点では作者が分からない"
    );

    // 起票と同じロックを保持し、ジョブが待機してから R2 を確定する。
    let txn = app.state.db.begin().await.unwrap();
    let repo = service::reviews::current_repo(&txn, tp.project_id)
        .await
        .unwrap();
    let round = service::reviews::next_round(&txn, tp.project_id, &repo, PR_NUMBER)
        .await
        .unwrap();
    assert_eq!(round, 2);
    let pid = txn
        .query_one_raw(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            "SELECT pg_backend_pid() AS pid",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i32>("", "pid")
        .unwrap();
    let job = job::ReviewSummaryJob {
        project_id: tp.project_id,
        pr_number: PR_NUMBER,
        repo_owner: REPO_OWNER.into(),
        repo_name: REPO_NAME.into(),
    };
    let state = job_state(&app);
    let processing = tokio::spawn(async move {
        job::review_summary::process(job, apalis::prelude::Data::new(state)).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blocked = app.state.db.query_one_raw(sea_orm::Statement::from_sql_and_values(
                sea_orm::DatabaseBackend::Postgres,
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))) AS blocked",
                [pid.into()],
            )).await.unwrap().unwrap().try_get::<bool>("", "blocked").unwrap();
            if blocked { break; }
            assert!(!processing.is_finished(), "ジョブは起票の確定を待つ必要がある");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.expect("summary job waits for round creation");
    let first = entity::reviews::Entity::find()
        .filter(entity::reviews::Column::ProjectId.eq(tp.project_id))
        .one(&txn)
        .await
        .unwrap()
        .unwrap();
    let mut concurrent: entity::reviews::ActiveModel = first.into();
    concurrent.id = Set(Uuid::new_v4());
    concurrent.round = Set(round);
    concurrent.summary = Set("並行起票".into());
    let concurrent = concurrent.insert(&txn).await.unwrap();
    service::notifications::notify_review_round_created(&txn, &concurrent, &[], reviewer.id)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    processing.await.unwrap().expect("post review summary");
    let mut notified_rounds: Vec<i64> = entity::notifications::Entity::find()
        .filter(entity::notifications::Column::UserId.eq(author.id))
        .filter(
            entity::notifications::Column::NotificationType
                .eq(::common::notifications::TYPE_REVIEW_ROUND_CREATED),
        )
        .all(&app.state.db)
        .await
        .unwrap()
        .into_iter()
        .map(|notification| notification.payload["round"].as_i64().unwrap())
        .collect();
    notified_rounds.sort();
    assert_eq!(
        notified_rounds,
        vec![1, 2],
        "並行起票したラウンドも最初のジョブで通知する"
    );
    job::review_summary::process(
        job::ReviewSummaryJob {
            project_id: tp.project_id,
            pr_number: PR_NUMBER,
            repo_owner: REPO_OWNER.into(),
            repo_name: REPO_NAME.into(),
        },
        apalis::prelude::Data::new(job_state(&app)),
    )
    .await
    .expect("repeat review summary");
    assert_eq!(
        author_notifications(&app, author.id).await,
        2,
        "再実行しても各ラウンドの作者通知は 1 件だけ"
    );

    // 作者を解決できる R3 は起票時に通知される。要約ジョブで重複させない。
    let res = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({
                "pr_number": PR_NUMBER,
                "head_sha": REVIEWED_HEAD,
                "summary": "再確認",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(author_notifications(&app, author.id).await, 3);
    for _ in 0..2 {
        job::review_summary::process(
            job::ReviewSummaryJob {
                project_id: tp.project_id,
                pr_number: PR_NUMBER,
                repo_owner: REPO_OWNER.into(),
                repo_name: REPO_NAME.into(),
            },
            apalis::prelude::Data::new(job_state(&app)),
        )
        .await
        .expect("post third round summary");
        assert_eq!(
            author_notifications(&app, author.id).await,
            3,
            "起票と要約ジョブを通しても、各ラウンドの作者通知は 1 件だけ"
        );
    }

    app.cleanup_user(author.id).await;
    app.cleanup_user(reviewer.id).await;
}

/// その利用者に届いたラウンド起票の通知の件数。
async fn author_notifications(app: &TestApp, user_id: Uuid) -> u64 {
    use sea_orm::PaginatorTrait;
    entity::notifications::Entity::find()
        .filter(entity::notifications::Column::UserId.eq(user_id))
        .filter(
            entity::notifications::Column::NotificationType
                .eq(::common::notifications::TYPE_REVIEW_ROUND_CREATED),
        )
        .count(&app.state.db)
        .await
        .expect("count notifications")
}

/// 担い手が消えても詰まらないよう、印とロックは TTL 付きで、
/// ロックの解放は取得時のトークンと一致するときだけ効く（仕様 §7）。
#[tokio::test]
async fn the_pending_flag_and_lock_expire_and_release_is_owner_checked() {
    use service::github::review_summary_queue as queue;

    let app = TestApp::new().await;
    let redis = &app.state.redis_client;
    // Redis だけを使うので、実在するプロジェクトでなくてよい
    let project_id = Uuid::new_v4();
    let pr = 618;

    assert!(
        queue::try_mark_pending(redis, project_id, REPO_KEY, pr)
            .await
            .expect("mark pending")
    );
    let token = queue::try_acquire_update_lock(redis, project_id, REPO_KEY, pr)
        .await
        .expect("acquire lock")
        .expect("空いていれば取れる");

    // ワーカーが落ちても、どちらの目印も期限で必ず明ける
    let (pending_ttl, lock_ttl) = queue::remaining_ttl_secs(redis, project_id, REPO_KEY, pr)
        .await
        .expect("ttl");
    assert!(
        pending_ttl > 0 && pending_ttl <= queue::SUMMARY_PENDING_TTL_SECS as i64,
        "印に TTL が付いている: {pending_ttl}"
    );
    assert!(
        lock_ttl > 0 && lock_ttl <= queue::SUMMARY_LOCK_TTL_SECS as i64,
        "ロックに TTL が付いている: {lock_ttl}"
    );

    // 保持中は取れない
    assert!(
        queue::try_acquire_update_lock(redis, project_id, REPO_KEY, pr)
            .await
            .expect("acquire lock")
            .is_none(),
        "同じ PR の更新は同時に走らない"
    );

    // 期限切れで別のジョブが取り直した状況を作る
    assert!(
        queue::release_update_lock(redis, project_id, REPO_KEY, pr, &token)
            .await
            .expect("release lock")
    );
    let newer = queue::try_acquire_update_lock(redis, project_id, REPO_KEY, pr)
        .await
        .expect("acquire lock")
        .expect("解放後は取り直せる");

    // 遅れて完走した古いジョブは、取り直されたロックを解放しない
    assert!(
        !queue::release_update_lock(redis, project_id, REPO_KEY, pr, &token)
            .await
            .expect("release lock"),
        "古いトークンでは解放できない"
    );
    assert!(
        queue::try_acquire_update_lock(redis, project_id, REPO_KEY, pr)
            .await
            .expect("acquire lock")
            .is_none(),
        "取り直したロックはそのまま残る"
    );

    queue::release_update_lock(redis, project_id, REPO_KEY, pr, &newer)
        .await
        .expect("release lock");
    queue::clear_pending(redis, project_id, REPO_KEY, pr)
        .await
        .expect("clear pending");
    assert!(
        queue::try_mark_pending(redis, project_id, REPO_KEY, pr)
            .await
            .expect("mark pending"),
        "印を落とせば次の更新をまた積める"
    );
    queue::clear_pending(redis, project_id, REPO_KEY, pr)
        .await
        .expect("clear pending");
}

/// 同じ PR を更新中のジョブがいる間は投稿せず、少し後ろへ積み直す。
///
/// 合流はジョブの本数を減らすだけで同時実行は止まらない。並行して走ると、古い状態を
/// 読んだ側の書き込みが後から着いてコメントが巻き戻る（仕様 §7）。
///
/// 「自分の番ではない」をジョブの失敗として返すと、`RetryPolicy` にバックオフが無い
/// ぶん数ミリ秒で試行回数を使い切って終端する。そのとき「更新待ち」の印だけが残り、
/// 生きたジョブが 1 本も無いまま、印の TTL のあいだ以降の遷移が合流で捨てられる。
#[serial_test::serial]
#[tokio::test]
async fn a_concurrent_summary_update_is_retried_instead_of_overwriting() {
    let mock_server = MockServer::start().await;
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    let mut app = TestApp::new_with_github().await;
    let reviewer = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;
    seed_statuses(&app, tp.project_id).await;
    link_integration(&app, tp.project_id, reviewer.id).await;
    let marker = service::github::pr_comments::summary_marker(tp.project_id);
    mount_mocks(&mock_server, true, &marker).await;

    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;

    let res = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({
                "pr_number": PR_NUMBER,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "summary": "総評",
                "findings": [{ "severity": "high", "title": "認可漏れ", "body": "本文" }],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // 先行するジョブがロックを握っている状態を作る
    let held = service::github::review_summary_queue::try_acquire_update_lock(
        &app.state.redis_client,
        tp.project_id,
        REPO_KEY,
        PR_NUMBER,
    )
    .await
    .expect("acquire lock")
    .expect("先行ジョブがロックを取れる");

    let state = job_state(&app);
    let job = job::ReviewSummaryJob {
        project_id: tp.project_id,
        pr_number: PR_NUMBER,
        repo_owner: REPO_OWNER.into(),
        repo_name: REPO_NAME.into(),
    };
    let queued_before = queued_summary_jobs(&app, tp.project_id).await;
    job::review_summary::process(job.clone(), apalis::prelude::Data::new(state.clone()))
        .await
        .expect("自分の番でないだけなので失敗にしない（失敗にすると即時再試行を使い切る）");
    assert!(
        bodies_of(&mock_server, wiremock::http::Method::PATCH)
            .await
            .is_empty(),
        "ロックを取れなかったジョブは投稿しない"
    );
    assert_eq!(
        queued_summary_jobs(&app, tp.project_id).await,
        queued_before + 1,
        "拾い直すジョブを積み直す（積まないと、この更新はどこにも残らない）"
    );
    assert!(
        !service::github::review_summary_queue::try_mark_pending(
            &app.state.redis_client,
            tp.project_id,
            REPO_KEY,
            PR_NUMBER
        )
        .await
        .expect("pending flag"),
        "積み直したジョブに合流させるため「更新待ち」の印は立っている"
    );

    // 先行ジョブが終われば、次のジョブが最新状態で更新できる
    service::github::review_summary_queue::release_update_lock(
        &app.state.redis_client,
        tp.project_id,
        REPO_KEY,
        PR_NUMBER,
        &held,
    )
    .await
    .expect("release lock");

    job::review_summary::process(job, apalis::prelude::Data::new(state))
        .await
        .expect("post review summary");
    let patched = bodies_of(&mock_server, wiremock::http::Method::PATCH).await;
    assert_eq!(patched.len(), 1, "解放後は 1 回だけ更新する");
    assert!(
        patched[0]["body"]
            .as_str()
            .expect("comment body")
            .contains("| High | Open | 1 |"),
        "最新の件数が出る: {}",
        patched[0]["body"]
    );

    app.cleanup_user(reviewer.id).await;
}

/// 第三者が同じマーカーのコメントを先に置いていても、それを更新しに行かない。
///
/// マーカーは PR の参加者なら誰でも本文に書ける。App は他人のコメントを編集できないので、
/// 掴んでしまうと更新が失敗し続け、失敗はベストエフォートで握り潰されるため正式な要約が
/// 永久に作られない（仕様 §7）。
#[serial_test::serial]
#[tokio::test]
async fn a_third_party_marker_does_not_hijack_the_summary_comment() {
    let mock_server = MockServer::start().await;
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }

    let mut app = TestApp::new_with_github().await;
    let reviewer = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;
    seed_statuses(&app, tp.project_id).await;
    link_integration(&app, tp.project_id, reviewer.id).await;
    let marker = service::github::pr_comments::summary_marker(tp.project_id);

    // 共通のモックを敷いたうえで、一覧だけ「第三者が先取りしたコメント」に差し替える
    mount_mocks(&mock_server, false, &marker).await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/{REPO_OWNER}/{REPO_NAME}/issues/{PR_NUMBER}/comments"
        )))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": 999,
                "body": format!("{}\nにせの要約", marker),
                "user": { "login": "someone", "type": "User" }
            }])),
        )
        .mount(&mock_server)
        .await;

    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;
    let res = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({
                "pr_number": PR_NUMBER,
                "head_sha": REVIEWED_HEAD,
                "summary": "総評",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let state = job_state(&app);
    job::review_summary::process(
        job::ReviewSummaryJob {
            project_id: tp.project_id,
            pr_number: PR_NUMBER,
            repo_owner: REPO_OWNER.into(),
            repo_name: REPO_NAME.into(),
        },
        apalis::prelude::Data::new(state),
    )
    .await
    .expect("post review summary");

    // 第三者のコメント（999）を編集しに行かず、自分のコメントを新規に作る
    let patched = bodies_of(&mock_server, wiremock::http::Method::PATCH).await;
    assert!(
        patched.is_empty(),
        "他人のコメントを更新しようとしない: {patched:?}"
    );
    let posted = bodies_of(&mock_server, wiremock::http::Method::POST).await;
    let posted: Vec<&serde_json::Value> =
        posted.iter().filter(|b| b.get("body").is_some()).collect();
    assert_eq!(posted.len(), 1, "自分の要約を新規に作る");

    app.cleanup_user(reviewer.id).await;
}

/// GitHub 連携の無いプロジェクトでは投稿しない（起票・管理自体は task 側で完結する）。
#[serial_test::serial]
#[tokio::test]
async fn a_project_without_integration_skips_the_comment() {
    let mock_server = MockServer::start().await;
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    let mut app = TestApp::new_with_github().await;
    let reviewer = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;
    seed_statuses(&app, tp.project_id).await;
    // 連携は張らない
    let marker = service::github::pr_comments::summary_marker(tp.project_id);
    mount_mocks(&mock_server, false, &marker).await;

    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;

    let res = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({
                "pr_number": PR_NUMBER,
                "head_sha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "起票自体は成功する");

    job::review_summary::process(
        job::ReviewSummaryJob {
            project_id: tp.project_id,
            pr_number: PR_NUMBER,
            // 連携が無いプロジェクトのラウンドは、控えも空文字列になる
            repo_owner: String::new(),
            repo_name: String::new(),
        },
        apalis::prelude::Data::new(job_state(&app)),
    )
    .await
    .expect("job succeeds without an integration");

    let requests = mock_server
        .received_requests()
        .await
        .expect("received requests");
    assert!(
        requests.is_empty(),
        "連携が無ければ GitHub API を叩かない: {} 件",
        requests.len()
    );

    app.cleanup_user(reviewer.id).await;
}

/// 投入から実行までの間に連携先が差し替わったら、投稿せず投入時の印だけを落とす。
///
/// 印は投入側がラウンドの見たリポジトリで立てる。実行側が現在の連携先で鍵を作ると
/// 別のキーを消しに行き、元の印が TTL のあいだ残る。その間、そのリポジトリの遷移は
/// 合流で捨てられ、コメントが古いまま止まる（仕様 §7）。
#[serial_test::serial]
#[tokio::test]
async fn a_job_enqueued_for_another_repository_clears_its_own_pending_flag() {
    let mock_server = MockServer::start().await;
    // SAFETY: serial アトリビュートにより他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
    }
    let mut app = TestApp::new_with_github().await;
    let reviewer = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;
    seed_statuses(&app, tp.project_id).await;
    link_integration(&app, tp.project_id, reviewer.id).await;
    let marker = service::github::pr_comments::summary_marker(tp.project_id);
    mount_mocks(&mock_server, false, &marker).await;

    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;

    let res = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({
                "pr_number": PR_NUMBER,
                "head_sha": REVIEWED_HEAD,
                "summary": "総評",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    // 起票が立てた印（acme/backend）が残っている状態を作る
    const OTHER_REPO_KEY: &str = "acme/frontend";
    service::github::review_summary_queue::try_mark_pending(
        &app.state.redis_client,
        tp.project_id,
        REPO_KEY,
        PR_NUMBER,
    )
    .await
    .expect("mark pending");

    // 実行前に連携先を差し替える
    github_integrations::Entity::update_many()
        .col_expr(
            github_integrations::Column::RepoName,
            sea_orm::sea_query::Expr::value("frontend"),
        )
        .filter(github_integrations::Column::ProjectId.eq(tp.project_id))
        .exec(&app.state.db)
        .await
        .expect("relink integration");

    let posted_before = mock_server
        .received_requests()
        .await
        .expect("received requests")
        .len();

    job::review_summary::process(
        job::ReviewSummaryJob {
            project_id: tp.project_id,
            pr_number: PR_NUMBER,
            repo_owner: REPO_OWNER.into(),
            repo_name: REPO_NAME.into(),
        },
        apalis::prelude::Data::new(job_state(&app)),
    )
    .await
    .expect("差し替えは失敗ではない（このジョブの出番が終わっただけ）");

    // 旧リポジトリ向けの内容を新リポジトリへ投稿しない
    assert_eq!(
        mock_server
            .received_requests()
            .await
            .expect("received requests")
            .len(),
        posted_before,
        "連携が差し替わっていたら GitHub を叩かない"
    );

    // 投入時のリポジトリの印を落とす（残すと以降の遷移が合流で捨てられる）
    assert!(
        service::github::review_summary_queue::try_mark_pending(
            &app.state.redis_client,
            tp.project_id,
            REPO_KEY,
            PR_NUMBER,
        )
        .await
        .expect("pending flag"),
        "投入時のリポジトリの印が落ちている（次の遷移が積める）"
    );

    // 新しい連携先の印には触らない（そちらの遷移が自前で積む）
    assert!(
        service::github::review_summary_queue::try_mark_pending(
            &app.state.redis_client,
            tp.project_id,
            OTHER_REPO_KEY,
            PR_NUMBER,
        )
        .await
        .expect("pending flag"),
        "差し替え先の印は立てていない"
    );

    app.cleanup_user(reviewer.id).await;
}

// ── Approve（仕様 §7）─────────────────────────────────────────────────────

/// レビュワーの GitHub アカウント（承認用の連携）。
const REVIEWER_GITHUB_ID: i64 = 9001;
const REVIEWER_TOKEN: &str = "ghu_reviewer_token";
const REVIEWER_REFRESH_TOKEN: &str = "ghr_reviewer_refresh";

/// レビュワーの承認用の連携。`expires_in_secs` が負なら期限切れのトークンを持たせる。
#[derive(Clone, Copy)]
struct ReviewerLink {
    expires_in_secs: i64,
}

const LINKED: Option<ReviewerLink> = Some(ReviewerLink {
    expires_in_secs: 8 * 60 * 60,
});

/// 連携済みプロジェクトに `findings` のラウンドを 1 本起票し、要約ジョブを 1 回回す。
/// レビュワーの承認用の連携（`link`）があれば作っておき、実行後の連携の行を返す。
///
/// 呼ぶ前に `with_priority(1)` のモックを足せば、`mount_mocks` の既定応答を上書きできる。
/// Approve の失敗でジョブを失敗にしないことも、ここの `expect` で毎回確かめる。
async fn review_once_and_run_summary(
    mock_server: &MockServer,
    findings: serde_json::Value,
    link: Option<ReviewerLink>,
) -> Option<oauth_connections::Model> {
    scenario(mock_server, findings, link, Scenario::default()).await
}

/// (取り消しの目印, 呼び出しの Bearer トークン) から GET reviews の応答を作る
type ReviewsFn = Box<dyn Fn(&str, &str) -> serde_json::Value + Send + Sync>;

/// [`review_once_and_run_summary`] に足す設定。
#[derive(Default)]
struct Scenario {
    /// 目印（取り消しのレビューに付く）を受け取って、GET reviews の応答を差し替える
    reviews: Option<ReviewsFn>,
    /// 同じ head への 2 本目のラウンド。`by_other` なら別のレビュワー（連携済み）が出す
    second_round: Option<(serde_json::Value, bool)>,
    /// 1 本目のラウンドがレビューした commit（既定は今の head の `REVIEWED_HEAD`）。
    /// 前の head を見た人のラウンドを作るのに使う
    first_head: Option<&'static str>,
}

const OTHER_GITHUB_ID: i64 = 9002;
const OTHER_TOKEN: &str = "ghu_other_token";

async fn link_reviewer(app: &TestApp, user_id: Uuid, github_id: i64, token: &str) {
    let enc = |plain: &str| {
        auth_core::crypto::encrypt_token(TEST_OAUTH_ENCRYPTION_KEY, plain).expect("encrypt")
    };
    oauth_connections::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        provider: Set(service::oauth::GITHUB_APP_PROVIDER.into()),
        provider_user_id: Set(github_id.to_string()),
        provider_login: Set(Some(format!("reviewer-{github_id}"))),
        access_token_enc: Set(Some(enc(token))),
        refresh_token_enc: Set(Some(enc(REVIEWER_REFRESH_TOKEN))),
        token_expires_at: Set(Some(
            (chrono::Utc::now() + chrono::Duration::hours(8)).into(),
        )),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    }
    .insert(&app.state.db)
    .await
    .expect("insert github_app connection");
}

async fn scenario(
    mock_server: &MockServer,
    findings: serde_json::Value,
    link: Option<ReviewerLink>,
    opts: Scenario,
) -> Option<oauth_connections::Model> {
    // SAFETY: 呼び出し元のテストは serial で、他テストとの並列実行を防いでいる。
    unsafe {
        std::env::set_var("GITHUB_API_BASE_URL", mock_server.uri());
        std::env::set_var("GITHUB_OAUTH_BASE_URL", mock_server.uri());
    }
    let mut app = TestApp::new_with_github().await;
    let reviewer = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;
    seed_statuses(&app, tp.project_id).await;
    link_integration(&app, tp.project_id, reviewer.id).await;
    let marker = service::github::pr_comments::summary_marker(tp.project_id);
    mount_mocks(mock_server, false, &marker).await;
    if let Some(reviews) = opts.reviews {
        let revoke_marker = service::github::pr_comments::revoke_marker(tp.project_id);
        Mock::given(method("GET"))
            .and(path(reviews_api_path()))
            .respond_with(move |req: &wiremock::Request| {
                let token = req
                    .headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("Bearer "))
                    .unwrap_or_default()
                    .to_string();
                ResponseTemplate::new(200).set_body_json(reviews(&revoke_marker, &token))
            })
            .with_priority(1)
            .mount(mock_server)
            .await;
    }

    if let Some(link) = link {
        link_reviewer(&app, reviewer.id, REVIEWER_GITHUB_ID, REVIEWER_TOKEN).await;
        if link.expires_in_secs != 8 * 60 * 60 {
            let conn = oauth_connections::Entity::find()
                .filter(oauth_connections::Column::UserId.eq(reviewer.id))
                .one(&app.state.db)
                .await
                .expect("load")
                .expect("connection");
            let mut active: oauth_connections::ActiveModel = conn.into();
            active.token_expires_at = Set(Some(
                (chrono::Utc::now() + chrono::Duration::seconds(link.expires_in_secs)).into(),
            ));
            active.update(&app.state.db).await.expect("set expiry");
        }
    }

    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;
    let res = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({
                "pr_number": PR_NUMBER,
                "head_sha": opts.first_head.unwrap_or(REVIEWED_HEAD),
                "summary": "総評",
                "findings": findings,
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let mut other_id = None;
    if let Some((findings2, by_other)) = opts.second_round {
        if by_other {
            let other = app.insert_user_default().await;
            link_reviewer(&app, other.id, OTHER_GITHUB_ID, OTHER_TOKEN).await;
            let now = chrono::Utc::now();
            let review = entity::reviews::ActiveModel {
                id: Set(Uuid::new_v4()),
                project_id: Set(tp.project_id),
                integration_id: Set(None),
                repo_owner: Set(REPO_OWNER.into()),
                repo_name: Set(REPO_NAME.into()),
                pr_number: Set(PR_NUMBER),
                round: Set(2),
                head_sha: Set(REVIEWED_HEAD.into()),
                reviewer_id: Set(other.id),
                summary: Set("別の人の総評".into()),
                pr_title: Set(None),
                pr_author: Set(None),
                pr_head_sha: Set(None),
                pr_head_checked_at: Set(None),
                summary_comment_id: Set(None),
                created_at: Set(now.into()),
            }
            .insert(&app.state.db)
            .await
            .expect("insert second round");
            for f in findings2.as_array().expect("findings") {
                entity::review_findings::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    review_id: Set(review.id),
                    severity: Set(entity::review_findings::FindingSeverity::High),
                    title: Set(f["title"].as_str().unwrap_or("t").into()),
                    body: Set("本文".into()),
                    file: Set(None),
                    line: Set(None),
                    state: Set(entity::review_findings::FindingState::Open),
                    deferred_task_id: Set(None),
                    fixed_by: Set(None),
                    created_at: Set(now.into()),
                    updated_at: Set(now.into()),
                }
                .insert(&app.state.db)
                .await
                .expect("insert second finding");
            }
            other_id = Some(other.id);
        } else {
            let res = app
                .post_json_with_session(
                    &format!(
                        "/v1/tenants/{}/projects/{}/reviews",
                        tp.tenant_id, tp.project_id
                    ),
                    serde_json::json!({
                        "pr_number": PR_NUMBER,
                        "head_sha": REVIEWED_HEAD,
                        "summary": "2 本目",
                        "findings": findings2,
                    }),
                )
                .await;
            assert_eq!(res.status(), StatusCode::CREATED);
        }
    }

    job::review_summary::process(
        job::ReviewSummaryJob {
            project_id: tp.project_id,
            pr_number: PR_NUMBER,
            repo_owner: REPO_OWNER.into(),
            repo_name: REPO_NAME.into(),
        },
        apalis::prelude::Data::new(job_state(&app)),
    )
    .await
    .expect("Approve の成否に関係なくジョブは成功する");

    let connection = oauth_connections::Entity::find()
        .filter(oauth_connections::Column::UserId.eq(reviewer.id))
        .filter(oauth_connections::Column::Provider.eq(service::oauth::GITHUB_APP_PROVIDER))
        .one(&app.state.db)
        .await
        .expect("load github_app connection");
    app.cleanup_user(reviewer.id).await;
    // 別のレビュワーのラウンドは、プロジェクトごと消えた後でないと消せない
    if let Some(id) = other_id {
        app.cleanup_user(id).await;
    }
    connection
}

/// マージ可（High / Medium の未解決なし・レビューした commit が現在の head）なら、
/// レビュワー本人のトークンで、その commit を指定して Approve する。
/// Low の未解決はマージを止めない。
#[serial_test::serial]
#[tokio::test]
async fn a_merge_ready_pull_request_is_approved_by_the_reviewer() {
    let mock_server = MockServer::start().await;
    review_once_and_run_summary(
        &mock_server,
        serde_json::json!([{ "severity": "low", "title": "命名", "body": "本文" }]),
        LINKED,
    )
    .await;

    assert_eq!(
        approvals(&mock_server).await,
        vec![(
            serde_json::json!({ "event": "APPROVE", "commit_id": REVIEWED_HEAD }),
            format!("Bearer {REVIEWER_TOKEN}"),
        )],
        "レビューした commit を、App ではなくレビュワーのトークンで 1 回だけ承認する"
    );
}

/// レビュワーが承認用の連携をしていなければ承認しない（bot でも代わりに承認しない）。
/// 要約コメントは出て、ジョブは成功する。
#[serial_test::serial]
#[tokio::test]
async fn a_reviewer_without_the_link_does_not_approve() {
    let mock_server = MockServer::start().await;
    review_once_and_run_summary(&mock_server, serde_json::json!([]), None).await;

    assert!(approvals(&mock_server).await.is_empty());
    let comments: Vec<serde_json::Value> = bodies_of(&mock_server, wiremock::http::Method::POST)
        .await
        .into_iter()
        .filter(|b| b.get("body").is_some())
        .collect();
    assert_eq!(comments.len(), 1, "要約コメントは出ている");
}

/// トークンの期限が切れていれば、リフレッシュトークンで更新してから承認し、
/// 回転した新しいトークンを保存し直す（古いリフレッシュトークンは GitHub 側で無効になる）。
#[serial_test::serial]
#[tokio::test]
async fn an_expired_reviewer_token_is_refreshed_before_approving() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(|req: &wiremock::Request| {
            let form: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(&req.body)
                    .into_owned()
                    .collect();
            if form.get("grant_type").map(String::as_str) != Some("refresh_token")
                || form.get("refresh_token").map(String::as_str) != Some(REVIEWER_REFRESH_TOKEN)
            {
                return ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "error": "bad_refresh_token"
                }));
            }
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "ghu_refreshed",
                "refresh_token": "ghr_rotated",
                "expires_in": 28800,
                "token_type": "bearer"
            }))
        })
        .expect(1)
        .mount(&mock_server)
        .await;

    let connection = review_once_and_run_summary(
        &mock_server,
        serde_json::json!([]),
        Some(ReviewerLink {
            expires_in_secs: -60,
        }),
    )
    .await
    .expect("connection");

    assert_eq!(
        approvals(&mock_server)
            .await
            .into_iter()
            .map(|(_, auth)| auth)
            .collect::<Vec<_>>(),
        vec!["Bearer ghu_refreshed".to_string()],
        "更新後のトークンで承認する"
    );
    let decrypt = |enc: Option<String>| {
        auth_core::crypto::decrypt_token(TEST_OAUTH_ENCRYPTION_KEY, &enc.expect("token"))
            .expect("decrypt")
    };
    assert_eq!(decrypt(connection.access_token_enc), "ghu_refreshed");
    assert_eq!(
        decrypt(connection.refresh_token_enc),
        "ghr_rotated",
        "回転したリフレッシュトークンを保存し直す（古い方は次に使えない）"
    );
    assert!(
        connection
            .token_expires_at
            .expect("expiry")
            .with_timezone(&chrono::Utc)
            > chrono::Utc::now() + chrono::Duration::hours(7),
        "新しい期限を保存する"
    );
}

/// High / Medium が未解決なら承認しない（上の対照）。
#[serial_test::serial]
#[tokio::test]
async fn a_pull_request_with_blocking_findings_is_not_approved() {
    let mock_server = MockServer::start().await;
    review_once_and_run_summary(
        &mock_server,
        serde_json::json!([{ "severity": "medium", "title": "競合", "body": "本文" }]),
        LINKED,
    )
    .await;

    assert!(
        approvals(&mock_server)
            .await
            .iter()
            .all(|(body, _)| body["event"] != "APPROVE"),
        "承認は出さない（変更要求は出る）"
    );
}

/// レビュー後にコミットが積まれた PR と、閉じた PR は承認しない。
#[serial_test::serial]
#[tokio::test]
async fn a_stale_or_closed_pull_request_is_not_approved() {
    for (case, head, state) in [
        (
            "レビュー後に push された",
            "0000000000000000000000000000000000000000",
            "open",
        ),
        ("閉じた", REVIEWED_HEAD, "closed"),
    ] {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{PR_NUMBER}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "title": "feat: レビュー指摘管理",
                "user": { "login": "yupix" },
                "head": { "sha": head },
                "state": state
            })))
            .with_priority(1)
            .mount(&mock_server)
            .await;
        review_once_and_run_summary(&mock_server, serde_json::json!([]), LINKED).await;

        assert!(
            approvals(&mock_server).await.is_empty(),
            "{case} PR は承認しない"
        );
    }
}

/// 同じ commit へのレビュワー本人の承認が既にあれば、2 ページ目にあっても承認し直さない。
/// 1 ページ目は既定のページサイズ（100 件）ちょうどで埋めて、ページ送りを確かめる。
#[serial_test::serial]
#[tokio::test]
async fn an_existing_approval_on_a_later_page_is_not_repeated() {
    let mock_server = MockServer::start().await;
    let others: Vec<serde_json::Value> = (0..100)
        .map(|i| {
            serde_json::json!({
                "user": { "id": 100 + i, "login": format!("reviewer-{i}"), "type": "User" },
                "commit_id": REVIEWED_HEAD,
                "state": "COMMENTED"
            })
        })
        .collect();
    Mock::given(method("GET"))
        .and(path(reviews_api_path()))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(others))
        .with_priority(1)
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path(reviews_api_path()))
        .and(query_param("page", "2"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "user": { "id": REVIEWER_GITHUB_ID, "login": "reviewer", "type": "User" },
                "commit_id": REVIEWED_HEAD,
                "state": "APPROVED"
            }])),
        )
        .with_priority(1)
        .mount(&mock_server)
        .await;
    review_once_and_run_summary(&mock_server, serde_json::json!([]), LINKED).await;

    assert!(
        approvals(&mock_server).await.is_empty(),
        "承認済みの commit を二重に承認しない"
    );
}

/// 承認が通らなくても（PR 作者本人で 422、リポジトリへの権限が無く 403 など）
/// ジョブは失敗にせず、要約コメントは出す。
#[serial_test::serial]
#[tokio::test]
async fn a_rejected_approval_still_posts_the_summary_comment() {
    for status in [422, 403] {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(reviews_api_path()))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(serde_json::json!({
                    "message": "Unprocessable Entity"
                })),
            )
            .with_priority(1)
            .mount(&mock_server)
            .await;
        review_once_and_run_summary(&mock_server, serde_json::json!([]), LINKED).await;

        assert_eq!(approvals(&mock_server).await.len(), 1, "承認は試みている");
        let comments: Vec<serde_json::Value> =
            bodies_of(&mock_server, wiremock::http::Method::POST)
                .await
                .into_iter()
                .filter(|b| b.get("body").is_some())
                .collect();
        assert_eq!(comments.len(), 1, "{status} でも要約コメントは出ている");
    }
}

// ── 承認の取り消しと承認し直し ────────────────────────────────────────────

fn review_json(id: i64, state: &str, body: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "user": { "id": id, "login": "reviewer", "type": "User" },
        "commit_id": REVIEWED_HEAD,
        "state": state,
        "body": body,
    })
}

fn high() -> serde_json::Value {
    serde_json::json!([{ "severity": "high", "title": "競合", "body": "本文" }])
}

fn approved_only() -> Scenario {
    Scenario {
        reviews: Some(Box::new(|_, _| {
            serde_json::json!([review_json(REVIEWER_GITHUB_ID, "APPROVED", None)])
        })),
        ..Default::default()
    }
}

/// 承認済みの PR に同じ head で High のラウンドが出たら、承認者のトークンで
/// REQUEST_CHANGES を 1 回だけ出す。目印と commit_id を付ける。
#[serial_test::serial]
#[tokio::test]
async fn a_new_blocking_round_revokes_the_approval() {
    let mock_server = MockServer::start().await;
    scenario(
        &mock_server,
        serde_json::json!([]),
        LINKED,
        Scenario {
            second_round: Some((high(), false)),
            ..approved_only()
        },
    )
    .await;

    let posted = approvals(&mock_server).await;
    assert_eq!(posted.len(), 1);
    let (body, auth) = &posted[0];
    assert_eq!(auth, &format!("Bearer {REVIEWER_TOKEN}"));
    assert_eq!(body["event"], "REQUEST_CHANGES");
    assert_eq!(body["commit_id"], REVIEWED_HEAD);
    assert!(
        body["body"]
            .as_str()
            .expect("body")
            .contains("<!-- koyori-approval-revoked:"),
        "目印を入れる"
    );
}

/// 取り消し済み（最後の決定が目印付きの REQUEST_CHANGES）なら、ジョブが再び走っても重ねて出さない。
#[serial_test::serial]
#[tokio::test]
async fn an_already_revoked_approval_is_not_revoked_twice() {
    let mock_server = MockServer::start().await;
    scenario(
        &mock_server,
        serde_json::json!([]),
        LINKED,
        Scenario {
            reviews: Some(Box::new(|marker, _| {
                serde_json::json!([
                    review_json(REVIEWER_GITHUB_ID, "APPROVED", None),
                    review_json(REVIEWER_GITHUB_ID, "CHANGES_REQUESTED", Some(marker)),
                    review_json(REVIEWER_GITHUB_ID, "COMMENTED", None),
                ])
            })),
            second_round: Some((high(), false)),
            ..Default::default()
        },
    )
    .await;

    assert!(approvals(&mock_server).await.is_empty());
}

/// 承認したのが前のラウンドの作成者で、High を出したのが別の人でも、前の承認者の承認を取り消す。
#[serial_test::serial]
#[tokio::test]
async fn the_earlier_approver_is_revoked_when_someone_else_adds_a_blocking_round() {
    let mock_server = MockServer::start().await;
    scenario(
        &mock_server,
        serde_json::json!([]),
        LINKED,
        Scenario {
            second_round: Some((high(), true)),
            ..approved_only()
        },
    )
    .await;

    // 承認者には取り消し、High を出した別の人には変更要求が出る
    let posted = approvals(&mock_server).await;
    assert_eq!(posted.len(), 2);
    for (body, _) in &posted {
        assert_eq!(body["event"], "REQUEST_CHANGES");
    }
    let text_for = |token: &str| {
        posted
            .iter()
            .find(|(_, t)| t == &format!("Bearer {token}"))
            .and_then(|(b, _)| b["body"].as_str())
            .unwrap_or_default()
            .to_string()
    };
    let revoked = text_for(REVIEWER_TOKEN);
    assert!(revoked.contains("取り下げ"), "{revoked}");
    let requested = text_for(OTHER_TOKEN);
    assert!(requested.contains("変更を求めます"), "{requested}");
    assert!(!requested.contains("取り下げ"), "{requested}");
}

/// 取り消さない場合: 取り下げ済み・人の変更要求・連携なし・head が違う・閉じた PR。
#[serial_test::serial]
#[tokio::test]
async fn nothing_is_revoked_without_a_live_approval_or_a_fresh_open_pr() {
    let human_cr = || review_json(REVIEWER_GITHUB_ID, "CHANGES_REQUESTED", Some("直して"));
    for (case, reviews, link, head, state) in [
        (
            "人が取り下げた",
            // 実 API の dismiss は、元のレビュー自体の state を DISMISSED に変える
            serde_json::json!([review_json(REVIEWER_GITHUB_ID, "DISMISSED", None)]),
            LINKED,
            REVIEWED_HEAD,
            "open",
        ),
        (
            "人の変更要求",
            serde_json::json!([
                review_json(REVIEWER_GITHUB_ID, "APPROVED", None),
                human_cr()
            ]),
            LINKED,
            REVIEWED_HEAD,
            "open",
        ),
        (
            "連携なし",
            serde_json::json!([review_json(REVIEWER_GITHUB_ID, "APPROVED", None)]),
            None,
            REVIEWED_HEAD,
            "open",
        ),
        (
            "head が違う",
            serde_json::json!([review_json(REVIEWER_GITHUB_ID, "APPROVED", None)]),
            LINKED,
            "0000000000000000000000000000000000000000",
            "open",
        ),
        (
            "閉じた PR",
            serde_json::json!([review_json(REVIEWER_GITHUB_ID, "APPROVED", None)]),
            LINKED,
            REVIEWED_HEAD,
            "closed",
        ),
    ] {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{PR_NUMBER}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "title": "feat: レビュー指摘管理",
                "user": { "login": "yupix" },
                "head": { "sha": head },
                "state": state
            })))
            .with_priority(1)
            .mount(&mock_server)
            .await;
        scenario(
            &mock_server,
            serde_json::json!([]),
            link,
            Scenario {
                reviews: Some(Box::new(move |_, _| reviews.clone())),
                second_round: Some((high(), false)),
                ..Default::default()
            },
        )
        .await;
        assert!(approvals(&mock_server).await.is_empty(), "{case}");
    }
}

/// 承認していなくても、High / Medium の未解決を含むラウンドを今の head に出した本人の名義で
/// 変更要求（REQUEST_CHANGES）を 1 回だけ出す。目印と commit_id を付ける。
#[serial_test::serial]
#[tokio::test]
async fn a_blocking_round_requests_changes_as_the_reviewer() {
    let mock_server = MockServer::start().await;
    review_once_and_run_summary(&mock_server, high(), LINKED).await;

    let posted = approvals(&mock_server).await;
    assert_eq!(posted.len(), 1);
    let (body, auth) = &posted[0];
    assert_eq!(auth, &format!("Bearer {REVIEWER_TOKEN}"));
    assert_eq!(body["event"], "REQUEST_CHANGES");
    assert_eq!(body["commit_id"], REVIEWED_HEAD);
    let text = body["body"].as_str().expect("body");
    assert!(text.contains("<!-- koyori-approval-revoked:"), "{text}");
    assert!(text.contains("変更を求めます"), "{text}");
    assert!(!text.contains("取り下げ"), "{text}");
}

/// 前の head への承認だけが残っていても、今の head の High には変更要求を出す
/// （承認は commit ごとなので、今の head の最後の決定は無い）。
#[serial_test::serial]
#[tokio::test]
async fn a_stale_approval_on_an_older_head_is_overridden_by_a_change_request() {
    let mock_server = MockServer::start().await;
    scenario(
        &mock_server,
        high(),
        LINKED,
        Scenario {
            reviews: Some(Box::new(|_, _| {
                serde_json::json!([review_on(REVIEWER_GITHUB_ID, OLD_HEAD, "APPROVED", None)])
            })),
            ..Default::default()
        },
    )
    .await;

    let posted = approvals(&mock_server).await;
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].0["event"], "REQUEST_CHANGES");
    assert_eq!(posted[0].0["commit_id"], REVIEWED_HEAD);
}

/// 変更要求は、自分のラウンドに未解決の High / Medium がある人だけが出す。
/// 指摘なしのラウンドを出した人（先に出した 1 本目）には出さない。
#[serial_test::serial]
#[tokio::test]
async fn only_reviewers_with_their_own_blocking_findings_request_changes() {
    let mock_server = MockServer::start().await;
    scenario(
        &mock_server,
        serde_json::json!([]),
        LINKED,
        Scenario {
            second_round: Some((high(), true)),
            ..Default::default()
        },
    )
    .await;

    let posted = approvals(&mock_server).await;
    assert_eq!(posted.len(), 1, "指摘を出していない人には出さない");
    assert_eq!(posted[0].1, format!("Bearer {OTHER_TOKEN}"));
    assert_eq!(posted[0].0["event"], "REQUEST_CHANGES");
    assert_eq!(posted[0].0["commit_id"], REVIEWED_HEAD);
}

/// 変更要求を出さない場合（上の対照）: 同じ commit に変更要求済み（目印の有無を問わない）・
/// 取り下げ済み・前の head の task の変更要求が残っている・連携なし・head が違う・閉じた PR。
#[serial_test::serial]
#[tokio::test]
async fn changes_are_not_requested_twice_or_against_a_human_decision() {
    let fresh = REVIEWED_HEAD;
    let stale = "0000000000000000000000000000000000000000";
    for (case, reviews, link, head, state) in [
        (
            "同じ commit に task の変更要求がある",
            serde_json::json!([review_json(
                REVIEWER_GITHUB_ID,
                "CHANGES_REQUESTED",
                Some("<!-- koyori-approval-revoked:PROJECT -->")
            )]),
            LINKED,
            fresh,
            "open",
        ),
        (
            "同じ commit に人の変更要求がある",
            serde_json::json!([review_json(
                REVIEWER_GITHUB_ID,
                "CHANGES_REQUESTED",
                Some("直して")
            )]),
            LINKED,
            fresh,
            "open",
        ),
        (
            "同じ commit で変更要求を人が取り下げた",
            // 実 API の dismiss は元のレビューの state を DISMISSED に変える（本文の目印は残る）
            serde_json::json!([review_json(
                REVIEWER_GITHUB_ID,
                "DISMISSED",
                Some("<!-- koyori-approval-revoked:PROJECT -->")
            )]),
            LINKED,
            fresh,
            "open",
        ),
        (
            "前の head の task の変更要求が残っている",
            serde_json::json!([review_on(
                REVIEWER_GITHUB_ID,
                OLD_HEAD,
                "CHANGES_REQUESTED",
                Some("<!-- koyori-approval-revoked:PROJECT -->")
            )]),
            LINKED,
            fresh,
            "open",
        ),
        ("連携なし", serde_json::json!([]), None, fresh, "open"),
        ("head が違う", serde_json::json!([]), LINKED, stale, "open"),
        ("閉じた PR", serde_json::json!([]), LINKED, fresh, "closed"),
    ] {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{PR_NUMBER}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "title": "feat: レビュー指摘管理",
                "user": { "login": "yupix" },
                "head": { "sha": head },
                "state": state
            })))
            .with_priority(1)
            .mount(&mock_server)
            .await;
        scenario(
            &mock_server,
            high(),
            link,
            Scenario {
                reviews: Some(Box::new(move |marker, _| {
                    // PROJECT は呼び出し側が書いた仮の目印。実際のプロジェクトの目印に置き換える
                    let text = serde_json::to_string(&reviews)
                        .expect("json")
                        .replace("<!-- koyori-approval-revoked:PROJECT -->", marker);
                    serde_json::from_str(&text).expect("json")
                })),
                ..Default::default()
            },
        )
        .await;
        assert!(approvals(&mock_server).await.is_empty(), "{case}");
    }
}

/// 同じ commit への本人の変更要求が 2 ページ目にあっても、重ねて出さない。
/// 1 ページ目は既定のページサイズ（100 件）ちょうどで埋めて、ページ送りを確かめる。
#[serial_test::serial]
#[tokio::test]
async fn an_existing_change_request_on_a_later_page_is_not_repeated() {
    let mock_server = MockServer::start().await;
    let others: Vec<serde_json::Value> = (0..100)
        .map(|i| {
            serde_json::json!({
                "user": { "id": 100 + i, "login": format!("reviewer-{i}"), "type": "User" },
                "commit_id": REVIEWED_HEAD,
                "state": "COMMENTED"
            })
        })
        .collect();
    Mock::given(method("GET"))
        .and(path(reviews_api_path()))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(others))
        .with_priority(1)
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path(reviews_api_path()))
        .and(query_param("page", "2"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "user": { "id": REVIEWER_GITHUB_ID, "login": "reviewer", "type": "User" },
                "commit_id": REVIEWED_HEAD,
                "state": "CHANGES_REQUESTED"
            }])),
        )
        .with_priority(1)
        .mount(&mock_server)
        .await;
    review_once_and_run_summary(&mock_server, high(), LINKED).await;

    assert!(
        approvals(&mock_server).await.is_empty(),
        "変更要求済みの commit に重ねて出さない"
    );
}

/// 取り消しと変更要求が 403 / 422 でも要約コメントは出て、ジョブは成功する。
#[serial_test::serial]
#[tokio::test]
async fn a_rejected_revocation_still_posts_the_summary_comment() {
    for status in [422, 403] {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(reviews_api_path()))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(serde_json::json!({
                    "message": "nope"
                })),
            )
            .with_priority(1)
            .mount(&mock_server)
            .await;
        scenario(
            &mock_server,
            serde_json::json!([]),
            LINKED,
            Scenario {
                second_round: Some((high(), false)),
                ..approved_only()
            },
        )
        .await;

        assert_eq!(
            approvals(&mock_server).await.len(),
            1,
            "取り消しは試みている"
        );
        let comments = bodies_of(&mock_server, wiremock::http::Method::POST)
            .await
            .into_iter()
            .filter(|b| b.get("body").is_some() && b.get("event").is_none())
            .count();
        assert_eq!(comments, 1, "{status} でも要約コメントは出ている");
    }

    // 承認していない人への新しい変更要求が拒まれたときも同じ
    for status in [422, 403] {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(reviews_api_path()))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(serde_json::json!({
                    "message": "nope"
                })),
            )
            .with_priority(1)
            .mount(&mock_server)
            .await;
        review_once_and_run_summary(&mock_server, high(), LINKED).await;

        assert_eq!(
            approvals(&mock_server).await.len(),
            1,
            "変更要求は試みている"
        );
        let comments = bodies_of(&mock_server, wiremock::http::Method::POST)
            .await
            .into_iter()
            .filter(|b| b.get("body").is_some() && b.get("event").is_none())
            .count();
        assert_eq!(comments, 1, "{status} でも要約コメントは出ている");
    }
}

/// 1 人の投稿が拒まれても（422 / 403）、残りのレビュワーへ進む。
/// どちらが先に処理されても確かめられるよう、失敗させる人を入れ替えて 2 回ずつ回す。
#[serial_test::serial]
#[tokio::test]
async fn a_rejected_reviewer_does_not_stop_the_other_reviewer() {
    for (failing, succeeding) in [(REVIEWER_TOKEN, OTHER_TOKEN), (OTHER_TOKEN, REVIEWER_TOKEN)] {
        for status in [422, 403] {
            let mock_server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path(reviews_api_path()))
                .and(wiremock::matchers::header(
                    "authorization",
                    format!("Bearer {failing}").as_str(),
                ))
                .respond_with(
                    ResponseTemplate::new(status).set_body_json(serde_json::json!({
                        "message": "nope"
                    })),
                )
                .with_priority(1)
                .mount(&mock_server)
                .await;
            scenario(
                &mock_server,
                high(),
                LINKED,
                Scenario {
                    second_round: Some((high(), true)),
                    ..Default::default()
                },
            )
            .await;

            let tokens: Vec<String> = approvals(&mock_server)
                .await
                .into_iter()
                .map(|(_, token)| token)
                .collect();
            assert_eq!(tokens.len(), 2, "{failing} が {status} でも両方に試みる");
            assert!(
                tokens.contains(&format!("Bearer {succeeding}")),
                "{failing} が {status} でも {succeeding} には出す"
            );
        }
    }
}

/// PR の作者がレビュワー本人のとき、GitHub は自分の PR への Approve / 変更要求を 422 で拒む
/// ので、投稿を試みない（Approve・変更要求・承認の取り消しの全部）。作者が別人なら出す。
/// 作者の ID が分からないとき（応答に id が無い）は従来どおり試みる（他のテスト）。
#[serial_test::serial]
#[tokio::test]
async fn nothing_is_posted_when_the_reviewer_authored_the_pull_request() {
    let someone_else: i64 = 7777;
    for (case, author, findings, opts, expected) in [
        (
            "Approve（作者が本人）",
            REVIEWER_GITHUB_ID,
            serde_json::json!([]),
            Scenario::default(),
            vec![],
        ),
        (
            "Approve（作者が別人）",
            someone_else,
            serde_json::json!([]),
            Scenario::default(),
            vec![("APPROVE", REVIEWER_TOKEN)],
        ),
        (
            "変更要求（作者が本人）",
            REVIEWER_GITHUB_ID,
            high(),
            Scenario::default(),
            vec![],
        ),
        (
            "変更要求（作者が別人）",
            someone_else,
            high(),
            Scenario::default(),
            vec![("REQUEST_CHANGES", REVIEWER_TOKEN)],
        ),
        (
            "承認の取り消し（作者が本人）",
            REVIEWER_GITHUB_ID,
            serde_json::json!([]),
            Scenario {
                second_round: Some((high(), false)),
                ..approved_only()
            },
            vec![],
        ),
        (
            "承認の取り消し（作者が別人）",
            someone_else,
            serde_json::json!([]),
            Scenario {
                second_round: Some((high(), false)),
                ..approved_only()
            },
            vec![("REQUEST_CHANGES", REVIEWER_TOKEN)],
        ),
        (
            "本人だけ飛ばし、別のレビュワーには出す",
            REVIEWER_GITHUB_ID,
            high(),
            Scenario {
                second_round: Some((high(), true)),
                ..Default::default()
            },
            vec![("REQUEST_CHANGES", OTHER_TOKEN)],
        ),
    ] {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{PR_NUMBER}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "title": "feat: レビュー指摘管理",
                "user": { "login": "author", "id": author },
                "head": { "sha": REVIEWED_HEAD },
                "state": "open"
            })))
            .with_priority(1)
            .mount(&mock_server)
            .await;
        scenario(&mock_server, findings, LINKED, opts).await;

        let posted: Vec<(String, String)> = approvals(&mock_server)
            .await
            .into_iter()
            .map(|(body, token)| (body["event"].as_str().unwrap_or_default().into(), token))
            .collect();
        let expected: Vec<(String, String)> = expected
            .into_iter()
            .map(|(event, token)| (event.to_string(), format!("Bearer {token}")))
            .collect();
        assert_eq!(posted, expected, "{case}");

        // 作者が本人のときは、レビュー一覧の取得（取り消し済みの判定）も試みない
        let listed = mock_server
            .received_requests()
            .await
            .expect("received requests")
            .iter()
            .filter(|r| {
                r.method == wiremock::http::Method::GET
                    && r.url.path() == reviews_api_path()
                    && r.headers.get("authorization").and_then(|v| v.to_str().ok())
                        == Some(format!("Bearer {REVIEWER_TOKEN}").as_str())
            })
            .count();
        if author == REVIEWER_GITHUB_ID {
            assert_eq!(listed, 0, "{case}: 本人の分はレビュー一覧も取らない");
        }
    }
}

/// 取り消した後にマージ可へ戻ったら承認し直す。人が取り下げた承認・人の変更要求の後は承認し直さない。
#[serial_test::serial]
#[tokio::test]
async fn approval_is_restored_only_after_our_own_revocation() {
    let human_cr = || review_json(REVIEWER_GITHUB_ID, "CHANGES_REQUESTED", Some("直して"));
    for (case, restored, reviews) in [
        (
            "取り消した後",
            true,
            vec![
                review_json(REVIEWER_GITHUB_ID, "APPROVED", None),
                review_json(
                    REVIEWER_GITHUB_ID,
                    "CHANGES_REQUESTED",
                    Some("<!-- koyori-approval-revoked:PROJECT -->"),
                ),
            ],
        ),
        (
            "承認せずに task が変更を求めた後",
            true,
            vec![review_json(
                REVIEWER_GITHUB_ID,
                "CHANGES_REQUESTED",
                Some("<!-- koyori-approval-revoked:PROJECT -->"),
            )],
        ),
        (
            "人が取り下げた",
            false,
            vec![review_json(REVIEWER_GITHUB_ID, "DISMISSED", None)],
        ),
        (
            "人の変更要求",
            false,
            vec![
                review_json(REVIEWER_GITHUB_ID, "APPROVED", None),
                human_cr(),
            ],
        ),
    ] {
        let mock_server = MockServer::start().await;
        review_with_reviews(&mock_server, reviews).await;
        let posted = approvals(&mock_server).await;
        if restored {
            assert_eq!(posted.len(), 1, "{case}");
            assert_eq!(posted[0].0["event"], "APPROVE", "{case}");
        } else {
            assert!(posted.is_empty(), "{case}");
        }
    }
}

async fn review_with_reviews(mock_server: &MockServer, reviews: Vec<serde_json::Value>) {
    scenario(
        mock_server,
        serde_json::json!([]),
        LINKED,
        Scenario {
            reviews: Some(Box::new(move |marker, _| {
                // PROJECT は呼び出し側が書いた仮の目印。実際のプロジェクトの目印に置き換える
                let text = serde_json::to_string(&reviews)
                    .expect("json")
                    .replace("<!-- koyori-approval-revoked:PROJECT -->", marker);
                serde_json::from_str(&text).expect("json")
            })),
            ..Default::default()
        },
    )
    .await;
}

/// 他のレビュワーの承認を取り消していた場合、マージ可に戻ったら両方に APPROVE が出る
/// （取り消された人は承認し直し）。取り消されていない人には出さない。
#[serial_test::serial]
#[tokio::test]
async fn every_revoked_reviewer_is_approved_again_when_merge_ready() {
    for (case, a_reviews, expected) in [
        (
            "A は取り消されている",
            Box::new(|marker: &str| {
                serde_json::json!([
                    review_json(REVIEWER_GITHUB_ID, "APPROVED", None),
                    review_json(REVIEWER_GITHUB_ID, "CHANGES_REQUESTED", Some(marker)),
                ])
            }) as Box<dyn Fn(&str) -> serde_json::Value + Send + Sync>,
            vec![
                format!("Bearer {OTHER_TOKEN}"),
                format!("Bearer {REVIEWER_TOKEN}"),
            ],
        ),
        (
            "A の最後は承認のまま",
            Box::new(|_: &str| {
                serde_json::json!([review_json(REVIEWER_GITHUB_ID, "APPROVED", None)])
            }),
            vec![format!("Bearer {OTHER_TOKEN}")],
        ),
        (
            "A に決定が無い",
            Box::new(|_: &str| serde_json::json!([])),
            vec![format!("Bearer {OTHER_TOKEN}")],
        ),
    ] {
        let mock_server = MockServer::start().await;
        scenario(
            &mock_server,
            serde_json::json!([]),
            LINKED,
            Scenario {
                reviews: Some(Box::new(move |marker, token| {
                    if token == REVIEWER_TOKEN {
                        a_reviews(marker)
                    } else {
                        serde_json::json!([])
                    }
                })),
                // 最新ラウンド（B）は指摘なし
                second_round: Some((serde_json::json!([]), true)),
                ..Default::default()
            },
        )
        .await;

        let posted = approvals(&mock_server).await;
        assert!(
            posted.iter().all(|(b, _)| b["event"] == "APPROVE"),
            "{case}"
        );
        let mut tokens: Vec<String> = posted.into_iter().map(|(_, t)| t).collect();
        tokens.sort();
        let mut expected = expected;
        expected.sort();
        assert_eq!(tokens, expected, "{case}");
    }
}

/// 前の head（push 前）を指す commit。
const OLD_HEAD: &str = "1111111111111111111111111111111111111111";

fn review_on(id: i64, commit: &str, state: &str, body: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "user": { "id": id, "login": "reviewer-a", "type": "User" },
        "commit_id": commit,
        "state": state,
        "body": body,
    })
}

/// 投稿した要約コメントの本文（新規投稿の POST）。
async fn summary_comment_body(mock_server: &MockServer) -> String {
    let comments: Vec<String> = bodies_of(mock_server, wiremock::http::Method::POST)
        .await
        .into_iter()
        .filter(|b| b.get("event").is_none())
        .filter_map(|b| b.get("body").and_then(|v| v.as_str()).map(String::from))
        .collect();
    assert_eq!(comments.len(), 1, "要約コメントは 1 件");
    comments.into_iter().next().expect("comment")
}

/// A が前の head（h1）で承認し、task がそれを取り消した後、push された h2 を B だけが
/// レビューしてマージ可になった場合。A の変更要求は GitHub に残ってマージを止めるので、
/// task も「マージ可」にせず A の再レビューを待つ。h2 を見ていない A を承認しない。
/// A の最後の決定が人の変更要求なら task の取り消しではないので待たない（対照）。
#[serial_test::serial]
#[tokio::test]
async fn a_reviewer_revoked_on_an_older_head_must_review_the_new_head() {
    for (case, a_last_body, waits) in [
        (
            "task が取り消した",
            Some("<!-- koyori-approval-revoked:PROJECT -->"),
            true,
        ),
        ("人の変更要求", Some("直して"), false),
    ] {
        let mock_server = MockServer::start().await;
        scenario(
            &mock_server,
            serde_json::json!([]),
            LINKED,
            Scenario {
                reviews: Some(Box::new(move |marker, _| {
                    serde_json::json!([
                        review_on(REVIEWER_GITHUB_ID, OLD_HEAD, "APPROVED", None),
                        review_on(
                            REVIEWER_GITHUB_ID,
                            OLD_HEAD,
                            "CHANGES_REQUESTED",
                            a_last_body
                                .map(|b| b
                                    .replace("<!-- koyori-approval-revoked:PROJECT -->", marker))
                                .as_deref(),
                        ),
                    ])
                })),
                // B が h2（今の head）を指摘なしでレビューした
                second_round: Some((serde_json::json!([]), true)),
                first_head: Some(OLD_HEAD),
            },
        )
        .await;

        let posted = approvals(&mock_server).await;
        let body = summary_comment_body(&mock_server).await;
        if waits {
            assert!(
                posted
                    .iter()
                    .all(|(_, t)| t != &format!("Bearer {REVIEWER_TOKEN}")),
                "{case}: h2 を見ていない A を承認しない"
            );
            assert!(
                posted.is_empty(),
                "{case}: マージ可でないので B も承認しない"
            );
            assert!(!body.contains("マージ可"), "{case}: {body}");
            assert!(body.contains("再レビュー待ち"), "{case}: {body}");
            assert!(body.contains("`reviewer-a`"), "{case}: {body}");
        } else {
            assert_eq!(
                posted,
                vec![(
                    serde_json::json!({ "event": "APPROVE", "commit_id": REVIEWED_HEAD }),
                    format!("Bearer {OTHER_TOKEN}"),
                )],
                "{case}: 最新ラウンドの B だけを承認する"
            );
            assert!(body.contains("マージ可"), "{case}: {body}");
        }
    }
}

/// 前の head で task に取り消された A が、今の head にマージ可のラウンドを出したら、
/// A の名義で今の head を承認して変更要求を上書きする。
#[serial_test::serial]
#[tokio::test]
async fn a_revoked_reviewer_is_approved_after_reviewing_the_new_head() {
    let mock_server = MockServer::start().await;
    scenario(
        &mock_server,
        serde_json::json!([]),
        LINKED,
        Scenario {
            reviews: Some(Box::new(|marker, _| {
                serde_json::json!([
                    review_on(REVIEWER_GITHUB_ID, OLD_HEAD, "APPROVED", None),
                    review_on(
                        REVIEWER_GITHUB_ID,
                        OLD_HEAD,
                        "CHANGES_REQUESTED",
                        Some(marker)
                    ),
                ])
            })),
            // A 自身が h2 を指摘なしでレビューした
            second_round: Some((serde_json::json!([]), false)),
            first_head: Some(OLD_HEAD),
        },
    )
    .await;

    assert_eq!(
        approvals(&mock_server).await,
        vec![(
            serde_json::json!({ "event": "APPROVE", "commit_id": REVIEWED_HEAD }),
            format!("Bearer {REVIEWER_TOKEN}"),
        )]
    );
    assert!(
        summary_comment_body(&mock_server)
            .await
            .contains("マージ可")
    );
}

/// レビューが上限（500 件）を超えて最後の決定が分からないときは、取り消さない。
#[serial_test::serial]
#[tokio::test]
async fn a_review_list_cut_off_by_the_page_limit_is_not_revoked() {
    let mock_server = MockServer::start().await;
    for page in 1..=5 {
        let mut items: Vec<serde_json::Value> = (0..100)
            .map(|i| review_json(100 + i, "COMMENTED", None))
            .collect();
        if page == 1 {
            items[0] = review_json(REVIEWER_GITHUB_ID, "APPROVED", None);
        }
        Mock::given(method("GET"))
            .and(path(reviews_api_path()))
            .and(query_param("page", page.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(items))
            .with_priority(1)
            .mount(&mock_server)
            .await;
    }
    scenario(
        &mock_server,
        serde_json::json!([]),
        LINKED,
        Scenario {
            second_round: Some((high(), false)),
            ..Default::default()
        },
    )
    .await;

    assert!(approvals(&mock_server).await.is_empty());
}
