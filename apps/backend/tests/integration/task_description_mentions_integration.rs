use crate::common::{TestApp, TestUser};
use axum::http::StatusCode;
use entity::{notifications, users, webhook_deliveries};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    prelude::Uuid,
};
use serde_json::Value;

// タスクの説明文のメンション（`<@利用者ID>` の印）の統合テスト。
// 作成と編集で増えた印の者だけに通知すること・宛先の境がコメントと同じこと・既存の素の
// `@username` では通知しないこと・応答の対応表・外れた者や退会した者の名前の範囲・
// 外へ出る口（検索の抜粋・コメントの webhook）に印が残らぬことを固定する。

struct Fx {
    app: TestApp,
    project_id: Uuid,
    tenant_id: Uuid,
    status_id: String,
    owner: TestUser,
}

async fn setup() -> Fx {
    let mut app = TestApp::new_with_webhook_loopback(true).await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let status = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/statuses",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({"name":"Backlog","color":"#336699","position":0,"is_default":true}),
        )
        .await;
    assert_eq!(status.status(), StatusCode::CREATED);
    let status_id = status.json::<Value>().await.expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();
    Fx {
        project_id: tp.project_id,
        tenant_id: tp.tenant_id,
        status_id,
        app,
        owner,
    }
}

fn mark(user: &TestUser) -> String {
    format!("<@{}>", user.id)
}

impl Fx {
    fn tasks(&self) -> String {
        format!(
            "/v1/tenants/{}/projects/{}/tasks",
            self.tenant_id, self.project_id
        )
    }

    /// テナントに入っている（プロジェクトにも入れる）利用者。
    async fn member(&self) -> TestUser {
        let user = self.app.insert_user(false, false).await;
        crate::common::ensure_tenant_member_for_project(
            &self.app.state.db,
            self.project_id,
            user.id,
        )
        .await;
        user
    }

    async fn username(&self, user: &TestUser) -> String {
        users::Entity::find_by_id(user.id)
            .one(&self.app.state.db)
            .await
            .expect("query")
            .expect("user")
            .username
    }

    async fn rename(&self, user: &TestUser, name: &str) {
        let model = users::Entity::find_by_id(user.id)
            .one(&self.app.state.db)
            .await
            .expect("query")
            .expect("user");
        let mut active: users::ActiveModel = model.into();
        active.username = Set(name.to_string());
        active.update(&self.app.state.db).await.expect("rename");
    }

    /// 説明文のメンション通知（コメントのものは `comment_id` を持つので分けて数える）。
    async fn description_mentions(&self, user: &TestUser) -> Vec<notifications::Model> {
        notifications::Entity::find()
            .filter(notifications::Column::UserId.eq(user.id))
            .filter(notifications::Column::NotificationType.eq("mentioned"))
            .all(&self.app.state.db)
            .await
            .expect("notifications")
            .into_iter()
            .filter(|n| n.payload["source"] == "task_description")
            .collect()
    }

    async fn any_mentioned(&self, user: &TestUser) -> usize {
        notifications::Entity::find()
            .filter(notifications::Column::UserId.eq(user.id))
            .filter(notifications::Column::NotificationType.eq("mentioned"))
            .all(&self.app.state.db)
            .await
            .expect("notifications")
            .len()
    }

    async fn create(&self, description: Option<&str>) -> Value {
        let mut body = serde_json::json!({"title":"説明文のメンション","status_id":self.status_id});
        if let Some(d) = description {
            body["description"] = d.into();
        }
        let res = self.app.post_json_with_session(&self.tasks(), body).await;
        assert_eq!(res.status(), StatusCode::CREATED);
        res.json().await.expect("json")
    }

    async fn update(&self, task_id: &str, body: Value) -> Value {
        let res = self
            .app
            .put_json_with_session(&format!("{}/{task_id}", self.tasks()), body)
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        res.json().await.expect("json")
    }

    async fn detail(&self, task_id: &str) -> Value {
        let res = self
            .app
            .get_with_session(&format!("{}/{task_id}", self.tasks()))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        res.json().await.expect("json")
    }

    /// 担当者にする（その者の足跡がテナントに残る＝後で外れても「属しておった」と引ける）。
    async fn assign(&self, task_id: &str, user: &TestUser) {
        self.update(
            task_id,
            serde_json::json!({"assignees":[{"user_id":user.id,"role":"Member"}]}),
        )
        .await;
    }

    /// テナントから除名する（`tenant_members` の行が消える。`users` の行は残る）。
    async fn remove_from_tenant(&self, user: &TestUser) {
        let res = self
            .app
            .delete_with_session(&format!(
                "/v1/tenants/{}/members/{}",
                self.tenant_id, user.id
            ))
            .await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }
}

