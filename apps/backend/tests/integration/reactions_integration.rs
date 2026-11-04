use crate::common::{TestApp, TestTenantProject, TestUser, ensure_tenant_member_for_project};
use axum::http::StatusCode;
use entity::notifications;
use entity::scopes::Scope;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::{Value, json};

/// `👍` のパーセントエンコード。frontend が送る形そのもの。
const THUMBS_UP: &str = "%F0%9F%91%8D";

struct Fixture {
    tp: TestTenantProject,
    owner: TestUser,
    task_id: String,
    comment_id: String,
}

impl Fixture {
    fn task_path(&self) -> String {
        format!(
            "/v1/tenants/{}/projects/{}/tasks/{}",
            self.tp.tenant_id, self.tp.project_id, self.task_id
        )
    }

    fn comment_path(&self) -> String {
        format!("{}/comments/{}", self.task_path(), self.comment_id)
    }
}

/// 所有者でログインした状態でタスクとコメントを 1 件ずつ作る。
async fn setup(app: &mut TestApp) -> Fixture {
    let owner = app.insert_user(false, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;

    let project_path = format!("/v1/tenants/{}/projects/{}", tp.tenant_id, tp.project_id);
    let status = app
        .post_json_with_session(
            &format!("{project_path}/statuses"),
            json!({ "name": "Backlog", "color": "#336699", "position": 0, "is_default": true }),
        )
        .await;
    assert_eq!(status.status(), StatusCode::CREATED);
    let status: Value = status.json().await.unwrap();

    let task = app
        .post_json_with_session(
            &format!("{project_path}/tasks"),
            json!({ "title": "reactions", "status_id": status["id"] }),
        )
        .await;
    assert_eq!(task.status(), StatusCode::CREATED);
    let task: Value = task.json().await.unwrap();
    let task_id = task["id"].as_str().unwrap().to_owned();

    let mut fixture = Fixture {
        tp,
        owner,
        task_id,
        comment_id: String::new(),
    };
    fixture.comment_id = post_comment(app, &fixture, None).await;
    fixture
}

async fn post_comment(app: &TestApp, fixture: &Fixture, parent: Option<&str>) -> String {
    let comment = app
        .post_json_with_session(
            &format!("{}/comments", fixture.task_path()),
            json!({ "body": "本文", "parent_comment_id": parent }),
        )
        .await;
    assert_eq!(comment.status(), StatusCode::CREATED);
    let comment: Value = comment.json().await.unwrap();
    comment["id"].as_str().unwrap().to_owned()
}

async fn login_as_new_member(app: &mut TestApp, fixture: &Fixture) -> TestUser {
    let member = app.insert_user(false, false).await;
    ensure_tenant_member_for_project(&app.state.db, fixture.tp.project_id, member.id).await;
    login_as(app, &member).await;
    member
}

async fn login_as(app: &mut TestApp, user: &TestUser) {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
}

async fn put_reaction(app: &TestApp, target_path: &str, emoji: &str) -> reqwest::Response {
    app.put_json_with_session(&format!("{target_path}/reactions/{emoji}"), json!({}))
        .await
}

async fn put_ok(app: &TestApp, target_path: &str, emoji: &str) -> Value {
    let res = put_reaction(app, target_path, emoji).await;
    assert_eq!(res.status(), StatusCode::OK);
    res.json().await.unwrap()
}

async fn delete_ok(app: &TestApp, target_path: &str, emoji: &str) -> Value {
    let res = app
        .delete_with_session(&format!("{target_path}/reactions/{emoji}"))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    res.json().await.unwrap()
}

async fn get_users(app: &TestApp, target_path: &str, emoji: &str) -> Value {
    let res = app
        .get_with_session(&format!("{target_path}/reactions/{emoji}/users"))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    res.json().await.unwrap()
}

async fn get_json(app: &TestApp, path: &str) -> Value {
    let res = app.get_with_session(path).await;
    assert_eq!(res.status(), StatusCode::OK);
    res.json().await.unwrap()
}

fn user_ids(users: &Value) -> Vec<String> {
    users
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn reaction_put_is_idempotent_and_delete_only_removes_own() {
    let mut app = TestApp::new().await;
    let fixture = setup(&mut app).await;

    for target in [fixture.task_path(), fixture.comment_path()] {
        login_as(&mut app, &fixture.owner).await;
        let summary = put_ok(&app, &target, THUMBS_UP).await;
        assert_eq!(summary["emoji"], "👍");
        assert_eq!(summary["count"], 1);
        assert_eq!(summary["reacted"], true);
        assert_eq!(user_ids(&summary["users"]), [fixture.owner.id.to_string()]);
        assert!(summary["users"][0]["name"].is_string());
        assert!(summary["users"][0]["avatar_url"].is_null());

        // 2 回押しても 1 件のまま
        let summary = put_ok(&app, &target, THUMBS_UP).await;
        assert_eq!(summary["count"], 1);

        // 別の人が押すと増え、本人が先頭に来る
        let member = login_as_new_member(&mut app, &fixture).await;
        let summary = put_ok(&app, &target, THUMBS_UP).await;
        assert_eq!(summary["count"], 2);
        assert_eq!(
            user_ids(&summary["users"]),
            [member.id.to_string(), fixture.owner.id.to_string()]
        );

        // 押していない絵文字を外しても何も起きない
        let summary = delete_ok(&app, &target, "%F0%9F%8E%89").await;
        assert_eq!(summary["emoji"], "🎉");
        assert_eq!(summary["count"], 0);
        assert_eq!(summary["reacted"], false);
        assert_eq!(summary["users"], json!([]));

        // 外せるのは自分の分だけ。所有者の分は残る
        let summary = delete_ok(&app, &target, THUMBS_UP).await;
        assert_eq!(summary["count"], 1);
        assert_eq!(summary["reacted"], false);
        assert_eq!(user_ids(&summary["users"]), [fixture.owner.id.to_string()]);

        // 既に外した分をもう一度外しても 200 のまま、所有者の分も減らない
        let summary = delete_ok(&app, &target, THUMBS_UP).await;
        assert_eq!(summary["count"], 1);

        login_as(&mut app, &fixture.owner).await;
        let users = get_users(&app, &target, THUMBS_UP).await;
        assert_eq!(users["count"], 1);

        // 所有者が自分の分を外すと 0 件
        let summary = delete_ok(&app, &target, THUMBS_UP).await;
        assert_eq!(summary["count"], 0);
    }
}

#[tokio::test]
async fn reaction_targets_do_not_mix() {
    let mut app = TestApp::new().await;
    let fixture = setup(&mut app).await;

    put_ok(&app, &fixture.comment_path(), THUMBS_UP).await;

    let task = get_json(&app, &fixture.task_path()).await;
    assert_eq!(task["reactions"], json!([]));
    let users = get_users(&app, &fixture.task_path(), THUMBS_UP).await;
    assert_eq!(users["count"], 0);

    // 別のタスクの id の下からは、コメントに届かない
    let other = setup(&mut app).await;
    let foreign = format!("{}/comments/{}", other.task_path(), fixture.comment_id);
    let res = put_reaction(&app, &foreign, THUMBS_UP).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reaction_21st_kind_is_rejected_but_existing_kind_is_not() {
    let mut app = TestApp::new().await;
    let fixture = setup(&mut app).await;

    for target in [fixture.task_path(), fixture.comment_path()] {
        login_as(&mut app, &fixture.owner).await;
        for i in 0..20 {
            put_ok(&app, &target, &format!("e{i}")).await;
        }

        let res = put_reaction(&app, &target, "e20").await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["message"], "reaction-limit-exceeded");
        assert_eq!(get_users(&app, &target, "e20").await["count"], 0);

        // 既にある絵文字は、20 種類に達していても押せる（本人の付け直しも別の人も）
        assert_eq!(put_ok(&app, &target, "e0").await["count"], 1);
        login_as_new_member(&mut app, &fixture).await;
        assert_eq!(put_ok(&app, &target, "e0").await["count"], 2);

        // 1 種類空けば、また新しい絵文字を足せる
        login_as(&mut app, &fixture.owner).await;
        delete_ok(&app, &target, "e19").await;
        assert_eq!(put_ok(&app, &target, "e20").await["count"], 1);
    }
}

#[tokio::test]
async fn reaction_limit_holds_under_concurrent_puts() {
    let mut app = TestApp::new().await;
    let fixture = setup(&mut app).await;
    let target = fixture.task_path();

    for i in 0..19 {
        put_ok(&app, &target, &format!("e{i}")).await;
    }

    // 残り 1 枠を 8 本で取り合っても、通るのは 1 本だけ
    let (a, b, c, d, e, f, g, h) = tokio::join!(
        put_reaction(&app, &target, "x0"),
        put_reaction(&app, &target, "x1"),
        put_reaction(&app, &target, "x2"),
        put_reaction(&app, &target, "x3"),
        put_reaction(&app, &target, "x4"),
        put_reaction(&app, &target, "x5"),
        put_reaction(&app, &target, "x6"),
        put_reaction(&app, &target, "x7"),
    );
    let statuses = [a, b, c, d, e, f, g, h].map(|res| res.status());
    let ok = statuses.iter().filter(|s| **s == StatusCode::OK).count();
    let rejected = statuses
        .iter()
        .filter(|s| **s == StatusCode::BAD_REQUEST)
        .count();
    assert_eq!((ok, rejected), (1, 7), "{statuses:?}");

    let task = get_json(&app, &target).await;
    assert_eq!(task["reactions"].as_array().unwrap().len(), 20);
}

#[tokio::test]
async fn reaction_on_deleted_comment_is_rejected_and_hidden() {
    let mut app = TestApp::new().await;
    let fixture = setup(&mut app).await;
    let target = fixture.comment_path();

    // 対照: 削除前は付けられる
    assert_eq!(put_ok(&app, &target, THUMBS_UP).await["count"], 1);
    let deleted = app.delete_with_session(&target).await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let res = put_reaction(&app, &target, "%F0%9F%8E%89").await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    // 既に押していた絵文字も同じ
    let res = put_reaction(&app, &target, THUMBS_UP).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 付いていた分は一覧にも個別の取得にも出ない
    let list = get_json(&app, &format!("{}/comments", fixture.task_path())).await;
    assert_eq!(list["comments"][0]["is_deleted"], true);
    assert_eq!(list["comments"][0]["reactions"], json!([]));
    let users = get_users(&app, &target, THUMBS_UP).await;
    assert_eq!(users["count"], 0);
    assert_eq!(users["users"], json!([]));

    // 外す操作は通る
    assert_eq!(delete_ok(&app, &target, THUMBS_UP).await["count"], 0);
}

#[tokio::test]
async fn reaction_requires_tenant_access_and_write_scope() {
    let mut app = TestApp::new().await;
    let fixture = setup(&mut app).await;
    let targets = [fixture.task_path(), fixture.comment_path()];

    for target in &targets {
        put_ok(&app, target, THUMBS_UP).await;
    }

    // 別テナントの利用者: 付ける・外す・見る、どれも通らない
    let outsider = app.insert_user(false, false).await;
    let outsider_tp = app.insert_tenant_project(outsider.id).await;
    login_as(&mut app, &outsider).await;
    for target in &targets {
        let denied = [
            put_reaction(&app, target, "%F0%9F%8E%89").await.status(),
            app.delete_with_session(&format!("{target}/reactions/{THUMBS_UP}"))
                .await
                .status(),
            app.get_with_session(&format!("{target}/reactions/{THUMBS_UP}/users"))
                .await
                .status(),
        ];
        for status in denied {
            assert!(
                status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
                "{target}: {status}"
            );
        }
    }

    // 別テナントに束ねられた鍵も通らない
    let foreign_pat = app
        .insert_pat(
            outsider.id,
            outsider_tp.tenant_id,
            vec![Scope::WriteTask, Scope::ReadTask],
            None,
        )
        .await;

    let read_pat = app
        .insert_pat(
            fixture.owner.id,
            fixture.tp.tenant_id,
            vec![Scope::ReadTask],
            None,
        )
        .await;
    let write_pat = app
        .insert_pat(
            fixture.owner.id,
            fixture.tp.tenant_id,
            vec![Scope::WriteTask],
            None,
        )
        .await;

    let client = app.client().clone();
    let base = app.base_url().to_owned();
    for target in &targets {
        let reaction_url = format!("{base}{target}/reactions/{THUMBS_UP}");
        let users_url = format!("{reaction_url}/users");

        let status = client
            .put(&reaction_url)
            .bearer_auth(&foreign_pat)
            .send()
            .await
            .unwrap()
            .status();
        assert!(
            status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND,
            "{target}: {status}"
        );

        // read:task だけの鍵は書けない。読みは通る
        for res in [
            client
                .put(&reaction_url)
                .bearer_auth(&read_pat)
                .send()
                .await,
            client
                .delete(&reaction_url)
                .bearer_auth(&read_pat)
                .send()
                .await,
        ] {
            assert_eq!(res.unwrap().status(), StatusCode::FORBIDDEN);
        }
        let res = client
            .get(&users_url)
            .bearer_auth(&read_pat)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let users: Value = res.json().await.unwrap();
        assert_eq!(users["count"], 1);

        // write:task の鍵は付ける・外すが通る（過剰拒否でない）。read:task を含意するので読みも通る
        let res = client
            .put(&reaction_url)
            .bearer_auth(&write_pat)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let res = client
            .delete(&reaction_url)
            .bearer_auth(&write_pat)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let res = client
            .get(&users_url)
            .bearer_auth(&write_pat)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn reaction_emoji_value_is_validated() {
    let mut app = TestApp::new().await;
    let fixture = setup(&mut app).await;

    for target in [fixture.task_path(), fixture.comment_path()] {
        for bad in ["%20", "a%20b", "a%09b", "%07", "%E3%80%80", &"a".repeat(65)] {
            let res = put_reaction(&app, &target, bad).await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "PUT {bad}");
            let res = app
                .delete_with_session(&format!("{target}/reactions/{bad}"))
                .await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "DELETE {bad}");
            let res = app
                .get_with_session(&format!("{target}/reactions/{bad}/users"))
                .await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "GET {bad}");
        }

        // 境界ちょうどの 64 バイトと、結合子入りの絵文字は通る
        let longest = "a".repeat(64);
        assert_eq!(put_ok(&app, &target, &longest).await["emoji"], longest);
        // 👨‍👩‍👧
        let family = "%F0%9F%91%A8%E2%80%8D%F0%9F%91%A9%E2%80%8D%F0%9F%91%A7";
        assert_eq!(
            put_ok(&app, &target, family).await["emoji"],
            "👨\u{200d}👩\u{200d}👧"
        );
    }
}

#[tokio::test]
async fn reactions_are_embedded_in_comment_list_and_task_detail() {
    let mut app = TestApp::new().await;
    let fixture = setup(&mut app).await;
    let reply_id = post_comment(&app, &fixture, Some(&fixture.comment_id)).await;
    let reply_path = format!("{}/comments/{reply_id}", fixture.task_path());
    let targets = [
        fixture.task_path(),
        fixture.comment_path(),
        reply_path.clone(),
    ];

    // 境界（5 人）を越える 6 人が押す。🎉 を先に付け、👍 の後から 🎉 が増えても順は動かない
    let mut reactors = Vec::new();
    for i in 0..6 {
        let member = login_as_new_member(&mut app, &fixture).await;
        for target in &targets {
            if i == 0 {
                put_ok(&app, target, "%F0%9F%8E%89").await;
            }
            put_ok(&app, target, THUMBS_UP).await;
        }
        if i == 5 {
            for target in &targets {
                put_ok(&app, target, "%F0%9F%8E%89").await;
            }
        }
        reactors.push(member);
    }
    let reactor_ids: Vec<String> = reactors.iter().map(|u| u.id.to_string()).collect();

    let check = |reactions: &Value, viewer_reacted: bool| {
        let list = reactions.as_array().expect("reactions array");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["emoji"], "🎉");
        assert_eq!(list[0]["count"], 2);
        assert_eq!(list[1]["emoji"], "👍");
        assert_eq!(list[1]["count"], 6);
        assert_eq!(list[1]["reacted"], viewer_reacted);
        let shown = user_ids(&list[1]["users"]);
        assert_eq!(shown.len(), 5);
        if viewer_reacted {
            // 本人（最後に押した人）は切られる位置でも先頭に出て、残りは古い順
            assert_eq!(shown[0], reactor_ids[5]);
            assert_eq!(shown[1..], reactor_ids[..4]);
        } else {
            assert_eq!(shown, reactor_ids[..5]);
        }
    };

    // 押した本人から見る
    let task = get_json(&app, &fixture.task_path()).await;
    check(&task["reactions"], true);
    let list = get_json(&app, &format!("{}/comments", fixture.task_path())).await;
    check(&list["comments"][0]["reactions"], true);
    check(&list["comments"][0]["replies"][0]["reactions"], true);

    // 押していない人から見る
    login_as(&mut app, &fixture.owner).await;
    let task = get_json(&app, &fixture.task_path()).await;
    check(&task["reactions"], false);
    let list = get_json(&app, &format!("{}/comments", fixture.task_path())).await;
    check(&list["comments"][0]["reactions"], false);
    check(&list["comments"][0]["replies"][0]["reactions"], false);

    // /users は切らずに全員を押した順で返す
    for target in &targets {
        let users = get_users(&app, target, THUMBS_UP).await;
        assert_eq!(users["emoji"], "👍");
        assert_eq!(users["count"], 6);
        assert_eq!(user_ids(&users["users"]), reactor_ids);
    }

    // 付いていないコメントとタスクは空配列
    let empty_comment = post_comment(&app, &fixture, None).await;
    let list = get_json(&app, &format!("{}/comments", fixture.task_path())).await;
    let comment = list["comments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == empty_comment.as_str())
        .unwrap();
    assert_eq!(comment["reactions"], json!([]));

    // 作成と更新の応答（同じ TaskDetailResponse）にも載る
    let updated = app
        .put_json_with_session(&fixture.task_path(), json!({ "title": "renamed" }))
        .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated: Value = updated.json().await.unwrap();
    check(&updated["reactions"], false);
}

async fn notification_count(app: &TestApp, fixture: &Fixture) -> u64 {
    notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(fixture.owner.id))
        .count(&app.state.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn reaction_records_no_activity_and_no_notification() {
    let mut app = TestApp::new().await;
    let fixture = setup(&mut app).await;
    let activities_path = format!("{}/activities", fixture.task_path());
    let before = get_json(&app, &activities_path).await["total"].clone();
    let notified_before = notification_count(&app, &fixture).await;

    // 所有者のタスク・コメントに別の人が押す（通知が出るならこの形で出る）
    login_as_new_member(&mut app, &fixture).await;
    for target in [fixture.task_path(), fixture.comment_path()] {
        put_ok(&app, &target, THUMBS_UP).await;
        delete_ok(&app, &target, THUMBS_UP).await;
    }

    assert_eq!(get_json(&app, &activities_path).await["total"], before);
    assert_eq!(notification_count(&app, &fixture).await, notified_before);
}
