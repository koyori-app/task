//! `GET /tasks/similar`（類似タスク候補、pg_trgm）。
//! apps/backend/docs/task-similarity-search.md §6②③ / §8②③。

use crate::common::{TestApp, TestTenantProject, TestUser, create_status, json_body};
use axum::http::StatusCode;
use entity::projects;
use entity::scopes::Scope;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::Value;
use uuid::Uuid;

struct Fixture {
    app: TestApp,
    owner: TestUser,
    tp: TestTenantProject,
    status_id: Uuid,
}

async fn setup() -> Fixture {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let status_id = create_status(&app, &tp, "Todo", false).await;
    Fixture {
        app,
        owner,
        tp,
        status_id,
    }
}

async fn create_task(
    app: &TestApp,
    tp: &TestTenantProject,
    status_id: Uuid,
    title: &str,
) -> String {
    let res = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/tasks",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({ "title": title, "status_id": status_id }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "create task {title}");
    json_body(res).await["id"]
        .as_str()
        .expect("task id")
        .to_string()
}

fn similar_path(tp: &TestTenantProject, query: &str) -> String {
    format!(
        "/v1/tenants/{}/projects/{}/tasks/similar?{query}",
        tp.tenant_id, tp.project_id
    )
}

async fn similar(app: &TestApp, tp: &TestTenantProject, query: &str) -> Value {
    let res = app.get_with_session(&similar_path(tp, query)).await;
    assert_eq!(res.status(), StatusCode::OK, "similar {query}");
    json_body(res).await
}

