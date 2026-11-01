//! アバター画像のアップロード（POST /v1/auth/me/avatar）と配信（GET /v1/avatars/{key}）。

use crate::common::{TestApp, TestUser};
use axum::http::{StatusCode, header};
use entity::scopes::Scope;
use entity::users;
use reqwest::multipart::{Form, Part};
use sea_orm::EntityTrait;

const PNG_HEADER: &[u8] = b"\x89PNG\r\n\x1a\n";
const MAX_BYTES: usize = 2 * 1024 * 1024;

/// 先頭だけ PNG の署名にした指定サイズのバイト列（中身の妥当性は検査しない）。
fn png_of_size(len: usize) -> Vec<u8> {
    let mut bytes = PNG_HEADER.to_vec();
    bytes.resize(len, 0);
    bytes
}

fn avatar_form(bytes: Vec<u8>) -> Form {
    Form::new().part(
        "file",
        Part::bytes(bytes)
            .file_name("avatar.png")
            .mime_str("image/png")
            .expect("mime"),
    )
}

async fn upload(app: &TestApp, bytes: Vec<u8>) -> reqwest::Response {
    app.session_client()
        .post(format!("{}/v1/auth/me/avatar", app.base_url()))
        .multipart(avatar_form(bytes))
        .send()
        .await
        .expect("upload request")
}

async fn login(app: &mut TestApp) -> TestUser {
    let user = app.insert_user(false, false).await;
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
    user
}

async fn stored_key(app: &TestApp, user: &TestUser) -> Option<String> {
    users::Entity::find_by_id(user.id)
        .one(&app.state.db)
        .await
        .expect("find user")
        .expect("user row")
        .avatar_storage_key
}

/// 認証なし（Cookie を持たないクライアント）で配信 URL を叩く。
async fn get_public(app: &TestApp, key: &str) -> reqwest::Response {
    reqwest::get(format!("{}/v1/avatars/{key}", app.base_url()))
        .await
        .expect("avatar get")
}

