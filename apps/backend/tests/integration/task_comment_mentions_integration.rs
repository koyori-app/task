use crate::common::{TestApp, TestUser};
use axum::http::StatusCode;
use entity::{notifications, users, webhook_deliveries};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    prelude::Uuid,
};
use serde_json::Value;

// コメントのメンション（`<@利用者ID>` の印）の統合テスト。
// 印だけを拾うこと・宛先の境・編集で増えた分だけ通知すること・本文が外へ出る口（webhook）で
// 印を名に戻すこと・API が名前の対応表を返すことを固定する。

struct Fx {
    app: TestApp,
    project_id: Uuid,
    tenant_id: Uuid,
    comments: String,
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
    let task = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/tasks",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({"title":"Mention task","status_id":status_id}),
        )
        .await;
    assert_eq!(task.status(), StatusCode::CREATED);
    let task_id = task.json::<Value>().await.expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();
    Fx {
        comments: format!(
            "/v1/tenants/{}/projects/{}/tasks/{}/comments",
            tp.tenant_id, tp.project_id, task_id
        ),
        project_id: tp.project_id,
        tenant_id: tp.tenant_id,
        app,
        owner,
    }
}

impl Fx {
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

    /// テナントに入っておらぬ利用者（このプロジェクトの外）。
    async fn outsider(&self) -> TestUser {
        self.app.insert_user(false, false).await
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

    async fn mentioned(&self, user: &TestUser) -> usize {
        notifications::Entity::find()
            .filter(notifications::Column::UserId.eq(user.id))
            .filter(notifications::Column::NotificationType.eq("mentioned"))
            .all(&self.app.state.db)
            .await
            .expect("notifications")
            .len()
    }

    async fn post(&self, body: &str) -> Value {
        let res = self
            .app
            .post_json_with_session(&self.comments, serde_json::json!({ "body": body }))
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        res.json().await.expect("json")
    }

    async fn edit(&self, comment_id: &str, body: &str) -> Value {
        let res = self
            .app
            .put_json_with_session(
                &format!("{}/{comment_id}", self.comments),
                serde_json::json!({ "body": body }),
            )
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        res.json().await.expect("json")
    }

    async fn list(&self) -> Value {
        let res = self.app.get_with_session(&self.comments).await;
        assert_eq!(res.status(), StatusCode::OK);
        res.json().await.expect("json")
    }
}

fn mark(user: &TestUser) -> String {
    format!("<@{}>", user.id)
}

#[tokio::test]
async fn marker_notifies_members_and_ignores_outsiders_and_unknown_ids() {
    let fx = setup().await;
    let alice = fx.member().await;
    let outsider = fx.outsider().await;
    let ghost = Uuid::new_v4();

    let created = fx
        .post(&format!(
            "{} {} <@{ghost}> お願いします",
            mark(&alice),
            mark(&outsider)
        ))
        .await;

    assert_eq!(
        fx.mentioned(&alice).await,
        1,
        "プロジェクトに入れる者へは届く"
    );
    assert_eq!(fx.mentioned(&outsider).await, 0, "境の外の者へは通知しない");

    // 本文は印のまま保存される（境の外・在らぬ id も拒まず、素の文字として扱わず、そのまま残す）
    let body = created["body"].as_str().unwrap();
    assert!(body.contains(&mark(&alice)) && body.contains(&mark(&outsider)));

    // 返りの対応表: member は名前つき、境の外は名前を伏せ、在らぬ id は gone
    let mentions = &created["mentions"];
    let alice_name = fx.username(&alice).await;
    assert_eq!(mentions[alice.id.to_string()]["status"], "member");
    assert_eq!(mentions[alice.id.to_string()]["name"], alice_name);
    assert_eq!(mentions[outsider.id.to_string()]["status"], "outside");
    assert!(
        mentions[outsider.id.to_string()]["name"].is_null(),
        "境の外の者の名前は返さない"
    );
    assert_eq!(mentions[ghost.to_string()]["status"], "gone");
}

#[tokio::test]
async fn tenant_owner_outside_explicit_member_list_can_be_mentioned() {
    // メンバーを明示したプロジェクトでも、テナント owner は宛先に含める（今の境と同じ）。
    // 他方、明示メンバーに入っておらぬテナントメンバーは境の外になる。
    let mut fx = setup().await;
    let named = fx.member().await;
    let unnamed = fx.member().await;
    let add = fx
        .app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/members",
                fx.tenant_id, fx.project_id
            ),
            serde_json::json!({"user_id": named.id, "role": "Member"}),
        )
        .await;
    assert_eq!(add.status(), StatusCode::CREATED);

    // named として投稿し、owner と unnamed を指す
    fx.app.reset_session_client();
    fx.app
        .login_session_no_content(&named.email, &named.password)
        .await;
    let created = fx
        .post(&format!("{} {}", mark_id(fx.owner.id), mark(&unnamed)))
        .await;

    assert_eq!(fx.mentioned(&fx.owner).await, 1, "テナント owner へは届く");
    assert_eq!(fx.mentioned(&unnamed).await, 0, "明示メンバーの外は境の外");
    assert_eq!(
        created["mentions"][unnamed.id.to_string()]["status"],
        "outside"
    );
}

fn mark_id(id: Uuid) -> String {
    format!("<@{id}>")
}

