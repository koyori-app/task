// テストケースの一覧
// - 未認証は 401（認証なしで任意ホストへの GET の踏み台にさせない）
// - 認証済みでも host の形式が不正（ドット無し・空ラベル・254 文字）は 404
// - 認証済み・loopback / private の IP リテラル（名前解決せず拒否）は 200 で汎用アイコン
//   （`image/svg+xml`、`Cache-Control: private, max-age=3600`、本文は `FALLBACK_SVG`）
// - 同じ host の 2 回目も同じ応答で、負のキャッシュ（Redis `favicon:v1:{host}`）が入っている
// - PAT の Bearer でも 200（拒否しすぎていない対照）
// - キャッシュに無い host の取得は利用者ごとに FETCH_RATE_LIMIT 回まで。超えた分は外へ取りに行かず
//   `no-store` の汎用アイコンで、負のキャッシュも作らない。キャッシュ済みの host は制限されず、
//   別の利用者の枠にも影響しない（拒否と対照の成功をセットで確かめる）
// 実画像の成功経路は https への接続を要する（取得は https 固定）ので統合では扱わず、
// `service::favicon` の `fetch_icon_at` の単体テストで覆う。
//
//! リンク先サイトの favicon の配信（GET /v1/favicons/{host}）。

use crate::common::TestApp;
use axum::http::{StatusCode, header};
use entity::scopes::Scope;
use service::favicon::{
    CACHE_KEY_PREFIX, FALLBACK_MIME, FALLBACK_SVG, FETCH_RATE_LIMIT, RATE_LIMIT_KEY_PREFIX,
};

async fn logged_in_app() -> TestApp {
    let mut app = TestApp::new().await;
    let user = app.insert_user(false, false).await;
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    app
}

async fn delete_key(app: &TestApp, host: &str) {
    let mut conn = app.state.redis_client.conn.acquire().await.unwrap();
    let _: i64 = redis::cmd("DEL")
        .arg(format!("{CACHE_KEY_PREFIX}{host}"))
        .query_async(&mut conn)
        .await
        .unwrap();
}

async fn key_exists(app: &TestApp, host: &str) -> bool {
    let mut conn = app.state.redis_client.conn.acquire().await.unwrap();
    let count: i64 = redis::cmd("EXISTS")
        .arg(format!("{CACHE_KEY_PREFIX}{host}"))
        .query_async(&mut conn)
        .await
        .unwrap();
    count == 1
}

async fn assert_fallback(res: reqwest::Response) {
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], FALLBACK_MIME);
    assert_eq!(
        res.headers()[header::CACHE_CONTROL],
        "private, max-age=3600"
    );
    assert_eq!(res.headers()[header::CONTENT_DISPOSITION], "inline");
    assert_eq!(res.text().await.unwrap(), FALLBACK_SVG);
}

#[tokio::test]
async fn unauthenticated_request_is_rejected() {
    let app = TestApp::new().await;
    let res = app.get("/v1/favicons/example.com").await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invalid_host_is_not_found() {
    let app = logged_in_app().await;
    let long254 = format!(
        "{}.{}.{}.{}",
        "a".repeat(63),
        "a".repeat(63),
        "a".repeat(63),
        "a".repeat(62)
    );
    for host in ["localhost", "a..b", long254.as_str()] {
        let res = app.get(&format!("/v1/favicons/{host}")).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{host}");
    }
}

#[tokio::test]
async fn private_and_loopback_literals_get_the_fallback_icon_and_are_negative_cached() {
    let app = logged_in_app().await;
    for host in ["10.0.0.1", "127.0.0.1"] {
        // 負のキャッシュは 1 時間残り、ハーネスは Redis を消さない。永続の Redis でも再実行できるよう先に消す
        delete_key(&app, host).await;
        assert!(
            !key_exists(&app, host).await,
            "{host}: 事前にキャッシュが無い"
        );
        assert_fallback(app.get(&format!("/v1/favicons/{host}")).await).await;
        assert!(key_exists(&app, host).await, "{host}: 負のキャッシュが入る");
        // 2 回目はキャッシュから同じ応答
        assert_fallback(app.get(&format!("/v1/favicons/{host}")).await).await;
    }
}

#[tokio::test]
async fn personal_access_token_is_accepted() {
    let app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let token = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    delete_key(&app, "10.0.0.2").await;
    let res = app.get_with_bearer("/v1/favicons/10.0.0.2", &token).await;
    assert_fallback(res).await;
}

/// 利用者ごとに固有の host（10.x.y.z の IP リテラル）。名前解決せず拒否されるので外へ出ない。
fn hosts_for(user_seed: u8, count: usize) -> Vec<String> {
    (0..count)
        .map(|n| format!("10.{user_seed}.{}.{}", n / 250, n % 250 + 1))
        .collect()
}

#[tokio::test]
async fn fetches_for_uncached_hosts_are_rate_limited_per_user() {
    let mut app = TestApp::new().await;
    let limit = FETCH_RATE_LIMIT as usize;
    // ハーネスは Redis を消さないので、使う host の負のキャッシュと利用者ごとの枠を先に空にする
    let hosts = hosts_for(77, limit + 1);
    for host in &hosts {
        delete_key(&app, host).await;
    }

    let user = app.insert_user(false, false).await;
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    {
        let mut conn = app.state.redis_client.conn.acquire().await.unwrap();
        let _: i64 = redis::cmd("DEL")
            .arg(format!("{RATE_LIMIT_KEY_PREFIX}{}", user.id))
            .query_async(&mut conn)
            .await
            .unwrap();
    }

    // 上限までは通常どおり取りに行き、負のキャッシュが入る
    for host in &hosts[..limit] {
        assert_fallback(app.get(&format!("/v1/favicons/{host}")).await).await;
        assert!(key_exists(&app, host).await, "{host}");
    }
    // 上限を超えた host は取りに行かず、no-store で、負のキャッシュも作らない
    let over = &hosts[limit];
    let res = app.get(&format!("/v1/favicons/{over}")).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], FALLBACK_MIME);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(res.text().await.unwrap(), FALLBACK_SVG);
    assert!(!key_exists(&app, over).await, "超過分は保存しない");
    // キャッシュ済みの host は上限に達していても通常どおり返る（過剰拒否でない対照）
    assert_fallback(app.get(&format!("/v1/favicons/{}", hosts[0])).await).await;

    // 別の利用者の枠は別（同じ host でも、その利用者はまだ取りに行ける）
    let other = app.insert_user(false, false).await;
    app.reset_session_client();
    app.login_session_no_content(&other.email, &other.password)
        .await;
    let res = app.get(&format!("/v1/favicons/{over}")).await;
    assert_eq!(
        res.headers()[header::CACHE_CONTROL],
        "private, max-age=3600"
    );
}
