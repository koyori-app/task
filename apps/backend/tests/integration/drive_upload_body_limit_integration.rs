// テストケースの一覧（docs/dev/features/drive/index.md から移した）
// - アップロード上限ちょうど・超過の判定
//
// Drive の行を作るテストは、backfill のテスト（`drive_project_id_backfill_integration`）と
// 同じ鍵で直列化する（理由は `drive_upload_acl_integration` の冒頭を参照）。
//
// axum の `Multipart` / `Json` 抽出器には `DefaultBodyLimit`（既定 2 MB）が別に掛かる。
// `RequestBodyLimitLayer`（`UPLOAD_MAX_SIZE_MB`）だけでは外れず、2 MB を超える添付が 500 になっていた。

use crate::common::TestApp;
use axum::{
    body::{Body, Bytes},
    http::{Request, StatusCode, header},
};
use entity::scopes::Scope;
use reqwest::multipart::{Form, Part};

/// axum の既定上限（2 MB）を越え、`UPLOAD_MAX_SIZE_MB` の既定（100 MB）には収まるサイズ。
const OVER_AXUM_DEFAULT: usize = 3 * 1024 * 1024;

/// 2 MB を超えるファイルもアップロードでき、本文の更新も通る。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn upload_and_update_content_over_axum_default_body_limit() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let form = Form::new().part(
        "file",
        Part::bytes(vec![b'a'; OVER_AXUM_DEFAULT])
            .file_name("large.txt")
            .mime_str("text/plain")
            .expect("mime"),
    );
    let response = app
        .client()
        .post(format!(
            "{}/v1/tenants/{}/drive/files",
            app.base_url(),
            tp.tenant_id
        ))
        .multipart(form)
        .send()
        .await
        .expect("upload request");
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "2 MB を超えるファイルもアップロードできる"
    );
    let uploaded: serde_json::Value = response.json().await.expect("upload json");
    assert_eq!(uploaded["size"].as_u64(), Some(OVER_AXUM_DEFAULT as u64));

    // 本文更新（JSON）も同じ 2 MB 上限に当たっていた
    let file_id = uploaded["id"].as_str().expect("id");
    let updated = app
        .put_json_with_session(
            &format!("/v1/tenants/{}/drive/files/{file_id}/content", tp.tenant_id),
            serde_json::json!({ "content": "b".repeat(OVER_AXUM_DEFAULT) }),
        )
        .await;
    assert_eq!(
        updated.status(),
        StatusCode::OK,
        "2 MB を超える本文でも更新できる"
    );

    app.cleanup_user(owner.id).await;
}

/// `UPLOAD_MAX_SIZE_MB` を超える本文は、読み取り途中で打ち切られても 500 ではなく 413。
///
/// Content-Length があると `RequestBodyLimitLayer` がハンドラーより前に 413 を返すので、
/// 長さを申告しないストリームで送り、multipart の読み取りエラーの分類を確かめる。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn upload_over_upload_max_size_is_413() {
    let app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let token = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::WriteDrive], None)
        .await;

    const BOUNDARY: &str = "drive-body-limit-boundary";
    const CHUNK: usize = 64 * 1024;
    let chunks = (app.state.drive_config.upload_max_bytes as usize).div_ceil(CHUNK) + 1;
    let head = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"big.bin\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    );
    // 遅延して流すので、サーバーが上限で読むのをやめた後の分は作られない
    let parts = std::iter::once(Bytes::from(head))
        .chain(std::iter::repeat_n(Bytes::from(vec![0u8; CHUNK]), chunks))
        .chain(std::iter::once(Bytes::from(format!(
            "\r\n--{BOUNDARY}--\r\n"
        ))));
    let body = Body::from_stream(futures::stream::iter(
        parts.map(Ok::<_, std::convert::Infallible>),
    ));

    let request = Request::builder()
        .method("POST")
        .uri(format!("/v1/tenants/{}/drive/files", tp.tenant_id))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(body)
        .expect("request");
    let response = app.request(request).await;
    assert_eq!(
        response.status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "上限超過は 413: {}",
        response.body
    );

    app.cleanup_user(owner.id).await;
}

/// 長さを申告した multipart 本文（ブラウザの FormData と同じ）を、遅延ストリームで組み立てる。
/// 戻り値は (本文, Content-Length)。
fn multipart_of_size(file_len: usize) -> (Body, usize) {
    const BOUNDARY: &str = "drive-exact-size-boundary";
    const CHUNK: usize = 64 * 1024;
    let head = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"exact.bin\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    );
    let tail = format!("\r\n--{BOUNDARY}--\r\n");
    let content_length = head.len() + file_len + tail.len();
    let chunks = (0..file_len)
        .step_by(CHUNK)
        .map(move |offset| Bytes::from(vec![0u8; CHUNK.min(file_len - offset)]));
    let parts = std::iter::once(Bytes::from(head))
        .chain(chunks)
        .chain(std::iter::once(Bytes::from(tail)));
    let body = Body::from_stream(futures::stream::iter(
        parts.map(Ok::<_, std::convert::Infallible>),
    ));
    (body, content_length)
}

fn upload_request(tenant_id: uuid::Uuid, token: &str, file_len: usize) -> Request<Body> {
    let (body, content_length) = multipart_of_size(file_len);
    Request::builder()
        .method("POST")
        .uri(format!("/v1/tenants/{tenant_id}/drive/files"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(
            header::CONTENT_TYPE,
            "multipart/form-data; boundary=drive-exact-size-boundary",
        )
        .header(header::CONTENT_LENGTH, content_length)
        .body(body)
        .expect("request")
}

/// `UPLOAD_MAX_SIZE_MB` ちょうどのファイルは受理し、1 バイトでも超えれば 413。
///
/// 本文の上限が設定値ちょうどだと、multipart の境界・ヘッダーの分だけ
/// Content-Length が上限を超え、ちょうどのファイルまで 413 になっていた。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn upload_exactly_upload_max_size_is_accepted_and_one_byte_over_is_413() {
    let app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let token = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::WriteDrive], None)
        .await;
    let max = app.state.drive_config.upload_max_bytes as usize;

    let created = app.request(upload_request(tp.tenant_id, &token, max)).await;
    assert_eq!(
        created.status,
        StatusCode::CREATED,
        "上限ちょうどのファイルは受理する: {}",
        created.body
    );
    let uploaded: serde_json::Value = serde_json::from_str(&created.body).expect("upload json");
    assert_eq!(uploaded["size"].as_u64(), Some(max as u64));

    let over = app
        .request(upload_request(tp.tenant_id, &token, max + 1))
        .await;
    assert_eq!(
        over.status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "上限を 1 バイト超えたら 413: {}",
        over.body
    );

    // 100 MB 級の実ファイルをアップロード先に残さない
    let file_id = uploaded["id"].as_str().expect("id");
    let deleted = app
        .request(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/v1/tenants/{}/drive/files/{file_id}",
                    tp.tenant_id
                ))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{}", deleted.body);

    app.cleanup_user(owner.id).await;
}
