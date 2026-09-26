use crate::common::TestApp;
use axum::http::StatusCode;
use entity::{webhook_deliveries, webhooks};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    prelude::Uuid,
};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

// 外部向け Webhook（TASK-227）の統合テスト。仕様は docs/features/tasks/10.webhooks.md。

const SECRET: &str = "0123456789abcdef-secret";

struct Fixture {
    app: TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    status_id: String,
    owner: crate::common::TestUser,
    member: crate::common::TestUser,
}

impl Fixture {
    fn webhooks_path(&self) -> String {
        format!(
            "/v1/tenants/{}/projects/{}/webhooks",
            self.tenant_id, self.project_id
        )
    }

    async fn login(&mut self, user: &crate::common::TestUser) {
        self.app.reset_session_client();
        self.app
            .login_session_no_content(&user.email, &user.password)
            .await;
    }

    /// いまログインしている利用者として Webhook を作り、ID を返す。
    async fn create_webhook(&self, url: &str, events: &[&str], format: &str) -> Uuid {
        let res = self
            .app
            .post_json_with_session(
                &self.webhooks_path(),
                serde_json::json!({"url": url, "secret": SECRET, "events": events, "format": format}),
            )
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let body: serde_json::Value = res.json().await.expect("json");
        assert_eq!(body["secret"], SECRET, "作成時だけ secret を平文で返す");
        assert_eq!(body["url"], url, "作成者には完全な URL を返す");
        body["id"].as_str().expect("id").parse().expect("uuid")
    }

    async fn create_task(&self, title: &str) {
        let res = self
            .app
            .post_json_with_session(
                &format!(
                    "/v1/tenants/{}/projects/{}/tasks",
                    self.tenant_id, self.project_id
                ),
                serde_json::json!({"title": title, "status_id": self.status_id}),
            )
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    async fn deliveries(&self, webhook_id: Uuid) -> Vec<webhook_deliveries::Model> {
        webhook_deliveries::Entity::find()
            .filter(webhook_deliveries::Column::WebhookId.eq(webhook_id))
            .order_by_asc(webhook_deliveries::Column::CreatedAt)
            .all(&self.app.state.db)
            .await
            .expect("load deliveries")
    }

    async fn webhook(&self, id: Uuid) -> webhooks::Model {
        webhooks::Entity::find_by_id(id)
            .one(&self.app.state.db)
            .await
            .expect("load webhook")
            .expect("webhook exists")
    }
}

fn job_state(app: &TestApp) -> job::JobState {
    job::JobState {
        settings: app.state.settings.clone(),
        db: app.state.db.clone(),
        redis_client: app.state.redis_client.clone(),
        smtp_client: app.state.smtp_client.clone(),
        http_client: app.state.http_client.clone(),
        review_summary_storage: app.state.review_summary_storage.clone(),
        pg_pool: app.state.pg_pool.clone(),
    }
}

/// オーナー（テナントオーナー）と Member ロールのプロジェクトメンバーがいるプロジェクト。
/// オーナーでログインした状態で返す。
async fn setup() -> Fixture {
    setup_with_loopback(true).await
}

async fn setup_with_loopback(allow: bool) -> Fixture {
    let mut app = TestApp::new_with_webhook_loopback(allow).await;
    let owner = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;

    let status_resp = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/statuses",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({"name":"Backlog","color":"#336699","position":0,"is_default":true}),
        )
        .await;
    assert_eq!(status_resp.status(), StatusCode::CREATED);
    let status_id = status_resp.json::<serde_json::Value>().await.expect("json")["id"]
        .as_str()
        .expect("status id")
        .to_string();

    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    let added = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/members",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({"user_id": member.id, "role": "Member"}),
        )
        .await;
    assert_eq!(added.status(), StatusCode::CREATED);

    Fixture {
        app,
        tenant_id: tp.tenant_id,
        project_id: tp.project_id,
        status_id,
        owner,
        member,
    }
}