#[tokio::test]
async fn plain_username_text_in_a_new_body_notifies_nobody() {
    let fx = setup().await;
    let alice = fx.member().await;
    let name = fx.username(&alice).await;

    // `@名前`、メールアドレス中の `@名前`（旧の正規表現は後ろの語を拾った）
    let created = fx
        .post(&format!("@{name} さん、x@{name} にも連絡してください"))
        .await;

    assert_eq!(
        fx.mentioned(&alice).await,
        0,
        "名前の文字ではメンションにならない"
    );
    assert_eq!(
        created["body"],
        format!("@{name} さん、x@{name} にも連絡してください"),
        "本文はそのまま保存される"
    );
    assert!(
        created["mentions"].as_object().unwrap().is_empty(),
        "印が無ければ対応表は空"
    );
}

#[tokio::test]
async fn edit_notifies_only_newly_added_markers() {
    let fx = setup().await;
    let alice = fx.member().await;
    let bob = fx.member().await;

    let created = fx.post(&format!("{} 見て", mark(&alice))).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(fx.mentioned(&alice).await, 1);

    let edited = fx
        .edit(&id, &format!("{} と {} 見て", mark(&alice), mark(&bob)))
        .await;

    assert_eq!(
        fx.mentioned(&alice).await,
        1,
        "編集前からの印では再通知しない"
    );
    assert_eq!(fx.mentioned(&bob).await, 1, "編集で増えた印だけ通知する");
    assert_eq!(edited["mentions"][bob.id.to_string()]["status"], "member");
}

#[tokio::test]
async fn same_name_users_only_the_chosen_one_is_notified() {
    let fx = setup().await;
    let first = fx.member().await;
    let second = fx.member().await;
    fx.rename(&first, "alice").await;
    fx.rename(&second, "alice").await;

    fx.post(&format!("{} よろしく", mark(&second))).await;

    assert_eq!(fx.mentioned(&second).await, 1, "選んだ一人には届く");
    assert_eq!(fx.mentioned(&first).await, 0, "同名の別の者には届かない");
}

#[tokio::test]
async fn legacy_plain_comments_stay_as_they_are_and_do_not_notify_on_edit() {
    let fx = setup().await;
    let alice = fx.member().await;
    let name = fx.username(&alice).await;

    // 既存の投稿（旧の書き方の `@username`）を、移し替えず本文のままの行として作る
    let created = fx.post(&format!("@{name} 旧い形の本文")).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(fx.mentioned(&alice).await, 0);

    let listed = fx.list().await;
    assert_eq!(
        listed["comments"][0]["body"],
        format!("@{name} 旧い形の本文"),
        "既存の本文は今のまま返る"
    );
    assert!(listed["mentions"].as_object().unwrap().is_empty());

    // 編集してもその中の素の `@username` は名前で解かれず、通知は飛ばない
    fx.edit(&id, &format!("@{name} 旧い形の本文（追記）")).await;
    assert_eq!(
        fx.mentioned(&alice).await,
        0,
        "編集でも素の @name は通知しない"
    );
}

#[tokio::test]
async fn list_returns_current_names_after_a_rename_and_skips_deleted_comments() {
    let fx = setup().await;
    let alice = fx.member().await;
    let bob = fx.member().await;

    fx.post(&format!("{} 確認", mark(&alice))).await;
    let to_delete = fx.post(&format!("{} 削除する", mark(&bob))).await;
    let res = fx
        .app
        .delete_with_session(&format!(
            "{}/{}",
            fx.comments,
            to_delete["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    fx.rename(&alice, "alice_renamed").await;
    let listed = fx.list().await;

    assert_eq!(
        listed["mentions"][alice.id.to_string()]["name"],
        "alice_renamed",
        "改名しても切れず、いまの名が返る"
    );
    assert!(
        listed["mentions"].get(bob.id.to_string()).is_none(),
        "削除済みコメントの本文に出る印は引かない"
    );
    // 本文は印のまま（名は本文に埋めない）。削除済みのコメントの本文は返らぬ
    let bodies: Vec<&str> = listed["comments"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["body"].as_str())
        .collect();
    assert_eq!(bodies.len(), 1, "削除済みのコメントは本文が null");
    assert!(bodies[0].contains(&mark(&alice)));
}

#[tokio::test]
async fn webhook_body_has_names_instead_of_markers() {
    let fx = setup().await;
    let alice = fx.member().await;
    let outsider = fx.outsider().await;
    let ghost = Uuid::new_v4();

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

    fx.post(&format!(
        "{} {} <@{ghost}> 確認",
        mark(&alice),
        mark(&outsider)
    ))
    .await;

    let deliveries = webhook_deliveries::Entity::find()
        .filter(webhook_deliveries::Column::WebhookId.eq(hook_id))
        .order_by_asc(webhook_deliveries::Column::CreatedAt)
        .all(&fx.app.state.db)
        .await
        .expect("deliveries");
    assert_eq!(deliveries.len(), 1);
    let payload = deliveries[0].payload.to_string();
    let alice_name = fx.username(&alice).await;
    assert!(!payload.contains("<@"), "webhook に印が残る: {payload}");
    assert!(
        !payload.contains(&alice.id.to_string()) && !payload.contains(&ghost.to_string()),
        "webhook に利用者 id が漏れる: {payload}"
    );
    let body = deliveries[0].payload["comment"]["body"].as_str().unwrap();
    assert!(body.contains(&format!("@{alice_name}")), "{body}");
    assert!(body.contains("@（このプロジェクトの外の利用者）"), "{body}");
    assert!(body.contains("@（退会した利用者）"), "{body}");
}
