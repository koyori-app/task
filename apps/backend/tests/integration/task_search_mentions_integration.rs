use crate::common::{TestApp, TestUser};
use axum::http::StatusCode;
use entity::users;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, prelude::Uuid};
use serde_json::Value;

// タスク検索の `@名` の統合テスト。説明文のメンションは `<@利用者ID>` の印で保存され、名は
// 本文に入らぬ。検索語の `@名` は検索の時に名を利用者へ引き、その者の印を含む説明文にも当てる。
// 改名の後は今の名で当たること・名の境（説明文のメンションの名を返す境と同じ）の外の同名の者では
// 当たらぬこと・同名の者が二人いれば両方に当てること・印より前の素の `@名` の本文は今どおり
// 文字で当たることを固定する。

struct Fx {
    app: TestApp,
    project_id: Uuid,
    tenant_id: Uuid,
    status_id: String,
}

async fn setup() -> Fx {
    let mut app = TestApp::new().await;
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
    }
}

fn mark(user: &TestUser) -> String {
    format!("<@{}>", user.id)
}

/// 試験ごとに他と重ならぬ名（英小文字だけ。検索の語の切れ目に掛からぬように）。
fn fresh_name(prefix: &str) -> String {
    let tail: String = Uuid::new_v4().as_bytes()[..8]
        .iter()
        .map(|b| char::from(b'a' + b % 26))
        .collect();
    format!("{prefix}{tail}")
}

impl Fx {
    fn tasks(&self) -> String {
        format!(
            "/v1/tenants/{}/projects/{}/tasks",
            self.tenant_id, self.project_id
        )
    }