fn hit_ids(body: &Value) -> Vec<String> {
    body["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .map(|t| t["id"].as_str().expect("id").to_string())
        .collect()
}

#[tokio::test]
async fn returns_similar_titles_in_score_order_within_project() {
    let Fixture {
        app, tp, status_id, ..
    } = setup().await;
    let exact = create_task(&app, &tp, status_id, "ログイン画面の修正").await;
    let close = create_task(&app, &tp, status_id, "ログイン画面の修正をする").await;
    let partial = create_task(&app, &tp, status_id, "ログイン画面").await;
    create_task(&app, &tp, status_id, "設定の保存").await;

    let deleted = create_task(&app, &tp, status_id, "ログイン画面の修正").await;
    let res = app
        .delete_with_session(&format!(
            "/v1/tenants/{}/projects/{}/tasks/{deleted}",
            tp.tenant_id, tp.project_id
        ))
        .await;
    assert!(res.status().is_success(), "delete task: {}", res.status());

    // 同じテナントの別プロジェクトにある同名タスクは候補に出さない
    let other_project = Uuid::new_v4();
    projects::ActiveModel {
        id: Set(other_project),
        name: Set("similar-other".into()),
        description: Set(String::new()),
        tenant_id: Set(tp.tenant_id),
        icon_emoji: Set(None),
        icon_url: Set(None),
        key: Set(format!(
            "S{}",
            other_project.to_string()[..8].to_uppercase()
        )),
        is_personal: Set(false),
        personal_owner_id: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert second project");
    let other_tp = TestTenantProject {
        tenant_id: tp.tenant_id,
        project_id: other_project,
    };
    let other_status = create_status(&app, &other_tp, "Todo", false).await;
    let other_task = create_task(&app, &other_tp, other_status, "ログイン画面の修正").await;

    let body = similar(&app, &tp, "title=ログイン画面の修正").await;
    assert_eq!(
        hit_ids(&body),
        vec![exact.clone(), close.clone(), partial.clone()]
    );
    let scores: Vec<f64> = body["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["score"].as_f64().expect("score"))
        .collect();
    assert_eq!(scores[0], 1.0, "完全一致は 1.0");
    assert!(
        scores.windows(2).all(|w| w[0] > w[1]),
        "score 降順: {scores:?}"
    );
    let ids = hit_ids(&body);
    assert!(!ids.contains(&deleted), "削除済みは除く");
    assert!(!ids.contains(&other_task), "別プロジェクトは除く");

    let body = similar(
        &app,
        &tp,
        &format!("title=ログイン画面の修正&exclude_task_id={exact}"),
    )
    .await;
    assert_eq!(hit_ids(&body), vec![close, partial]);
}

#[tokio::test]
async fn authorization_matches_search_tasks() {
    let Fixture {
        mut app,
        owner,
        tp,
        status_id,
    } = setup().await;
    create_task(&app, &tp, status_id, "ログイン画面の修正").await;
    let query = "title=ログイン画面の修正";
    let path = similar_path(&tp, query);

    // 別テナントの project_id を組み合わせた URL は、両テナントのオーナーでも通らない
    let other_tp = app.insert_tenant_project(owner.id).await;
    let mixed = format!(
        "/v1/tenants/{}/projects/{}/tasks/similar?{query}",
        tp.tenant_id, other_tp.project_id
    );
    assert_eq!(
        app.get_with_session(&mixed).await.status(),
        StatusCode::NOT_FOUND
    );

    // ReadTask を持たない PAT は 403、持つ PAT は 200
    let no_read_task = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadProject], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&path, &no_read_task).await.status(),
        StatusCode::FORBIDDEN
    );
    let read_task = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&path, &read_task).await.status(),
        StatusCode::OK
    );

    let member = app.insert_user(false, false).await;
    let outsider = app.insert_user(false, false).await;
    let added = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/members", tp.tenant_id),
            serde_json::json!({ "user_id": member.id, "role": "Member" }),
        )
        .await;
    assert_eq!(added.status(), StatusCode::CREATED);

    // 非メンバーは 403
    app.reset_session_client();
    app.login_session_no_content(&outsider.email, &outsider.password)
        .await;
    assert_eq!(
        app.get_with_session(&path).await.status(),
        StatusCode::FORBIDDEN
    );

    // テナントメンバーは 200（過剰拒否でないこと）
    app.reset_session_client();
    app.login_session_no_content(&member.email, &member.password)
        .await;
    let res = app.get_with_session(&path).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(json_body(res).await["tasks"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn title_is_validated_by_char_count() {
    let Fixture { app, tp, .. } = setup().await;

    let res = app
        .get_with_session(&similar_path(&tp, "title=%20%20%20"))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST, "空白のみ");

    // 日本語はバイト数ではなく文字数で数える（255 文字 = 765 バイト）
    let ok = "あ".repeat(255);
    let res = app
        .get_with_session(&similar_path(&tp, &format!("title={ok}")))
        .await;
    assert_eq!(res.status(), StatusCode::OK, "255 文字");

    let too_long = "あ".repeat(256);
    let res = app
        .get_with_session(&similar_path(&tp, &format!("title={too_long}")))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST, "256 文字");
}

#[tokio::test]
async fn limit_is_capped_at_20_with_stable_order() {
    let Fixture {
        app, tp, status_id, ..
    } = setup().await;
    let mut ids = Vec::new();
    for _ in 0..22 {
        ids.push(create_task(&app, &tp, status_id, "同じタイトルのタスク").await);
    }
    // score が全件同点なので id 昇順で並ぶ（Postgres の uuid 比較は小文字 16 進の文字列順と一致）
    ids.sort();

    let body = similar(&app, &tp, "title=同じタイトルのタスク").await;
    assert_eq!(hit_ids(&body), ids[..5].to_vec(), "既定は 5 件");

    let body = similar(&app, &tp, "title=同じタイトルのタスク&limit=21").await;
    assert_eq!(hit_ids(&body), ids[..20].to_vec(), "21 は 20 に丸まる");
    let again = similar(&app, &tp, "title=同じタイトルのタスク&limit=21").await;
    assert_eq!(hit_ids(&again), hit_ids(&body), "同点でも順序が安定する");
}
