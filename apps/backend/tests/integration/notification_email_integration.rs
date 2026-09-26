use crate::common::TestApp;
use axum::http::StatusCode;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, prelude::Uuid};

// 通知メール（TASK-226）の統合テスト。
//
// 「email_events に入っている種別だけが送信待ちになり、掃き出しが 1 回だけ送る」ことを
// 固定する。仕様は docs/features/tasks/5.notifications.md §3。

struct Fixture {
    app: TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    status_id: String,
    owner: crate::common::TestUser,
    member: crate::common::TestUser,
}

impl Fixture {
    fn tasks_path(&self) -> String {
        format!(
            "/v1/tenants/{}/projects/{}/tasks",
            self.tenant_id, self.project_id
        )
    }

    fn settings_path(&self) -> String {
        format!("/v1/users/me/notification-settings/{}", self.project_id)
    }

    async fn login(&mut self, user: &crate::common::TestUser) {
        self.app.reset_session_client();
        self.app
            .login_session_no_content(&user.email, &user.password)
            .await;
    }

    /// 対象利用者の通知（新しい順ではなく作成順）。
    async fn notifications_of(&self, user_id: Uuid) -> Vec<entity::notifications::Model> {
        entity::notifications::Entity::find()
            .filter(entity::notifications::Column::UserId.eq(user_id))
            .all(&self.app.state.db)
            .await
            .expect("load notifications")
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

/// オーナーとメンバーがいるプロジェクト（ステータス 1 つ）。
async fn setup() -> Fixture {
    let mut app = TestApp::new().await;
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

/// いまログインしている利用者としてタスクを作り、メンバーを担当に足す。
async fn assign_task(fx: &Fixture, title: &str) {
    let task_resp = fx
        .app
        .post_json_with_session(
            &fx.tasks_path(),
            serde_json::json!({"title": title, "status_id": fx.status_id}),
        )
        .await;
    assert_eq!(task_resp.status(), StatusCode::CREATED);
    let task_id = task_resp.json::<serde_json::Value>().await.expect("json")["id"]
        .as_str()
        .expect("task id")
        .to_string();

    let assigned = fx
        .app
        .post_json_with_session(
            &format!("{}/{task_id}/assignees", fx.tasks_path()),
            serde_json::json!({"user_id": fx.member.id, "role": "primary"}),
        )
        .await;
    assert_eq!(assigned.status(), StatusCode::CREATED);
}

/// `email_events` に入れた種別だけが送信待ちになり、掃き出しは 1 回だけ送る。
#[tokio::test]
async fn queued_notification_is_emailed_once() {
    let mut fx = setup().await;
    let (owner, member) = (fx.owner.clone(), fx.member.clone());

    // 担当になったらメールを受け取る設定にする
    fx.login(&member).await;
    let res = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({
                "email_events": ["assigned"],
                "in_app_events": ["assigned", "status_changed"],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    fx.login(&owner).await;
    assign_task(&fx, "OAuth 対応").await;

    let queued = fx.notifications_of(member.id).await;
    assert_eq!(queued.len(), 1);
    assert!(
        queued[0].email_queued_at.is_some(),
        "email_events に入れた種別は送信待ちになる"
    );
    assert!(queued[0].emailed_at.is_none());

    let state = job_state(&fx.app);
    assert_eq!(
        job::notification_email::send_pending_once(&state)
            .await
            .expect("sweep"),
        1
    );

    let mails = fx.app.sent_mails();
    assert_eq!(mails.len(), 1, "1 件だけ送る");
    assert_eq!(mails[0].to, member.email);
    assert!(
        mails[0].subject.contains("#1 OAuth 対応"),
        "件名: {}",
        mails[0].subject
    );
    assert!(
        mails[0].text.contains("/projects/"),
        "本文にタスクへの導線がある: {}",
        mails[0].text
    );

    let after = fx.notifications_of(member.id).await;
    assert!(after[0].emailed_at.is_some(), "送信済みの印が立つ");

    // 2 周目は送る物が無い
    assert_eq!(
        job::notification_email::send_pending_once(&state)
            .await
            .expect("second sweep"),
        0
    );
    assert_eq!(fx.app.sent_mails().len(), 1, "同じ通知を二度送らない");
}

/// `email_events` に無い種別（既定は空）はメールにならない。
#[tokio::test]
async fn unsubscribed_event_is_not_queued() {
    let mut fx = setup().await;
    let (owner, member) = (fx.owner.clone(), fx.member.clone());

    fx.login(&member).await;
    let res = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({
                "email_events": ["review_round_any"],
                "in_app_events": ["assigned"],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    fx.login(&owner).await;
    assign_task(&fx, "メールにしないタスク").await;

    let queued = fx.notifications_of(member.id).await;
    assert_eq!(queued.len(), 1, "in-app 通知は作られる");
    assert!(
        queued[0].email_queued_at.is_none(),
        "email_events に無い種別は送信待ちにならない"
    );

    let state = job_state(&fx.app);
    assert_eq!(
        job::notification_email::send_pending_once(&state)
            .await
            .expect("sweep"),
        0
    );
    assert!(fx.app.sent_mails().is_empty());
}

/// レビュー通知もメールになる（件名に PR 番号が入る）。
#[tokio::test]
async fn review_round_email_carries_pr_number() {
    assert_review_round_email("review_round_created").await;
}

#[tokio::test]
async fn review_round_subscription_also_enables_email() {
    assert_review_round_email("review_round_any").await;
}

async fn assert_review_round_email(email_event: &str) {
    let mut fx = setup().await;
    let (owner, member) = (fx.owner.clone(), fx.member.clone());

    // プロジェクトの全ラウンドを購読し、メールでも受け取る
    fx.login(&member).await;
    let res = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({
                "email_events": [email_event],
                "in_app_events": ["review_round_any"],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    fx.login(&owner).await;
    let created = fx
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
                "findings": [{ "severity": "high", "title": "認可が抜けている", "body": "根拠" }],
            }),
        )
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);

    let queued = fx.notifications_of(member.id).await;
    assert_eq!(queued.len(), 1);
    assert!(queued[0].email_queued_at.is_some());

    let state = job_state(&fx.app);
    assert_eq!(
        job::notification_email::send_pending_once(&state)
            .await
            .expect("sweep"),
        1
    );
    let mails = fx.app.sent_mails();
    assert_eq!(mails.len(), 1);
    assert!(
        mails[0].subject.contains("PR #618"),
        "件名: {}",
        mails[0].subject
    );
    assert!(
        mails[0].text.contains("pr=618"),
        "本文に指摘一覧への導線がある: {}",
        mails[0].text
    );
}

/// メール未認証の利用者へは送らず、待ち行列からも落とす。
#[tokio::test]
async fn unverified_user_is_dropped_from_the_queue() {
    let mut fx = setup().await;
    let (owner, member) = (fx.owner.clone(), fx.member.clone());

    fx.login(&member).await;
    let res = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({
                "email_events": ["assigned"],
                "in_app_events": ["assigned"],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    entity::users::Entity::update_many()
        .col_expr(
            entity::users::Column::EmailVerified,
            sea_orm::sea_query::Expr::value(false),
        )
        .filter(entity::users::Column::Id.eq(member.id))
        .exec(&fx.app.state.db)
        .await
        .expect("unverify member");

    fx.login(&owner).await;
    assign_task(&fx, "未認証の宛先").await;

    let state = job_state(&fx.app);
    assert_eq!(
        job::notification_email::send_pending_once(&state)
            .await
            .expect("sweep"),
        0
    );
    assert!(
        fx.app.sent_mails().is_empty(),
        "未認証のアドレスへは送らない"
    );

    let after = fx.notifications_of(member.id).await;
    assert!(
        after[0].email_queued_at.is_none(),
        "何度拾っても送れないので待ち行列から落とす"
    );
    assert!(after[0].emailed_at.is_none());
}

/// 通知を作った後にテナントから外れた利用者へは送らず、待ち行列からも落とす。
/// 一覧 API で見えなくなった通知の内容をメールで届けない。
#[tokio::test]
async fn user_who_lost_access_is_dropped_from_the_queue() {
    let mut fx = setup().await;
    let (owner, member) = (fx.owner.clone(), fx.member.clone());

    fx.login(&member).await;
    let res = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({
                "email_events": ["assigned"],
                "in_app_events": ["assigned"],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    fx.login(&owner).await;
    assign_task(&fx, "外れた宛先").await;
    assert!(
        fx.notifications_of(member.id).await[0]
            .email_queued_at
            .is_some(),
        "作成時点では送信待ち"
    );

    entity::tenant_members::Entity::delete_many()
        .filter(entity::tenant_members::Column::TenantId.eq(fx.tenant_id))
        .filter(entity::tenant_members::Column::UserId.eq(member.id))
        .exec(&fx.app.state.db)
        .await
        .expect("remove member from tenant");

    let state = job_state(&fx.app);
    assert_eq!(
        job::notification_email::send_pending_once(&state)
            .await
            .expect("sweep"),
        0
    );
    assert!(
        fx.app.sent_mails().is_empty(),
        "権限を失った利用者へは送らない"
    );

    let after = fx.notifications_of(member.id).await;
    assert!(after[0].email_queued_at.is_none(), "待ち行列から落とす");
    assert!(after[0].emailed_at.is_none());
}

/// 一時障害では再試行時刻を待ち、連続掃き出しだけで打ち止めにならない。
#[tokio::test]
async fn smtp_failures_wait_for_backoff_and_can_recover() {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    use tokio::io::AsyncWriteExt;

    let mut fx = setup().await;
    let (owner, member) = (fx.owner.clone(), fx.member.clone());
    fx.login(&member).await;
    let response = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({"email_events": ["assigned"], "in_app_events": ["assigned"]}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    fx.login(&owner).await;
    assign_task(&fx, "SMTP recovery").await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let _ = stream
                .write_all(b"421 Service temporarily unavailable\r\n")
                .await;
        }
    });
    let mut state = job_state(&fx.app);
    let unavailable =
        service::smtp::SmtpClient::new("127.0.0.1", port, "test", "test", "sender@example.com")
            .unwrap();
    state.smtp_client = unavailable.clone();

    for (index, delay) in [30, 300, 1800, 7200].into_iter().enumerate() {
        let before = chrono::Utc::now();
        assert_eq!(
            job::notification_email::send_pending_once(&state)
                .await
                .unwrap(),
            0
        );
        let notification = fx.notifications_of(member.id).await.remove(0);
        assert_eq!(notification.email_attempts, (index + 1) as i16);
        let next = notification.email_queued_at.unwrap();
        assert!(next >= before + chrono::Duration::seconds(delay));
        assert!(next <= chrono::Utc::now() + chrono::Duration::seconds(delay));
        assert!(notification.emailed_at.is_none());
        for _ in 0..6 {
            assert_eq!(
                job::notification_email::send_pending_once(&state)
                    .await
                    .unwrap(),
                0
            );
        }
        assert_eq!(
            fx.notifications_of(member.id).await[0].email_attempts,
            notification.email_attempts
        );
        let mut active: entity::notifications::ActiveModel = notification.into();
        active.email_queued_at = Set(Some(
            (chrono::Utc::now() - chrono::Duration::seconds(1)).into(),
        ));
        active.update(&state.db).await.unwrap();
    }
    // 5 回目の前に SMTP が回復すれば、待っていたメールを一度だけ送れる。
    state.smtp_client = fx.app.state.smtp_client.clone();
    assert_eq!(
        job::notification_email::send_pending_once(&state)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        job::notification_email::send_pending_once(&state)
            .await
            .unwrap(),
        0
    );
    assert_eq!(fx.app.sent_mails().len(), 1);
    assert!(fx.notifications_of(member.id).await[0].emailed_at.is_some());

    // 障害が続く場合も、間隔を空けた 5 回で打ち止めになり、それ以上は送らない。
    assign_task(&fx, "SMTP exhausted").await;
    state.smtp_client = unavailable;
    for attempt in 1..=job::notification_email::MAX_ATTEMPTS {
        assert_eq!(
            job::notification_email::send_pending_once(&state)
                .await
                .unwrap(),
            0
        );
        let notification = fx
            .notifications_of(member.id)
            .await
            .into_iter()
            .find(|n| n.emailed_at.is_none())
            .unwrap();
        assert_eq!(notification.email_attempts, attempt);
        if attempt == job::notification_email::MAX_ATTEMPTS {
            assert!(
                notification.email_queued_at.is_none(),
                "打ち止めた行は待ち行列（送信待ちの部分インデックス）から落とす"
            );
            break;
        }
        let mut active: entity::notifications::ActiveModel = notification.into();
        active.email_queued_at = Set(Some(
            (chrono::Utc::now() - chrono::Duration::seconds(1)).into(),
        ));
        active.update(&state.db).await.unwrap();
    }
    state.smtp_client = fx.app.state.smtp_client.clone();
    assert_eq!(
        job::notification_email::send_pending_once(&state)
            .await
            .unwrap(),
        0
    );
    assert_eq!(fx.app.sent_mails().len(), 1);
    server.abort();
}