    /// テナントに入っている（プロジェクトにも入れる）利用者を、名を付けて作る。
    async fn member_named(&self, name: &str) -> TestUser {
        let user = self.app.insert_user(false, false).await;
        crate::common::ensure_tenant_member_for_project(
            &self.app.state.db,
            self.project_id,
            user.id,
        )
        .await;
        self.rename(&user, name).await;
        user
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

    async fn create(&self, title: &str, description: &str) -> String {
        let body = serde_json::json!({
            "title": title,
            "status_id": self.status_id,
            "description": description,
        });
        let res = self.app.post_json_with_session(&self.tasks(), body).await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let created: Value = res.json().await.expect("json");
        created["id"].as_str().expect("id").to_string()
    }

    /// 検索で当たったタスクの id（並びは問わぬので整えて返す）と、応答の全体。
    async fn search(&self, q: &str) -> (Vec<String>, Value) {
        let res = self
            .app
            .get_with_session(&format!(
                "{}/search?q={}",
                self.tasks(),
                urlencoding::encode(q)
            ))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body: Value = res.json().await.expect("json");
        let mut ids: Vec<String> = body["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .map(|hit| hit["id"].as_str().expect("id").to_string())
            .collect();
        ids.sort();
        assert_eq!(body["total"].as_u64(), Some(ids.len() as u64), "{body}");
        (ids, body)
    }
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

#[tokio::test]
async fn at_name_matches_description_mentioning_that_user() {
    let fx = setup().await;
    let name = fresh_name("mina");
    let mina = fx.member_named(&name).await;
    let hit = fx
        .create("レビューの依頼", &format!("{} に 見て もらう", mark(&mina)))
        .await;
    fx.create("別件", "誰にも 頼まぬ").await;

    let (ids, body) = fx.search(&format!("@{name}")).await;
    assert_eq!(ids, vec![hit.clone()], "{body}");
    // 抜粋は印を `@今の名` に戻した本文で、その名が強調される
    let highlight = body["tasks"][0]["highlight"].as_str().unwrap();
    assert!(!highlight.contains("<@"), "{highlight}");
    assert!(
        highlight.contains(&format!("<em>@{name}</em>")),
        "{highlight}"
    );

    // 名の大小は問わぬ
    let (ids, _) = fx.search(&format!("@{}", name.to_uppercase())).await;
    assert_eq!(ids, vec![hit.clone()]);

    // 他の語と並べれば、両方を満たす物だけが当たる
    let (ids, _) = fx.search(&format!("@{name} 見て")).await;
    assert_eq!(ids, vec![hit.clone()]);
    let (ids, _) = fx.search(&format!("@{name} 頼まぬ")).await;
    assert!(ids.is_empty(), "{ids:?}");

    // 名の一部では、印に当てぬ
    let (ids, _) = fx.search(&format!("@{}", &name[..5])).await;
    assert!(ids.is_empty(), "{ids:?}");

    // `@` だけの語は、名に引かず、文字で当てる。文字の当ては本文の生の字に及ぶので、
    // 印の `<@` を含む説明文にだけ当たる（main の部分一致の当て方。誰の印かは引かれていない）
    let (ids, _) = fx.search("@").await;
    assert_eq!(ids, vec![hit], "{ids:?}");
}

#[tokio::test]
async fn renamed_user_matches_by_new_name_only() {
    let fx = setup().await;
    let before = fresh_name("haru");
    let after = fresh_name("natsu");
    let haru = fx.member_named(&before).await;
    let hit = fx
        .create("改名の前に書いた", &format!("{} へ確認", mark(&haru)))
        .await;

    fx.rename(&haru, &after).await;

    let (ids, _) = fx.search(&format!("@{after}")).await;
    assert_eq!(ids, vec![hit], "今の名で当たる");
    let (ids, _) = fx.search(&format!("@{before}")).await;
    assert!(ids.is_empty(), "前の名では当たらぬ: {ids:?}");
}

#[tokio::test]
async fn same_name_user_of_another_tenant_never_matches() {
    let fx = setup().await;
    let name = fresh_name("sora");
    let sora = fx.member_named(&name).await;
    // 別のテナントに属し、そこで活動している同名の利用者
    let other = fx.app.insert_user(false, false).await;
    let other_tp = fx.app.insert_tenant_project(other.id).await;
    assert_ne!(other_tp.tenant_id, fx.tenant_id);
    fx.rename(&other, &name).await;

    let ours = fx
        .create("この組の者", &format!("{} に頼む", mark(&sora)))
        .await;
    fx.create("外の者", &format!("{} に頼む", mark(&other)))
        .await;

    let (ids, body) = fx.search(&format!("@{name}")).await;
    assert_eq!(ids, vec![ours], "名の境の外の同名の者には当てぬ");
    // 外の者の id は、応答のどこにも出ない
    assert!(!body.to_string().contains(&other.id.to_string()), "{body}");
}

#[tokio::test]
async fn two_members_with_the_same_name_both_match() {
    let fx = setup().await;
    let name = fresh_name("ren");
    let first = fx.member_named(&name).await;
    let second = fx.member_named(&name).await;
    let a = fx.create("一人目", &format!("{} へ", mark(&first))).await;
    let b = fx.create("二人目", &format!("{} へ", mark(&second))).await;

    // 画面ではどちらも `@名` と出る。検索も見える名で引くので、両方に当てる
    let (ids, _) = fx.search(&format!("@{name}")).await;
    assert_eq!(ids, sorted(vec![a, b]));
}

#[tokio::test]
async fn plain_at_name_text_still_matches() {
    let fx = setup().await;
    // 印より前に書かれた素の `@名`。名の者がおってもおらずとも、文字として当たる
    let name = fresh_name("aoi");
    fx.member_named(&name).await;
    let old = fx
        .create("古い本文", &format!("@{name} さんに頼んだ"))
        .await;
    let nobody = fresh_name("kaze");
    let text_only = fx
        .create("誰でもない名", &format!("@{nobody} という名の者はおらぬ"))
        .await;

    let (ids, _) = fx.search(&format!("@{name}")).await;
    assert_eq!(ids, vec![old]);
    let (ids, _) = fx.search(&format!("@{nobody}")).await;
    assert_eq!(ids, vec![text_only]);
}
