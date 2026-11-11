use crate::common::TestApp;
use axum::http::StatusCode;
use entity::tasks;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, prelude::Uuid};
use serde_json::{Value, json};

// タスク説明文の長さ上限（`payload::tasks::MAX_DESCRIPTION_LEN`）の統合テスト。
// 超過は作成・更新とも 400。上限ちょうどと日本語（バイトではなく文字で数える）は通る。
// 本文を送らない更新は、上限前に保存された長い本文が残っていても通る。
// 値は仕様（docs/dev/features/tasks/1.core.md）の 100,000 文字を直に書いて固定する。
const MAX: usize = 100_000;

struct Fx {
    app: TestApp,
    tasks: String,
    status_id: String,
}

async fn setup() -> Fx {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let tasks = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let status = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/statuses",
                tp.tenant_id, tp.project_id
            ),
            json!({"name":"Backlog","color":"#336699","position":0,"is_default":true}),
        )
        .await;
    assert_eq!(status.status(), StatusCode::CREATED);
    let status_id = status.json::<Value>().await.expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();
    Fx {
        app,
        tasks,
        status_id,
    }
}

impl Fx {
    async fn create(&self, description: &str) -> reqwest::Response {
        self.app
            .post_json_with_session(
                &self.tasks,
                json!({"title":"説明文の上限","status_id":self.status_id,"description":description}),
            )
            .await
    }

    async fn update(&self, id: &str, body: Value) -> StatusCode {
        self.app
            .put_json_with_session(&format!("{}/{id}", self.tasks), body)
            .await
            .status()
    }
}

#[tokio::test]
async fn create_rejects_description_over_limit_counting_chars() {
    let fx = setup().await;

    let over = fx.create(&"a".repeat(MAX + 1)).await;
    assert_eq!(over.status(), StatusCode::BAD_REQUEST, "100,001 文字");

    let at_limit = fx.create(&"a".repeat(MAX)).await;
    assert_eq!(
        at_limit.status(),
        StatusCode::CREATED,
        "100,000 文字ちょうど"
    );

    // 300,000 バイトだが 100,000 文字なので通る
    let multibyte = fx.create(&"あ".repeat(MAX)).await;
    assert_eq!(
        multibyte.status(),
        StatusCode::CREATED,
        "日本語 100,000 文字"
    );
}

#[tokio::test]
async fn update_rejects_description_over_limit_but_omitted_description_passes() {
    let fx = setup().await;
    let created = fx.create("短い本文").await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let id = created.json::<Value>().await.expect("json")["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(
        fx.update(&id, json!({"description": "a".repeat(MAX + 1)}))
            .await,
        StatusCode::BAD_REQUEST,
        "100,001 文字"
    );
    assert_eq!(
        fx.update(&id, json!({"description": "a".repeat(MAX)}))
            .await,
        StatusCode::OK,
        "100,000 文字ちょうど"
    );

    // 上限が入る前に保存された長い本文（移行しない）を直に置く。
    let model = tasks::Entity::find_by_id(id.parse::<Uuid>().unwrap())
        .one(&fx.app.state.db)
        .await
        .expect("query")
        .expect("task");
    let mut active: tasks::ActiveModel = model.into();
    active.description = Set(Some("a".repeat(MAX * 2)));
    active.update(&fx.app.state.db).await.expect("update");

    // 本文を送らなければ検証にかからず、他の項目は直せる
    assert_eq!(
        fx.update(&id, json!({"title": "改題"})).await,
        StatusCode::OK,
        "description を省いた更新"
    );
}