async fn mock_server(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

/// 変更は Admin / オーナーだけ（Member は 403）、読み取りは Member も可。secret は一覧に出ない。
#[tokio::test]
async fn crud_and_authorization() {
    let mut fx = setup().await;
    let (owner, member) = (fx.owner.clone(), fx.member.clone());
    let id = fx
        .create_webhook("https://203.0.113.10/hook", &["task.created"], "json")
        .await;

    fx.login(&member).await;
    let denied = fx
        .app
        .post_json_with_session(
            &fx.webhooks_path(),
            serde_json::json!({"url": "https://203.0.113.10/x", "secret": SECRET, "events": ["task.created"]}),
        )
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN, "Member は作れない");
    let denied = fx
        .app
        .delete_with_session(&format!("{}/{id}", fx.webhooks_path()))
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN, "Member は消せない");

    let listed = fx.app.get_with_session(&fx.webhooks_path()).await;
    assert_eq!(listed.status(), StatusCode::OK, "Member も一覧は読める");
    let listed: serde_json::Value = listed.json().await.expect("json");
    assert_eq!(listed.as_array().expect("array").len(), 1);
    assert!(
        listed[0].get("secret").is_none(),
        "一覧に secret を出さない"
    );
    assert!(listed[0].get("secret_enc").is_none());
    assert_eq!(listed[0]["url"], "https://203.0.113.10/hook");

    fx.login(&owner).await;
    let updated = fx
        .app
        .put_json_with_session(
            &format!("{}/{id}", fx.webhooks_path()),
            serde_json::json!({"events": ["task.created", "comment.created"], "format": "discord"}),
        )
        .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated: serde_json::Value = updated.json().await.expect("json");
    assert_eq!(
        updated["events"],
        serde_json::json!(["task.created", "comment.created"])
    );
    assert_eq!(updated["format"], "discord");

    let bad_event = fx
        .app
        .put_json_with_session(
            &format!("{}/{id}", fx.webhooks_path()),
            serde_json::json!({"events": ["task.updated"]}),
        )
        .await;
    assert_eq!(
        bad_event.status(),
        StatusCode::BAD_REQUEST,
        "未実装のイベントは 400"
    );

    let deleted = fx
        .app
        .delete_with_session(&format!("{}/{id}", fx.webhooks_path()))
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert!(
        webhooks::Entity::find_by_id(id)
            .one(&fx.app.state.db)
            .await
            .expect("load")
            .is_none()
    );
}