#[tokio::test]
async fn create_notifies_members_but_not_outsiders_unknown_ids_or_the_author() {
    let fx = setup().await;
    let alice = fx.member().await;
    let stranger = fx.app.insert_user(false, false).await;
    let ghost = Uuid::new_v4();

    let created = fx
        .create(Some(&format!(
            "{} {} <@{ghost}> {} 見て",
            mark(&alice),
            mark(&stranger),
            mark(&fx.owner)
        )))
        .await;

    let notified = fx.description_mentions(&alice).await;
    assert_eq!(notified.len(), 1, "プロジェクトに入れる者へは届く");
    assert_eq!(
        notified[0].task_id.map(|t| t.to_string()),
        created["id"].as_str().map(str::to_string),
        "遷移先はそのタスク"
    );
    assert!(
        notified[0].payload.get("comment_id").is_none(),
        "コメントを指さない"
    );
    assert_eq!(fx.any_mentioned(&stranger).await, 0, "境の外へは通知しない");
    assert_eq!(
        fx.any_mentioned(&fx.owner).await,
        0,
        "自分自身には通知しない"
    );

    // 本文は印のまま保存され、応答に対応表が並ぶ
    assert!(
        created["description"]
            .as_str()
            .unwrap()
            .contains(&mark(&alice))
    );
    let alice_name = fx.username(&alice).await;
    assert_eq!(
        created["mentions"][alice.id.to_string()]["status"],
        "member"
    );
    assert_eq!(
        created["mentions"][alice.id.to_string()]["name"],
        alice_name
    );
    assert_eq!(
        created["mentions"][stranger.id.to_string()]["status"],
        "outside"
    );
    assert!(
        created["mentions"][stranger.id.to_string()]["name"].is_null(),
        "別のテナントの利用者の名前は返さない"
    );
    assert_eq!(created["mentions"][ghost.to_string()]["status"], "gone");
}

#[tokio::test]
async fn edit_notifies_only_newly_added_markers() {
    let fx = setup().await;
    let alice = fx.member().await;
    let bob = fx.member().await;

    let created = fx.create(Some(&format!("{} 見て", mark(&alice)))).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(fx.description_mentions(&alice).await.len(), 1);

    let edited = fx
        .update(
            &id,
            serde_json::json!({"description": format!("{} と {} 見て", mark(&alice), mark(&bob))}),
        )
        .await;

    assert_eq!(
        fx.description_mentions(&alice).await.len(),
        1,
        "編集前からの印では再通知しない"
    );
    assert_eq!(
        fx.description_mentions(&bob).await.len(),
        1,
        "編集で増えた印だけ通知する"
    );
    assert_eq!(edited["mentions"][bob.id.to_string()]["status"], "member");

    // 説明に触れぬ更新・説明を消す更新では通知しない
    fx.update(&id, serde_json::json!({"title":"題だけ変える"}))
        .await;
    fx.update(&id, serde_json::json!({"clear_description": true}))
        .await;
    assert_eq!(fx.description_mentions(&alice).await.len(), 1);
    assert_eq!(fx.description_mentions(&bob).await.len(), 1);
}

#[tokio::test]
async fn plain_username_text_in_a_description_notifies_nobody_even_on_edit() {
    let fx = setup().await;
    let alice = fx.member().await;
    let name = fx.username(&alice).await;

    // 既存の説明文（旧の書き方の `@username`）は移し替えず、そのまま返る
    let created = fx.create(Some(&format!("@{name} 旧い形の説明"))).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(fx.any_mentioned(&alice).await, 0);
    assert_eq!(created["description"], format!("@{name} 旧い形の説明"));
    assert!(created["mentions"].as_object().unwrap().is_empty());

    // 編集してもその中の素の `@username` は名前で解かれず、通知は飛ばない
    fx.update(
        &id,
        serde_json::json!({"description": format!("@{name} 旧い形の説明（追記） x@{name}")}),
    )
    .await;
    assert_eq!(fx.any_mentioned(&alice).await, 0);
    let again = fx.detail(&id).await;
    assert_eq!(
        again["description"],
        format!("@{name} 旧い形の説明（追記） x@{name}")
    );
    assert!(again["mentions"].as_object().unwrap().is_empty());
}

#[tokio::test]
async fn detail_returns_current_names_after_a_rename() {
    let fx = setup().await;
    let alice = fx.member().await;
    let created = fx.create(Some(&format!("{} 確認", mark(&alice)))).await;
    let id = created["id"].as_str().unwrap().to_string();

    fx.rename(&alice, "alice_renamed").await;
    let detail = fx.detail(&id).await;

    assert_eq!(
        detail["mentions"][alice.id.to_string()]["name"],
        "alice_renamed",
        "改名しても切れず、いまの名が返る"
    );
    assert!(
        detail["description"]
            .as_str()
            .unwrap()
            .contains(&mark(&alice)),
        "本文は印のまま"
    );
}

#[tokio::test]
async fn removed_member_keeps_their_name_but_is_not_notified() {
    let fx = setup().await;
    let alice = fx.member().await;
    let base = fx.create(None).await;
    // テナントに足跡を残してから除名する
    fx.assign(base["id"].as_str().unwrap(), &alice).await;
    fx.remove_from_tenant(&alice).await;

    let created = fx
        .create(Some(&format!("{} まだ名前は出る", mark(&alice))))
        .await;

    let alice_name = fx.username(&alice).await;
    let entry = &created["mentions"][alice.id.to_string()];
    assert_eq!(entry["status"], "outside", "状態の区別は保つ");
    assert_eq!(entry["name"], alice_name, "外れた者でも名前は消えない");
    assert!(
        entry["avatar_url"].is_null(),
        "名前以外（アイコン）は返さない"
    );
    assert_eq!(
        fx.any_mentioned(&alice).await,
        0,
        "外れた者には通知しない（今の振る舞い）"
    );
}