#[tokio::test]
async fn upload_sets_self_hosted_url_and_serves_without_auth() {
    let mut app = TestApp::new().await;
    let user = login(&mut app).await;

    let res = upload(&app, png_of_size(64)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = res.json().await.expect("body");

    let key = stored_key(&app, &user).await.expect("avatar_storage_key");
    let url = body["avatar_url"].as_str().expect("avatar_url");
    assert_eq!(url, format!("{}/v1/avatars/{key}", app.base_url()));
    assert!(key.starts_with("avatar-") && key.ends_with(".png"), "{key}");

    let served = get_public(&app, &key).await;
    assert_eq!(served.status(), StatusCode::OK);
    assert_eq!(served.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(
        served.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    assert_eq!(served.headers()[header::CONTENT_DISPOSITION], "inline");
    assert_eq!(
        served.bytes().await.expect("bytes").to_vec(),
        png_of_size(64)
    );

    // update_me と同じ監査ログを残す
    let me = app
        .get_me()
        .await
        .json::<serde_json::Value>()
        .await
        .expect("me");
    assert_eq!(me["avatar_url"], url);

    app.cleanup_user(user.id).await;
}

/// 2 MiB ちょうどは通り、1 バイトでも超えると 413。本文上限の層を越える大きさも 413。
#[tokio::test]
async fn enforces_two_mib_limit() {
    let mut app = TestApp::new().await;
    let user = login(&mut app).await;

    let exact = upload(&app, png_of_size(MAX_BYTES)).await;
    assert_eq!(exact.status(), StatusCode::OK, "2 MiB ちょうどは受け付ける");
    let key_after_exact = stored_key(&app, &user).await;

    let over = upload(&app, png_of_size(MAX_BYTES + 1)).await;
    assert_eq!(over.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let far_over = upload(&app, png_of_size(3 * 1024 * 1024)).await;
    assert_eq!(far_over.status(), StatusCode::PAYLOAD_TOO_LARGE);

    assert_eq!(
        stored_key(&app, &user).await,
        key_after_exact,
        "拒否されたアップロードは保存先を変えない"
    );

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn rejects_non_image_bytes() {
    let mut app = TestApp::new().await;
    let user = login(&mut app).await;

    // content-type を image/png と偽っても先頭バイトで判定する
    let res = upload(
        &app,
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>".to_vec(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let empty = upload(&app, Vec::new()).await;
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
    assert_eq!(stored_key(&app, &user).await, None);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn reupload_removes_previous_object() {
    let mut app = TestApp::new().await;
    let user = login(&mut app).await;

    assert_eq!(upload(&app, png_of_size(32)).await.status(), StatusCode::OK);
    let first = stored_key(&app, &user).await.expect("first key");
    assert_eq!(upload(&app, png_of_size(48)).await.status(), StatusCode::OK);
    let second = stored_key(&app, &user).await.expect("second key");

    assert_ne!(first, second);
    assert_eq!(
        get_public(&app, &first).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(get_public(&app, &second).await.status(), StatusCode::OK);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn clearing_or_replacing_with_url_removes_uploaded_object() {
    let mut app = TestApp::new().await;
    let user = login(&mut app).await;

    upload(&app, png_of_size(32)).await;
    let key = stored_key(&app, &user).await.expect("key");
    let res = app
        .patch_json_with_session(
            "/v1/auth/me",
            serde_json::json!({ "clear_avatar_url": true }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(stored_key(&app, &user).await, None);
    assert_eq!(get_public(&app, &key).await.status(), StatusCode::NOT_FOUND);

    upload(&app, png_of_size(32)).await;
    let key = stored_key(&app, &user).await.expect("key");
    let res = app
        .patch_json_with_session(
            "/v1/auth/me",
            serde_json::json!({ "avatar_url": "https://example.com/a.png" }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(stored_key(&app, &user).await, None);
    assert_eq!(get_public(&app, &key).await.status(), StatusCode::NOT_FOUND);

    // アバター以外の更新では消さない（対照）
    upload(&app, png_of_size(32)).await;
    let key = stored_key(&app, &user).await.expect("key");
    let res = app
        .patch_json_with_session("/v1/auth/me", serde_json::json!({ "bio": "変更" }))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(stored_key(&app, &user).await.as_deref(), Some(key.as_str()));
    assert_eq!(get_public(&app, &key).await.status(), StatusCode::OK);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn admin_delete_user_removes_uploaded_object() {
    let mut app = TestApp::new().await;
    let user = login(&mut app).await;
    upload(&app, png_of_size(32)).await;
    let key = stored_key(&app, &user).await.expect("key");

    let admin = app.insert_user(true, false).await;
    app.reset_session_client();
    app.login_session_no_content(&admin.email, &admin.password)
        .await;
    let res = app
        .delete_with_session(&format!("/v1/admin/users/{}", user.id))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert_eq!(get_public(&app, &key).await.status(), StatusCode::NOT_FOUND);

    app.cleanup_user(admin.id).await;
}

/// update_me と同じく PAT では使えない（認証方式の不一致なので 403）。
#[tokio::test]
async fn rejects_personal_access_token() {
    let app = TestApp::new().await;
    let user = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(user.id).await;
    let pat = app
        .insert_pat(user.id, tp.tenant_id, vec![Scope::Api], None)
        .await;

    let res = reqwest::Client::new()
        .post(format!("{}/v1/auth/me/avatar", app.base_url()))
        .bearer_auth(&pat)
        .multipart(avatar_form(png_of_size(32)))
        .send()
        .await
        .expect("upload with bearer");
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(stored_key(&app, &user).await, None);

    app.cleanup_user(user.id).await;
}

#[tokio::test]
async fn malformed_key_is_not_found() {
    let app = TestApp::new().await;
    for key in [
        "avatar-0f8fad5b-d9cb-469f-a165-70867728950e.svg",
        "avatar-0F8FAD5B-D9CB-469F-A165-70867728950E.png",
        "0f8fad5b-d9cb-469f-a165-70867728950e",
        "..%2Fsecret.png",
    ] {
        assert_eq!(
            get_public(&app, key).await.status(),
            StatusCode::NOT_FOUND,
            "{key}"
        );
    }
    // 形式は正しいが存在しないキー
    assert_eq!(
        get_public(&app, "avatar-0f8fad5b-d9cb-469f-a165-70867728950e.png")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}
