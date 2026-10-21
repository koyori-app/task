//! `GET /tasks/search` の日本語部分一致・表記ゆれ（pg_trgm）。
//! apps/backend/docs/task-similarity-search.md §6① / §8①。

use crate::common::{TestApp, TestTenantProject, create_status};
use axum::http::StatusCode;
use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;

struct Fixture {
    app: TestApp,
    tp: TestTenantProject,
    status_id: Uuid,
}

async fn setup() -> Fixture {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tp = app.insert_tenant_project(user.id).await;
    let status_id = create_status(&app, &tp, "Todo", false).await;
    Fixture { app, tp, status_id }
}

async fn create_task(fx: &Fixture, title: &str, description: Option<&str>) -> String {
    let mut body = serde_json::json!({ "title": title, "status_id": fx.status_id });
    if let Some(description) = description {
        body["description"] = description.into();
    }
    let res = fx
        .app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/tasks",
                fx.tp.tenant_id, fx.tp.project_id
            ),
            body,
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "create task {title}");
    let task: Value = res.json().await.expect("task json");
    task["id"].as_str().expect("task id").to_string()
}

/// `query` は URL のクエリ文字列としてそのまま付ける（空白は呼び出し側で `%20` にする）
async fn search(fx: &Fixture, query: &str) -> Value {
    let res = fx
        .app
        .get_with_session(&format!(
            "/v1/tenants/{}/projects/{}/tasks/search?{query}",
            fx.tp.tenant_id, fx.tp.project_id
        ))
        .await;
    assert_eq!(res.status(), StatusCode::OK, "search {query}");
    res.json().await.expect("search json")
}

fn hit_titles(body: &Value) -> Vec<String> {
    body["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .map(|t| t["title"].as_str().expect("title").to_string())
        .collect()
}

fn hit_ids(body: &Value) -> Vec<String> {
    body["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .map(|t| t["id"].as_str().expect("id").to_string())
        .collect()
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

#[tokio::test]
async fn japanese_substring_matches_head_middle_and_tail() {
    let fx = setup().await;
    create_task(&fx, "ログイン画面の修正", None).await;
    create_task(&fx, "認証ログイン画面", None).await;
    create_task(&fx, "管理者ログイン", None).await;
    create_task(&fx, "設定の保存", None).await;

    // simple 設定の tsvector はこれらを 1 語にするため、修正前は 0 件だった
    let body = search(&fx, "q=ログイン").await;
    assert_eq!(body["total"], 3);
    assert_eq!(
        sorted(hit_titles(&body)),
        sorted(vec![
            "ログイン画面の修正".into(),
            "認証ログイン画面".into(),
            "管理者ログイン".into(),
        ])
    );

    // 2 文字は trigram が作れないが ILIKE で拾う
    let body = search(&fx, "q=画面").await;
    assert_eq!(body["total"], 2);
    assert_eq!(
        sorted(hit_titles(&body)),
        sorted(vec!["ログイン画面の修正".into(), "認証ログイン画面".into()])
    );
}

#[tokio::test]
async fn description_only_matches_still_hit() {
    let fx = setup().await;
    create_task(&fx, "表示の不具合", Some("ログイン画面が崩れる")).await;
    create_task(&fx, "設定", Some("fix oauth callback")).await;
    create_task(&fx, "無関係", Some("なにもない")).await;

    let body = search(&fx, "q=ログイン").await;
    assert_eq!(hit_titles(&body), vec!["表示の不具合".to_string()]);
    assert_eq!(body["total"], 1);

    // 従来の tsvector 経路（description だけにある語）
    let body = search(&fx, "q=oauth").await;
    assert_eq!(hit_titles(&body), vec!["設定".to_string()]);
    assert_eq!(body["total"], 1);
}

#[tokio::test]
async fn typo_matches_by_word_similarity() {
    let fx = setup().await;
    create_task(&fx, "authentication flow", None).await;
    create_task(&fx, "billing page", None).await;

    let body = search(&fx, "q=authentcation").await;
    assert_eq!(hit_titles(&body), vec!["authentication flow".to_string()]);
    assert_eq!(body["total"], 1);
}

#[tokio::test]
async fn query_longer_than_text_does_not_panic() {
    let fx = setup().await;
    create_task(&fx, "a b", None).await;

    // 検索語は trim しかしないので内部の空白が残り、8 文字 > 対象 4 文字（"a b "）になる。
    // 修正前はハイライトの範囲外スライスで panic し、接続が切れていた
    let body = search(&fx, "q=a%20%20%20%20%20%20b").await;
    assert_eq!(hit_titles(&body), vec!["a b".to_string()]);
}

#[tokio::test]
async fn approximate_match_longer_than_title_returns_highlight_without_em() {
    let fx = setup().await;
    create_task(&fx, "authentication", None).await;

    let body = search(&fx, "q=authenticationxx").await;
    assert_eq!(hit_titles(&body), vec!["authentication".to_string()]);
    let highlight = body["tasks"][0]["highlight"].as_str().expect("highlight");
    assert!(!highlight.contains("<em>"), "highlight: {highlight}");
}

#[tokio::test]
async fn mixed_match_kinds_count_once_and_page_without_gaps() {
    let fx = setup().await;
    // tsvector のみ（description に語が順不同で出る）
    let tsvector_only = create_task(&fx, "unrelated alpha", Some("the script will deploy")).await;
    // ILIKE のみ（description の語の途中に連続して出る）
    let ilike_only = create_task(&fx, "unrelated beta", Some("xdeploy scripty")).await;
    // `<%` のみ（title の表記ゆれ）
    let trgm_only = create_task(&fx, "deploy scrip", None).await;
    // すべてに一致
    let all = create_task(&fx, "deploy script", Some("deploy script")).await;
    create_task(&fx, "unrelated gamma", None).await;

    let query = "q=deploy%20script";
    let body = search(&fx, query).await;
    assert_eq!(body["total"], 4);

    let mut seen = Vec::new();
    for offset in [0, 2, 4] {
        let page = search(&fx, &format!("{query}&limit=2&offset={offset}")).await;
        assert_eq!(page["total"], 4, "offset {offset}");
        seen.extend(hit_ids(&page));
    }
    assert_eq!(seen.len(), 4, "pages: {seen:?}");
    assert_eq!(
        seen.iter().cloned().collect::<HashSet<_>>(),
        HashSet::from([tsvector_only, ilike_only, trgm_only.clone(), all.clone()])
    );
    // タイトル一致（ILIKE / `<%`）の行が先に来る
    assert_eq!(
        seen[..2].iter().cloned().collect::<HashSet<_>>(),
        HashSet::from([trgm_only, all])
    );
}

#[tokio::test]
async fn title_match_ranks_above_repeated_description_match() {
    let fx = setup().await;
    create_task(
        &fx,
        "無関係なタスク",
        Some("ログイン ログイン ログイン ログイン"),
    )
    .await;
    create_task(&fx, "認証ログイン画面", None).await;

    let body = search(&fx, "q=ログイン").await;
    assert_eq!(
        hit_titles(&body),
        vec!["認証ログイン画面".to_string(), "無関係なタスク".to_string()]
    );
}
