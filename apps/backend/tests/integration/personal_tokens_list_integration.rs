// テストケースの一覧（docs/dev/features/account-settings.md から移した）
// - 一覧が自分の有効なトークンだけを名前順で返すこと、平文・ハッシュを含まないこと、取り消しで一覧から消えること、空一覧、未ログイン拒否、Bearer 拒否

use crate::common::{TestApp, insert_tenant};
use axum::http::StatusCode;
use entity::scopes::Scope;

/// 作成 API でトークンを 1 件発行し、その ID を返す。
async fn create_token(app: &TestApp, tenant_id: uuid::Uuid, name: &str) -> uuid::Uuid {
    let res = app
        .post_json_with_session(
            "/v1/personal_tokens",
            serde_json::json!({
                "name": name,
                "tenant_ids": [tenant_id],
                "scopes": ["read:task"],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED, "トークン発行は成功する");
    let body = res.json::<serde_json::Value>().await.expect("create body");
    body["id"].as_str().expect("id").parse().expect("uuid")
}

async fn list_tokens(app: &TestApp) -> Vec<serde_json::Value> {
    let res = app.get_with_session("/v1/personal_tokens").await;
    assert_eq!(res.status(), StatusCode::OK);
    res.json::<Vec<serde_json::Value>>()
        .await
        .expect("list body")
}

/// 自分の有効なトークンだけが名前順で返り、他ユーザーの分は混ざらない。
/// 平文トークン・ハッシュは一覧に含まれない。
#[tokio::test]
async fn lists_only_own_active_tokens() {
    let mut app = TestApp::new().await;

    let user = app.insert_user_default().await;
    let other = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let other_tenant_id = insert_tenant(&app.state.db, other.id).await;

    // 他ユーザーのトークン（見えてはいけない）。
    app.insert_pat(other.id, other_tenant_id, vec![Scope::Api], None)
        .await;

    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;

    // 1 件も持たないうちは空配列が返る（404 等にしない）。
    assert!(list_tokens(&app).await.is_empty(), "まだ 1 件も無い");

    // 名前順を確認するため、あえて逆順で作る。
    create_token(&app, tenant_id, "b-second").await;
    create_token(&app, tenant_id, "a-first").await;

    let tokens = list_tokens(&app).await;
    assert_eq!(tokens.len(), 2, "自分のトークンだけが返る");
    assert_eq!(tokens[0]["name"], "a-first", "名前の昇順で返る");
    assert_eq!(tokens[1]["name"], "b-second");

    for token in &tokens {
        assert_eq!(token["user_id"], user.id.to_string());
        assert_eq!(
            token["tenant_ids"],
            serde_json::json!([tenant_id.to_string()])
        );
        assert_eq!(token["revoked"], false);
        assert_eq!(token["scopes"], serde_json::json!(["read:task"]));
        assert_eq!(
            token["token_last_four"].as_str().expect("last four").len(),
            4
        );
        // 平文トークンとハッシュは作成応答以外で返さない。
        assert!(token.get("token").is_none(), "平文トークンを含まない");
        assert!(token.get("token_hash").is_none(), "ハッシュを含まない");
    }

    app.cleanup_user(user.id).await;
    app.cleanup_user(other.id).await;
}

/// 取り消したトークンは一覧から消える（取り消し前は載っている対照付き）。
#[tokio::test]
async fn revoked_tokens_disappear_from_list() {
    let mut app = TestApp::new().await;

    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;

    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;

    let keep_id = create_token(&app, tenant_id, "keep").await;
    let revoke_id = create_token(&app, tenant_id, "revoke-me").await;

    let before = list_tokens(&app).await;
    assert_eq!(before.len(), 2, "取り消し前は両方載っている");

    let res = app
        .delete_with_session(&format!("/v1/personal_tokens/{revoke_id}"))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let after = list_tokens(&app).await;
    assert_eq!(after.len(), 1, "取り消した分だけ消える");
    assert_eq!(after[0]["id"], keep_id.to_string());

    app.cleanup_user(user.id).await;
}

/// MCP の OAuth の許可は `personal_tokens` の行だが、PAT の一覧・単体取得・取り消しには出ない
/// （「連携中のアプリ」の `/v1/oauth_grants` で扱う）。対照: PAT は従来どおり出る。
#[tokio::test]
async fn oauth_grants_are_not_personal_tokens() {
    let mut app = TestApp::new().await;

    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let grant_id = uuid::Uuid::new_v4();
    backend::utils::oauth_server::grant::create_grant(
        &app.state.db,
        &app.state.settings.personal_token_secret,
        &backend::utils::oauth_server::grant::PendingAuthorization {
            grant_id,
            user_id: user.id,
            client_id: crate::common::TEST_OAUTH_CLIENT.to_string(),
            client_name: "Test Client".to_string(),
            redirect_uri: "https://client.test/callback".to_string(),
            code_challenge: String::new(),
            scopes: vec![Scope::ReadTask],
            tenant_ids: vec![tenant_id],
            allow_refresh: true,
            issued_at_ms: chrono::Utc::now().timestamp_millis(),
        },
    )
    .await
    .expect("create grant");

    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let pat_id = create_token(&app, tenant_id, "a-pat").await;

    let listed = list_tokens(&app).await;
    assert_eq!(listed.len(), 1, "許可は PAT の一覧に出ない");
    assert_eq!(listed[0]["id"], pat_id.to_string());

    let res = app
        .get_with_session(&format!("/v1/personal_tokens/{grant_id}"))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let res = app
        .delete_with_session(&format!("/v1/personal_tokens/{grant_id}"))
        .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    // 404 の取り消しで許可が落ちていない
    let res = app.get_with_session("/v1/oauth_grants").await;
    let grants = res.json::<Vec<serde_json::Value>>().await.expect("grants");
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0]["id"], grant_id.to_string());

    // 対照: PAT は単体取得も取り消しもできる
    let res = app
        .get_with_session(&format!("/v1/personal_tokens/{pat_id}"))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = app
        .delete_with_session(&format!("/v1/personal_tokens/{pat_id}"))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    app.cleanup_user(user.id).await;
}