#[tokio::test]
async fn discord_url_is_only_returned_to_admin_operations() {
    let mut fx = setup().await;
    let (owner, member) = (fx.owner.clone(), fx.member.clone());
    let url = "https://203.0.113.10/api/webhooks/123/discord-token?secret=token";
    let id = fx.create_webhook(url, &["task.created"], "discord").await;

    fx.login(&member).await;
    let listed = fx.app.get_with_session(&fx.webhooks_path()).await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed: serde_json::Value = listed.json().await.expect("json");
    assert_eq!(listed[0]["url"], "[redacted]");
    assert!(!listed.to_string().contains("discord-token"));
    let denied = fx
        .app
        .put_json_with_session(
            &format!("{}/{id}", fx.webhooks_path()),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    fx.login(&owner).await;
    let updated = fx
        .app
        .put_json_with_session(
            &format!("{}/{id}", fx.webhooks_path()),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        updated.json::<serde_json::Value>().await.expect("json")["url"],
        url
    );
    assert_eq!(fx.webhook(id).await.url, url, "保存する送信先は伏せない");
}

#[tokio::test(flavor = "multi_thread")]
async fn loopback_is_rejected_on_create_update_and_delivery_by_default() {
    let fx = setup_with_loopback(false).await;
    let id = fx
        .create_webhook("https://203.0.113.10/hook", &["task.created"], "json")
        .await;
    for url in [
        "http://127.0.0.1/hook",
        "https://127.0.0.2/hook",
        "https://127.1/hook",
        "https://2130706433/hook",
        "http://localhost/hook",
        "https://localhost./hook",
        "https://[::1]/hook",
        "https://[::ffff:127.0.0.1]/hook",
    ] {
        let created = fx
            .app
            .post_json_with_session(
                &fx.webhooks_path(),
                serde_json::json!({"url": url, "secret": SECRET, "events": ["task.created"]}),
            )
            .await;
        assert_eq!(created.status(), StatusCode::BAD_REQUEST, "create: {url}");
        let updated = fx
            .app
            .put_json_with_session(
                &format!("{}/{id}", fx.webhooks_path()),
                serde_json::json!({"url": url}),
            )
            .await;
        assert_eq!(updated.status(), StatusCode::BAD_REQUEST, "update: {url}");
    }

    // 以前の設定で保存した loopback の送信先も、送信直前に拒否する。
    let server = mock_server(200).await;
    let mut active: webhooks::ActiveModel = fx.webhook(id).await.into();
    active.url = Set(server.uri());
    active
        .update(&fx.app.state.db)
        .await
        .expect("store old destination");
    fx.create_task("送信しないタスク").await;
    assert_eq!(
        job::webhook_delivery::send_pending_once(&job_state(&fx.app))
            .await
            .expect("sweep"),
        0
    );
    assert!(
        server
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
    let deliveries = fx.deliveries(id).await;
    assert_eq!(deliveries[0].attempt, 1);
    assert!(
        deliveries[0]
            .last_error
            .as_deref()
            .expect("error")
            .contains("loopback")
    );
}

#[tokio::test]
async fn delivery_connection_errors_do_not_expose_discord_urls() {
    let fx = setup().await;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    drop(listener);
    let url = format!("http://{address}/api/webhooks/123/discord-token");
    let id = fx.create_webhook(&url, &["task.created"], "discord").await;
    fx.create_task("接続に失敗するタスク").await;
    assert_eq!(
        job::webhook_delivery::send_pending_once(&job_state(&fx.app))
            .await
            .expect("sweep"),
        0
    );
    let deliveries = fx.deliveries(id).await;
    let error = deliveries[0].last_error.as_deref().expect("error");
    assert!(error.starts_with("send:"), "{error}");
    assert!(!error.contains("discord-token"), "{error}");
    assert!(!error.contains(&url), "{error}");
}

/// private / http の送信先と、短い secret は 400。
#[tokio::test]
async fn rejects_unsafe_url_and_short_secret() {
    let fx = setup().await;
    for url in [
        "https://10.0.0.5/hook",
        "https://169.254.169.254/",
        "http://203.0.113.10/hook",
    ] {
        let res = fx
            .app
            .post_json_with_session(
                &fx.webhooks_path(),
                serde_json::json!({"url": url, "secret": SECRET, "events": ["task.created"]}),
            )
            .await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{url} は拒否する");
    }
    let res = fx
        .app
        .post_json_with_session(
            &fx.webhooks_path(),
            serde_json::json!({"url": "https://203.0.113.10/hook", "secret": "short", "events": ["task.created"]}),
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::BAD_REQUEST,
        "短い secret は拒否する"
    );
}

/// 他プロジェクトの Webhook ID は 404（存在を漏らさない）。
#[tokio::test]
async fn other_project_webhook_is_not_found() {
    let fx = setup().await;
    let id = fx
        .create_webhook("https://203.0.113.10/hook", &["task.created"], "json")
        .await;
    let other = crate::common::insert_extra_project(&fx.app, fx.tenant_id).await;
    let other_path = format!("/v1/tenants/{}/projects/{other}/webhooks", fx.tenant_id);

    let res = fx
        .app
        .put_json_with_session(
            &format!("{other_path}/{id}"),
            serde_json::json!({"is_active": false}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let res = fx
        .app
        .get_with_session(&format!("{other_path}/{id}/deliveries"))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let res = fx
        .app
        .delete_with_session(&format!("{other_path}/{id}"))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert!(fx.webhook(id).await.is_active, "対象外の口からは変わらない");
}

/// レビューの起票で、購読している Webhook に配信が 1 行積まれる（購読していないものには積まない）。
#[tokio::test]
async fn review_round_enqueues_one_delivery() {
    let fx = setup().await;
    let subscribed = fx
        .create_webhook(
            "https://203.0.113.10/review",
            &["review.round_created"],
            "json",
        )
        .await;
    let unsubscribed = fx
        .create_webhook("https://203.0.113.10/task", &["task.created"], "json")
        .await;

    let res = fx
        .app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/reviews",
                fx.tenant_id, fx.project_id
            ),
            serde_json::json!({
                "pr_number": 618,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "summary": "総評",
                "findings": [{"severity": "high", "title": "指摘", "body": "根拠"}],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let deliveries = fx.deliveries(subscribed).await;
    assert_eq!(deliveries.len(), 1);
    let delivery = &deliveries[0];
    assert_eq!(delivery.event, "review.round_created");
    assert_eq!(delivery.attempt, 0);
    assert!(delivery.next_attempt_at.is_some(), "送信待ちで積む");
    assert_eq!(delivery.payload["event"], "review.round_created");
    assert_eq!(delivery.payload["pr_number"], 618);
    assert_eq!(delivery.payload["counts"]["high"], 1);
    assert_eq!(
        delivery.payload["actor"]["id"],
        fx.owner.id.to_string(),
        "actor は id つきの object"
    );
    assert!(delivery.payload["project"]["key"].is_string());
    assert!(fx.deliveries(unsubscribed).await.is_empty());

    // 届かない宛先の配信を残すと、他のテストの掃き出しが送信の timeout を待つ
    webhooks::Entity::delete_many()
        .filter(webhooks::Column::ProjectId.eq(fx.project_id))
        .exec(&fx.app.state.db)
        .await
        .expect("cleanup webhooks");
}

/// 2xx で配信済みになり、署名ヘッダは HMAC-SHA256(secret, 本文) と一致する。
#[tokio::test]
async fn delivers_json_with_signature() {
    let fx = setup().await;
    let server = mock_server(200).await;
    let id = fx
        .create_webhook(&server.uri(), &["task.created"], "json")
        .await;
    fx.create_task("署名を確かめる").await;

    job::webhook_delivery::send_pending_once(&job_state(&fx.app))
        .await
        .expect("sweep");

    let delivery = fx.deliveries(id).await.remove(0);
    assert_eq!(delivery.status_code, Some(200));
    assert_eq!(delivery.attempt, 1);
    assert!(delivery.delivered_at.is_some());
    assert!(
        delivery.next_attempt_at.is_none(),
        "済んだら待ち行列から外れる"
    );

    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    let header = |name: &str| {
        request
            .headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(
        header("x-task-signature"),
        service::webhooks::sign(SECRET, &request.body)
    );
    assert!(header("x-task-signature").starts_with("sha256="));
    assert_eq!(header("x-task-event"), "task.created");
    assert_eq!(header("x-task-delivery"), delivery.id.to_string());
    let body: serde_json::Value = serde_json::from_slice(&request.body).expect("json body");
    assert_eq!(body["task"]["title"], "署名を確かめる");
    assert_eq!(body["project_id"], fx.project_id.to_string());
}

/// 5xx は attempt を進め、30 秒後に再試行する。
#[tokio::test]
async fn failure_schedules_retry_with_backoff() {
    let fx = setup().await;
    let server = mock_server(500).await;
    let id = fx
        .create_webhook(&server.uri(), &["task.created"], "json")
        .await;
    fx.create_task("失敗する").await;

    let before = chrono::Utc::now();
    job::webhook_delivery::send_pending_once(&job_state(&fx.app))
        .await
        .expect("sweep");

    let delivery = fx.deliveries(id).await.remove(0);
    assert_eq!(delivery.status_code, Some(500));
    assert_eq!(delivery.attempt, 1);
    assert!(delivery.delivered_at.is_none());
    assert!(delivery.last_error.is_some());
    let next = delivery.next_attempt_at.expect("再試行が予定される");
    let wait = next.with_timezone(&chrono::Utc) - before;
    assert!(
        wait >= chrono::Duration::seconds(29) && wait <= chrono::Duration::seconds(40),
        "次の試行は約 30 秒後: {wait}"
    );
    assert_eq!(
        fx.webhook(id).await.failure_streak,
        0,
        "打ち止めまでは数えない"
    );
}

/// 5 回失敗した配信が 5 本続くと Webhook が止まり、以降の配信は積まれない。
#[tokio::test]
async fn five_exhausted_deliveries_deactivate_webhook() {
    let fx = setup().await;
    let server = mock_server(500).await;
    let id = fx
        .create_webhook(&server.uri(), &["task.created"], "json")
        .await;
    for n in 0..5 {
        fx.create_task(&format!("失敗 {n}")).await;
    }

    let state = job_state(&fx.app);
    for _ in 0..5 {
        // バックオフを待たずに次の試行へ進める
        for delivery in fx.deliveries(id).await {
            if delivery.next_attempt_at.is_some() {
                let mut active: webhook_deliveries::ActiveModel = delivery.into();
                active.next_attempt_at = Set(Some(chrono::Utc::now().into()));
                active.update(&fx.app.state.db).await.expect("advance");
            }
        }
        job::webhook_delivery::send_pending_once(&state)
            .await
            .expect("sweep");
    }

    for delivery in fx.deliveries(id).await {
        assert_eq!(delivery.attempt, 5);
        assert!(delivery.next_attempt_at.is_none(), "5 回で打ち止め");
    }
    let webhook = fx.webhook(id).await;
    assert_eq!(webhook.failure_streak, 5);
    assert!(!webhook.is_active, "5 連続の打ち止めで止まる");
    assert_eq!(
        server.received_requests().await.expect("recorded").len(),
        25
    );

    fx.create_task("止まった後").await;
    assert_eq!(
        fx.deliveries(id).await.len(),
        5,
        "止まった Webhook には積まない"
    );

    // 有効に戻すと数え直す
    let res = fx
        .app
        .put_json_with_session(
            &format!("{}/{id}", fx.webhooks_path()),
            serde_json::json!({"is_active": true}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let webhook = fx.webhook(id).await;
    assert!(webhook.is_active);
    assert_eq!(webhook.failure_streak, 0);
}

/// discord 形式は content / embeds を送り、署名ヘッダを付けない。
#[tokio::test]
async fn discord_format_sends_content_and_embeds_without_signature() {
    let fx = setup().await;
    let server = mock_server(204).await;
    let id = fx
        .create_webhook(&server.uri(), &["task.created"], "discord")
        .await;
    fx.create_task("Discord に流す").await;

    job::webhook_delivery::send_pending_once(&job_state(&fx.app))
        .await
        .expect("sweep");

    assert_eq!(fx.deliveries(id).await[0].status_code, Some(204));
    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(requests.len(), 1);
    assert!(requests[0].headers.get("x-task-signature").is_none());
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("json body");
    assert!(
        body["content"]
            .as_str()
            .expect("content")
            .contains("Discord に流す")
    );
    assert_eq!(body["embeds"][0]["title"], "task.created");
}

/// 再送は同じ payload で新しい配信を作り、元の行は変えない。Member は再送できない。
#[tokio::test]
async fn redeliver_creates_new_delivery() {
    let mut fx = setup().await;
    let (owner, member) = (fx.owner.clone(), fx.member.clone());
    let server = mock_server(200).await;
    let id = fx
        .create_webhook(&server.uri(), &["task.created"], "json")
        .await;
    fx.create_task("再送する").await;
    job::webhook_delivery::send_pending_once(&job_state(&fx.app))
        .await
        .expect("sweep");
    let original = fx.deliveries(id).await.remove(0);
    let redeliver_path = format!(
        "{}/{id}/deliveries/{}/redeliver",
        fx.webhooks_path(),
        original.id
    );

    fx.login(&member).await;
    let denied = fx
        .app
        .post_json_with_session(&redeliver_path, serde_json::json!({}))
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let history = fx
        .app
        .get_with_session(&format!("{}/{id}/deliveries", fx.webhooks_path()))
        .await;
    assert_eq!(history.status(), StatusCode::OK, "Member も履歴は読める");

    fx.login(&owner).await;
    let res = fx
        .app
        .post_json_with_session(&redeliver_path, serde_json::json!({}))
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let created: serde_json::Value = res.json().await.expect("json");
    assert_ne!(created["id"], original.id.to_string());
    assert_eq!(created["payload"], original.payload);
    assert_eq!(created["attempt"], 0);

    let deliveries = fx.deliveries(id).await;
    assert_eq!(deliveries.len(), 2);
    let unchanged = deliveries
        .iter()
        .find(|d| d.id == original.id)
        .expect("original");
    assert_eq!(unchanged, &original, "元の行は変えない");

    let history = fx
        .app
        .get_with_session(&format!("{}/{id}/deliveries?limit=1", fx.webhooks_path()))
        .await;
    let history: serde_json::Value = history.json().await.expect("json");
    assert_eq!(history.as_array().expect("array").len(), 1);
    assert_eq!(history[0]["id"], created["id"], "新しい順");
}