#[tokio::test]
async fn user_of_another_tenant_never_gets_a_name() {
    let fx = setup().await;
    // 別のテナントに属し、そこで活動している利用者
    let other_owner = fx.app.insert_user(false, false).await;
    let other = fx.app.insert_tenant_project(other_owner.id).await;
    assert_ne!(other.tenant_id, fx.tenant_id);

    let created = fx
        .create(Some(&format!("{} 他のテナントの人", mark(&other_owner))))
        .await;

    let entry = &created["mentions"][other_owner.id.to_string()];
    assert_eq!(entry["status"], "outside");
    assert!(
        entry["name"].is_null(),
        "印の id を知っただけで別のテナントの利用者の名前は引けない: {entry}"
    );
    // 返りの JSON のどこにも、その名前が出ない
    let other_name = fx.username(&other_owner).await;
    assert!(!created.to_string().contains(&other_name));
}

#[tokio::test]
async fn removed_member_without_any_trace_has_no_name() {
    // tenant_members は除名で行ごと消え、所属の履歴は残らない。足跡（担当・コメント・作成など）の
    // 無い者は「属しておった」と引けず、名前は伏せたまま。ここが名前の範囲の境
    let fx = setup().await;
    let alice = fx.member().await;
    fx.remove_from_tenant(&alice).await;

    let created = fx.create(Some(&format!("{} 足跡なし", mark(&alice)))).await;

    let entry = &created["mentions"][alice.id.to_string()];
    assert_eq!(entry["status"], "outside");
    assert!(entry["name"].is_null(), "{entry}");
}

#[tokio::test]
async fn search_snippet_never_shows_markers() {
    let fx = setup().await;
    let alice = fx.member().await;
    let alice_name = fx.username(&alice).await;
    fx.create(Some(&format!("OAuth の確認は {} にお願い", mark(&alice))))
        .await;

    let res = fx
        .app
        .get_with_session(&format!("{}/search?q=OAuth", fx.tasks()))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: Value = res.json().await.expect("json");
    let hits = body["tasks"].as_array().expect("tasks");
    assert_eq!(hits.len(), 1);
    let highlight = hits[0]["highlight"].as_str().unwrap();
    assert!(!highlight.contains("<@"), "{highlight}");
    assert!(
        !highlight.contains(&alice.id.to_string()),
        "id が漏れる: {highlight}"
    );
    assert!(highlight.contains(&format!("@{alice_name}")), "{highlight}");
}

#[tokio::test]
async fn comment_webhook_shows_the_name_of_a_removed_member() {
    let fx = setup().await;
    let alice = fx.member().await;
    let base = fx.create(None).await;
    let task_id = base["id"].as_str().unwrap().to_string();
    fx.assign(&task_id, &alice).await;
    fx.remove_from_tenant(&alice).await;
    let stranger = fx.app.insert_user(false, false).await;

    let hook = fx
        .app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/webhooks",
                fx.tenant_id, fx.project_id
            ),
            serde_json::json!({
                "url": "http://127.0.0.1:9/hook",
                "secret": "0123456789abcdef-secret",
                "events": ["comment.created"],
                "format": "json"
            }),
        )
        .await;
    assert_eq!(hook.status(), StatusCode::CREATED);
    let hook_id: Uuid = hook.json::<Value>().await.expect("json")["id"]
        .as_str()
        .unwrap()
        .parse()
        .expect("uuid");

    let res = fx
        .app
        .post_json_with_session(
            &format!("{}/{task_id}/comments", fx.tasks()),
            serde_json::json!({"body": format!("{} {} 確認", mark(&alice), mark(&stranger))}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let deliveries = webhook_deliveries::Entity::find()
        .filter(webhook_deliveries::Column::WebhookId.eq(hook_id))
        .order_by_asc(webhook_deliveries::Column::CreatedAt)
        .all(&fx.app.state.db)
        .await
        .expect("deliveries");
    assert_eq!(deliveries.len(), 1);
    let payload = deliveries[0].payload.to_string();
    assert!(!payload.contains("<@"), "webhook に印が残る: {payload}");
    assert!(
        !payload.contains(&alice.id.to_string()) && !payload.contains(&stranger.id.to_string()),
        "webhook に利用者 id が漏れる: {payload}"
    );
    let body = deliveries[0].payload["comment"]["body"].as_str().unwrap();
    let alice_name = fx.username(&alice).await;
    let stranger_name = fx.username(&stranger).await;
    assert!(
        body.contains(&format!("@{alice_name}")),
        "外れた者の名前が webhook に出る: {body}"
    );
    assert!(
        !body.contains(&stranger_name),
        "別のテナントの利用者の名前は出ない: {body}"
    );
    assert!(body.contains("@（このプロジェクトの外の利用者）"), "{body}");
}