/// 未ログインは拒否される。
#[tokio::test]
async fn rejects_unauthenticated() {
    let mut app = TestApp::new().await;
    app.reset_session_client();

    let res = app.get_with_session("/v1/personal_tokens").await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

/// PAT（Bearer）ではトークン管理 API を使えない（セッション専用）。
#[tokio::test]
async fn rejects_bearer_token_auth() {
    let app = TestApp::new().await;

    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let token = app
        .insert_pat(user.id, tenant_id, vec![Scope::Api], None)
        .await;

    let res = app.get_with_bearer("/v1/personal_tokens", &token).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    app.cleanup_user(user.id).await;
}

/// scopes を空配列で送ると 400 になり、トークンは発行されない（#633）。
#[tokio::test]
async fn create_returns_400_for_empty_scopes() {
    let mut app = TestApp::new().await;

    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;

    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;

    let res = app
        .post_json_with_session(
            "/v1/personal_tokens",
            serde_json::json!({
                "name": "no-scope",
                "tenant_ids": [tenant_id],
                "scopes": [],
            }),
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::BAD_REQUEST,
        "scopes が空のリクエストは 400 で拒む"
    );

    let tokens = list_tokens(&app).await;
    assert!(
        tokens.is_empty(),
        "拒まれたリクエストでトークンは発行されない"
    );

    app.cleanup_user(user.id).await;
}

/// scopes が一件あれば従来どおり 201 で発行できる（空拒否の対照）。
#[tokio::test]
async fn create_returns_201_for_single_scope() {
    let mut app = TestApp::new().await;

    let user = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;

    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;

    // create_token は 201 を検証してから ID を返す。
    let id = create_token(&app, tenant_id, "one-scope").await;

    let tokens = list_tokens(&app).await;
    assert_eq!(tokens.len(), 1, "一件だけ発行されている");
    assert_eq!(tokens[0]["id"], id.to_string());
    assert_eq!(tokens[0]["scopes"], serde_json::json!(["read:task"]));

    app.cleanup_user(user.id).await;
}
