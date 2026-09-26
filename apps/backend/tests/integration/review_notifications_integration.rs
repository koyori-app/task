use crate::common::TestApp;
use axum::http::StatusCode;
use entity::{oauth_connections, project_statuses, reviews, scopes::Scope};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, prelude::Uuid,
};

// レビュー通知（TASK-224）の統合テスト。
//
// 「誰に届いて、誰に届かないか」を固定する。受信者の規則は
// docs/features/tasks/5.notifications.md §3 の「レビュー通知の受信者」。

struct Fixture {
    app: TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    reviewer: crate::common::TestUser,
    developer: crate::common::TestUser,
}

impl Fixture {
    fn reviews_path(&self) -> String {
        format!(
            "/v1/tenants/{}/projects/{}/reviews",
            self.tenant_id, self.project_id
        )
    }

    fn findings_path(&self) -> String {
        format!(
            "/v1/tenants/{}/projects/{}/review-findings",
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
}

/// レビュワー（テナントオーナー）と修正者（テナントメンバー）がいるプロジェクト。
async fn setup() -> Fixture {
    let mut app = TestApp::new().await;
    let reviewer = app.insert_user_default().await;
    let developer = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;

    for (name, position, is_default, is_done) in
        [("Todo", 0, true, false), ("Done", 1, false, true)]
    {
        project_statuses::ActiveModel {
            id: Set(Uuid::new_v4()),
            project_id: Set(tp.project_id),
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

    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;
    let added = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/members", tp.tenant_id),
            serde_json::json!({ "user_id": developer.id, "role": "Member" }),
        )
        .await;
    assert_eq!(added.status(), StatusCode::CREATED);

    Fixture {
        app,
        tenant_id: tp.tenant_id,
        project_id: tp.project_id,
        reviewer,
        developer,
    }
}

fn username_of(user: &crate::common::TestUser) -> String {
    format!("test_{}", &user.id.to_string()[..8])
}

fn round_body(pr: i32, severity: &str, title: &str) -> serde_json::Value {
    serde_json::json!({
        "pr_number": pr,
        "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
        "summary": "総評",
        "findings": [{
            "severity": severity,
            "title": title,
            "body": "再現条件と根拠",
        }],
    })
}

/// いまログインしている利用者としてラウンドを 1 本起票し、指摘の ID を返す。
async fn submit_round(fx: &Fixture, pr: i32, severity: &str, title: &str) -> String {
    let res = fx
        .app
        .post_json_with_session(&fx.reviews_path(), round_body(pr, severity, title))
        .await;
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "ラウンドの起票は成功する"
    );
    let body: serde_json::Value = res.json().await.expect("json body");
    body["findings"][0]["id"]
        .as_str()
        .expect("finding id")
        .to_string()
}

async fn transition(fx: &Fixture, finding_id: &str, state: &str) {
    let res = fx
        .app
        .patch_json_with_session(
            &format!("{}/{finding_id}", fx.findings_path()),
            serde_json::json!({ "state": state }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK, "状態遷移は成功する");
}

/// その利用者の通知一覧（新しい順）。呼ぶとセッションがその利用者に切り替わる。
async fn notifications(fx: &mut Fixture, user: &crate::common::TestUser) -> Vec<serde_json::Value> {
    fx.login(user).await;
    let res = fx.app.get_with_session("/v1/users/me/notifications").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = res.json().await.expect("json body");
    body["notifications"]
        .as_array()
        .expect("notifications array")
        .clone()
}

/// 購読していない人にはラウンドの起票は届かない。購読すると届く（起票者本人は除く）。
#[tokio::test]
async fn round_creation_reaches_subscribers_only() {
    let mut fx = setup().await;
    let (reviewer, developer) = (fx.reviewer.clone(), fx.developer.clone());

    fx.login(&reviewer).await;
    submit_round(&fx, 618, "high", "認可が抜けている").await;

    // R1 の関係者は起票者本人だけ。誰にも通知は出ない
    assert!(notifications(&mut fx, &developer).await.is_empty());
    assert!(notifications(&mut fx, &reviewer).await.is_empty());

    // 修正者がプロジェクトの全ラウンドを購読する
    fx.login(&developer).await;
    let res = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({
                "email_events": [],
                "in_app_events": ["review_round_any"],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    fx.login(&reviewer).await;
    submit_round(&fx, 618, "low", "命名が実装を説明していない").await;

    let items = notifications(&mut fx, &developer).await;
    assert_eq!(items.len(), 1, "購読者へ R2 の起票が 1 件届く");
    assert_eq!(items[0]["notification_type"], "review_round_created");
    assert_eq!(items[0]["project_id"], fx.project_id.to_string());
    assert!(
        items[0]["task"].is_null(),
        "レビュー通知はタスクに紐づかない"
    );
    assert_eq!(items[0]["payload"]["round"], 2);
    assert_eq!(items[0]["payload"]["pr_number"], 618);
    assert_eq!(items[0]["payload"]["counts"]["low"], 1);
    assert_eq!(items[0]["payload"]["reviewer"], username_of(&reviewer));

    assert!(
        notifications(&mut fx, &reviewer).await.is_empty(),
        "起票した本人には届かない"
    );
}

/// 関係者でも購読者でも、1 ラウンドにつき通知は 1 件（重複しない）。
#[tokio::test]
async fn a_participant_who_also_subscribes_gets_one_notification() {
    let mut fx = setup().await;
    let (reviewer, developer) = (fx.reviewer.clone(), fx.developer.clone());

    fx.login(&reviewer).await;
    let finding_id = submit_round(&fx, 700, "high", "認可が抜けている").await;

    // 修正を宣言して関係者になり、さらに購読もする
    fx.login(&developer).await;
    transition(&fx, &finding_id, "fixed").await;
    let res = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({
                "email_events": [],
                "in_app_events": ["review_round_created", "review_round_any"],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    fx.login(&reviewer).await;
    submit_round(&fx, 700, "low", "命名").await;

    let items = notifications(&mut fx, &developer).await;
    let rounds: Vec<&serde_json::Value> = items
        .iter()
        .filter(|n| n["notification_type"] == "review_round_created")
        .collect();
    assert_eq!(rounds.len(), 1, "関係者かつ購読者でも 1 件だけ");
}

/// 状態遷移は相手側へ届き、動かした本人には届かない。
#[tokio::test]
async fn finding_transitions_notify_the_other_side() {
    let mut fx = setup().await;
    let (reviewer, developer) = (fx.reviewer.clone(), fx.developer.clone());

    fx.login(&reviewer).await;
    let finding_id = submit_round(&fx, 618, "high", "認可が抜けている").await;

    fx.login(&developer).await;
    transition(&fx, &finding_id, "fixed").await;

    let items = notifications(&mut fx, &reviewer).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["notification_type"], "review_finding_changed");
    assert_eq!(items[0]["project_id"], fx.project_id.to_string());
    assert_eq!(items[0]["payload"]["from"], "open");
    assert_eq!(items[0]["payload"]["to"], "fixed");
    assert_eq!(items[0]["payload"]["severity"], "high");
    assert_eq!(items[0]["payload"]["title"], "認可が抜けている");
    assert_eq!(items[0]["payload"]["actor"], username_of(&developer));
    assert!(
        notifications(&mut fx, &developer).await.is_empty(),
        "宣言した本人には届かない"
    );

    // 確認は修正者（fixed_by として関係者）へ届く
    fx.login(&reviewer).await;
    transition(&fx, &finding_id, "verified").await;

    let items = notifications(&mut fx, &developer).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["payload"]["to"], "verified");
    assert_eq!(items[0]["payload"]["actor"], username_of(&reviewer));
}

/// PAT（AI レビュワーの経路）で起票しても通知は同じように出る。
#[tokio::test]
async fn a_round_submitted_with_a_pat_notifies_the_same_way() {
    let mut fx = setup().await;
    let (reviewer, developer) = (fx.reviewer.clone(), fx.developer.clone());

    fx.login(&developer).await;
    let res = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({
                "email_events": [],
                "in_app_events": ["review_round_any"],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let token = fx
        .app
        .insert_pat(reviewer.id, fx.tenant_id, vec![Scope::WriteReview], None)
        .await;
    let res = fx
        .app
        .post_json_with_bearer(
            &fx.reviews_path(),
            round_body(618, "medium", "境界値が未検証"),
            &token,
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let items = notifications(&mut fx, &developer).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["notification_type"], "review_round_created");
    assert_eq!(items[0]["payload"]["counts"]["medium"], 1);
}

/// PR の作者は、ラウンドに控えた login から解決できれば関係者になる。
#[tokio::test]
async fn the_pull_request_author_becomes_a_participant() {
    let mut fx = setup().await;
    let (reviewer, developer) = (fx.reviewer.clone(), fx.developer.clone());

    // 修正者の GitHub 接続（保存側は小文字で控える）
    let now = chrono::Utc::now();
    oauth_connections::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(developer.id),
        provider: Set("github".into()),
        provider_user_id: Set(format!("gh-{}", developer.id)),
        provider_email: Set(None),
        instance_url: Set(None),
        access_token_enc: Set(None),
        refresh_token_enc: Set(None),
        token_expires_at: Set(None),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        provider_login: Set(Some("dev-login".into())),
    }
    .insert(&fx.app.state.db)
    .await
    .expect("insert oauth connection");

    fx.login(&reviewer).await;
    submit_round(&fx, 618, "high", "認可が抜けている").await;

    // 要約ジョブが PR の作者を控えた状態にする（大小の違いは解決側で吸収する）
    reviews::Entity::update_many()
        .col_expr(
            reviews::Column::PrAuthor,
            sea_orm::sea_query::Expr::value(Some("Dev-Login")),
        )
        .filter(reviews::Column::ProjectId.eq(fx.project_id))
        .exec(&fx.app.state.db)
        .await
        .expect("cache pr author");

    fx.login(&reviewer).await;
    submit_round(&fx, 618, "low", "命名").await;

    let items = notifications(&mut fx, &developer).await;
    assert_eq!(items.len(), 1, "購読していなくても PR の作者には届く");
    assert_eq!(items[0]["notification_type"], "review_round_created");
    assert_eq!(items[0]["payload"]["round"], 2);
}

/// 通知設定はレビューの種別を受け付け、未知の種別は拒む。
#[tokio::test]
async fn notification_settings_accept_the_review_event_types() {
    let mut fx = setup().await;
    let developer = fx.developer.clone();
    fx.login(&developer).await;

    let ok = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({
                "email_events": [],
                "in_app_events": [
                    "review_round_created",
                    "review_finding_changed",
                    "review_round_any",
                ],
            }),
        )
        .await;
    assert_eq!(ok.status(), StatusCode::OK);

    let invalid = fx
        .app
        .put_json_with_session(
            &fx.settings_path(),
            serde_json::json!({
                "email_events": [],
                "in_app_events": ["review_round_anyy"],
            }),
        )
        .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
}

/// 繰り延べの通知は遷移先の `deferred_task_id` を持つ。他の遷移には付けない。
#[tokio::test]
async fn deferral_notification_carries_the_deferred_task() {
    let mut fx = setup().await;
    let (reviewer, developer) = (fx.reviewer.clone(), fx.developer.clone());

    fx.login(&reviewer).await;
    let finding_id = submit_round(&fx, 640, "low", "命名").await;

    fx.login(&developer).await;
    transition(&fx, &finding_id, "deferred").await;
    let deferred_task_id = entity::review_findings::Entity::find_by_id(
        finding_id.parse::<Uuid>().expect("finding uuid"),
    )
    .one(&fx.app.state.db)
    .await
    .expect("finding")
    .expect("finding exists")
    .deferred_task_id
    .expect("deferred task");

    let items = notifications(&mut fx, &reviewer).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["payload"]["to"], "deferred");
    assert_eq!(
        items[0]["payload"]["deferred_task_id"],
        deferred_task_id.to_string()
    );
    assert_eq!(items[0]["project"]["id"], fx.project_id.to_string());
    assert_eq!(items[0]["project"]["tenant_id"], fx.tenant_id.to_string());

    // 対照: deferred → open（取り消し）の通知には付けない
    fx.login(&developer).await;
    transition(&fx, &finding_id, "open").await;
    let items = notifications(&mut fx, &reviewer).await;
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["payload"]["to"], "open");
    assert!(items[0]["payload"].get("deferred_task_id").is_none());
}
