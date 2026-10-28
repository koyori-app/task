// Drive の行を作るテストは、backfill のテスト（`drive_project_id_backfill_integration`）が
// 実行前に `TRUNCATE drive_files, drive_folders CASCADE` を流すのと同じ鍵で直列化する。
// backfill の SQL はテナントを跨いで全行を見るので、あちらは Drive を空にしてからでないと
// 他のファイルの残骸で落ちる。鍵を共有しないと、その TRUNCATE がこちらの実行中の行を
// 巻き添えにする（CASCADE は drive_folder_shares と task_attachments にも及ぶ）。

use crate::common::{TestApp, file_id_of, insert_extra_project, upload_drive_file as upload_file};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use entity::{
    drive_files, drive_folder_shares, drive_folders, project_members, scopes::Scope, tenants,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use uuid::Uuid;

async fn insert_project_folder(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    created_by: Uuid,
) -> Uuid {
    let folder_id = Uuid::new_v4();
    drive_folders::ActiveModel {
        id: Set(folder_id),
        name: Set("project-folder".into()),
        parent_id: Set(None),
        tenant_id: Set(tenant_id),
        project_id: Set(Some(project_id)),
        created_by: Set(created_by),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert project folder");
    folder_id
}

async fn insert_tenant_folder(app: &TestApp, tenant_id: Uuid, created_by: Uuid) -> Uuid {
    let folder_id = Uuid::new_v4();
    drive_folders::ActiveModel {
        id: Set(folder_id),
        name: Set("tenant-folder".into()),
        parent_id: Set(None),
        tenant_id: Set(tenant_id),
        project_id: Set(None),
        created_by: Set(created_by),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert tenant folder");
    folder_id
}

async fn insert_token_share(
    app: &TestApp,
    folder_id: Uuid,
    created_by: Uuid,
    token: &str,
    expires_at: Option<DateTime<Utc>>,
) {
    drive_folder_shares::ActiveModel {
        id: Set(Uuid::new_v4()),
        folder_id: Set(folder_id),
        shared_with_user_id: Set(None),
        share_token: Set(Some(token.into())),
        permission: Set(drive_folder_shares::SharePermission::Viewer),
        created_by: Set(created_by),
        expires_at: Set(expires_at.map(Into::into)),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert token share");
}

async fn add_member(app: &TestApp, project_id: Uuid, user_id: Uuid) {
    crate::common::ensure_tenant_member_for_project(&app.state.db, project_id, user_id).await;
    project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        user_id: Set(user_id),
        role: Set(project_members::ProjectRole::Member),
    }
    .insert(&app.state.db)
    .await
    .expect("insert project member");
}

async fn set_tenant_quota(app: &TestApp, tenant_id: Uuid, quota_bytes: i64) {
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&app.state.db)
        .await
        .expect("load tenant")
        .expect("tenant exists");
    let mut active: tenants::ActiveModel = tenant.into();
    active.drive_quota_bytes = Set(Some(quota_bytes));
    active
        .update(&app.state.db)
        .await
        .expect("update tenant quota");
}

fn content_path(tenant_id: Uuid, file_id: Uuid) -> String {
    format!("/v1/tenants/{tenant_id}/drive/files/{file_id}/content")
}

fn timestamp_of(file: &serde_json::Value, field: &str) -> DateTime<Utc> {
    file[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is string"))
        .parse()
        .unwrap_or_else(|_| panic!("{field} is rfc3339"))
}

/// テキストファイルの本文を差し替えられる。size と updated_at が更新され、
/// 配信エンドポイントからも新しい内容が読める。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn update_content_replaces_text_file_body() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let uploaded = upload_file(
        &app,
        tp.tenant_id,
        None,
        "notes.md",
        "text/markdown",
        b"before",
    )
    .await;
    let file_id = file_id_of(&uploaded);
    let created_at = timestamp_of(&uploaded, "created_at");
    assert_eq!(
        uploaded["size"].as_i64(),
        Some(6),
        "アップロード直後のサイズ"
    );

    let new_body = "# after\n\n書き換えた本文";
    let response = app
        .put_json_with_session(
            &content_path(tp.tenant_id, file_id),
            serde_json::json!({ "content": new_body }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK, "本文更新は成功するはず");

    let updated: serde_json::Value = response.json().await.expect("update response json");
    assert_eq!(
        updated["size"].as_i64(),
        Some(new_body.len() as i64),
        "size は新しい本文のバイト数になる"
    );
    assert_eq!(
        updated["mime_type"].as_str(),
        Some("text/markdown"),
        "mime_type は据え置き"
    );
    assert!(
        timestamp_of(&updated, "updated_at") > created_at,
        "updated_at が編集時刻に更新される"
    );

    // 配信エンドポイントから新しい内容が読めること（ストレージにも反映されている）。
    let content = app
        .get_with_session(&format!("/v1/drive/files/{file_id}/content"))
        .await;
    assert_eq!(content.status(), StatusCode::OK);
    assert_eq!(
        content.text().await.expect("content body"),
        new_body,
        "配信される内容が差し替わっている"
    );

    app.cleanup_user(owner.id).await;
}

/// 空文字列での更新は「中身を空にする編集」として許可する。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn update_content_allows_empty_body() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let uploaded = upload_file(&app, tp.tenant_id, None, "memo.txt", "text/plain", b"xyz").await;
    let file_id = file_id_of(&uploaded);

    let response = app
        .put_json_with_session(
            &content_path(tp.tenant_id, file_id),
            serde_json::json!({ "content": "" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK, "空文字列でも更新できる");
    let updated: serde_json::Value = response.json().await.expect("update response json");
    assert_eq!(updated["size"].as_i64(), Some(0), "size は 0 になる");

    app.cleanup_user(owner.id).await;
}

/// テキストとして扱えない MIME のファイルは編集できない（400）。
/// 対照として同じ経路でテキストファイルは編集できることも確認する。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn update_content_rejects_non_text_mime() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let binary = upload_file(
        &app,
        tp.tenant_id,
        None,
        "logo.png",
        "image/png",
        &[0x89, 0x50, 0x4e, 0x47],
    )
    .await;
    let binary_id = file_id_of(&binary);
    let rejected = app
        .put_json_with_session(
            &content_path(tp.tenant_id, binary_id),
            serde_json::json!({ "content": "not an image" }),
        )
        .await;
    assert_eq!(
        rejected.status(),
        StatusCode::BAD_REQUEST,
        "image/png はテキストとして編集できない"
    );

    // 対照: .ts はブラウザが MPEG-TS として申告してくることがあるが、
    // 拡張子の上書きが優先されるため TypeScript として保存され、編集できる
    // （過剰拒否でないこと）。
    let source = upload_file(
        &app,
        tp.tenant_id,
        None,
        "main.ts",
        "video/mp2t",
        b"export const a = 1\n",
    )
    .await;
    let source_id = file_id_of(&source);
    let accepted = app
        .put_json_with_session(
            &content_path(tp.tenant_id, source_id),
            serde_json::json!({ "content": "export const a = 2\n" }),
        )
        .await;
    assert_eq!(
        accepted.status(),
        StatusCode::OK,
        "テキスト系ファイルは編集できる"
    );

    app.cleanup_user(owner.id).await;
}

/// 他テナントのファイル ID を自テナントのパスで指定しても 404（テナント越えを許さない）。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn update_content_does_not_cross_tenants() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let other_tenant_id = crate::common::insert_tenant(&app.state.db, owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let uploaded = upload_file(&app, tp.tenant_id, None, "memo.txt", "text/plain", b"body").await;
    let file_id = file_id_of(&uploaded);

    let not_found = app
        .put_json_with_session(
            &content_path(other_tenant_id, file_id),
            serde_json::json!({ "content": "leak" }),
        )
        .await;
    assert_eq!(
        not_found.status(),
        StatusCode::NOT_FOUND,
        "別テナント配下としては見つからない"
    );

    // 対照: 正しいテナントのパスなら編集できる。
    let ok = app
        .put_json_with_session(
            &content_path(tp.tenant_id, file_id),
            serde_json::json!({ "content": "edited" }),
        )
        .await;
    assert_eq!(ok.status(), StatusCode::OK, "自テナント配下では編集できる");

    app.cleanup_user(owner.id).await;
}

/// プロジェクトフォルダ内のファイル編集は、そのプロジェクトのメンバーのみ許可（403 / 200）。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn update_content_enforces_project_membership() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await; // tenant T + project A
    let project_b = insert_extra_project(&app, tp.tenant_id).await;
    let folder_b = insert_project_folder(&app, tp.tenant_id, project_b, owner.id).await;

    // テナントオーナーとして project B のフォルダにテキストファイルを置く。
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let uploaded = upload_file(
        &app,
        tp.tenant_id,
        Some(folder_b),
        "shared.txt",
        "text/plain",
        b"original",
    )
    .await;
    let file_id = file_id_of(&uploaded);

    // project B にメンバーを 1 人指定して「絞り込み状態」にする。
    // #568 でメンバー未指定のプロジェクトはテナント全体に開放されるため、
    // プロジェクト単位の隔離を試すには先に指定が要る。
    let member = app.insert_user(false, false).await;
    add_member(&app, project_b, member.id).await;

    // 攻撃者: project A のメンバー（テナントアクセスは通る）だが B の非メンバー。
    let attacker = app.insert_user(false, false).await;
    add_member(&app, tp.project_id, attacker.id).await;
    app.reset_session_client();
    app.login_session_no_content(&attacker.email, &attacker.password)
        .await;
    let forbidden = app
        .put_json_with_session(
            &content_path(tp.tenant_id, file_id),
            serde_json::json!({ "content": "tampered" }),
        )
        .await;
    assert_eq!(
        forbidden.status(),
        StatusCode::FORBIDDEN,
        "B 非メンバーは B のファイルを編集できない"
    );

    // 対照: project B のメンバーは編集できる（過剰拒否でない）。
    app.reset_session_client();
    app.login_session_no_content(&member.email, &member.password)
        .await;
    let allowed = app
        .put_json_with_session(
            &content_path(tp.tenant_id, file_id),
            serde_json::json!({ "content": "edited by member" }),
        )
        .await;
    assert_eq!(
        allowed.status(),
        StatusCode::OK,
        "B メンバーは B のファイルを編集できる"
    );

    // owner を先に削除するとテナント配下がカスケード削除され、残りの削除で FK が衝突しない。
    app.cleanup_user(owner.id).await;
    app.cleanup_user(attacker.id).await;
    app.cleanup_user(member.id).await;
}

/// 差し替えでクォータを超える場合は 413。収まる場合は 200（旧サイズを差し引いて判定している）。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn update_content_enforces_quota_on_size_delta() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    // 5 バイトのファイルを置いてから、テナントのクォータを 10 バイトに絞る。
    let uploaded = upload_file(
        &app,
        tp.tenant_id,
        None,
        "small.txt",
        "text/plain",
        b"hello",
    )
    .await;
    let file_id = file_id_of(&uploaded);
    set_tenant_quota(&app, tp.tenant_id, 10).await;

    // 旧サイズ(5) を差し引いても 10 を超える 20 バイトは拒否。
    let too_large = app
        .put_json_with_session(
            &content_path(tp.tenant_id, file_id),
            serde_json::json!({ "content": "12345678901234567890" }),
        )
        .await;
    assert_eq!(
        too_large.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "クォータを超える差し替えは拒否される"
    );

    // 拒否されたときは中身もサイズも元のまま。
    let unchanged = app
        .get_with_session(&format!("/v1/drive/files/{file_id}/content"))
        .await;
    assert_eq!(
        unchanged.text().await.expect("content body"),
        "hello",
        "拒否時に元の内容が壊れていない"
    );

    // 対照: 旧サイズを差し引けば収まる 8 バイトは通る（過剰拒否でない）。
    let within = app
        .put_json_with_session(
            &content_path(tp.tenant_id, file_id),
            serde_json::json!({ "content": "12345678" }),
        )
        .await;
    assert_eq!(
        within.status(),
        StatusCode::OK,
        "旧サイズを差し引けば収まる差し替えは通る"
    );

    app.cleanup_user(owner.id).await;
}

/// 同一ファイルへの並行更新でも DB とストレージが整合し続けること。
///
/// ハンドラはテナント行と対象ファイル行をロックしてから旧サイズ・旧ストレージキーを
/// 読む。ロックが正しく効いていれば、2 本の更新は直列化され、最終状態はどちらか一方の
/// 内容と完全一致する（部分書き込みや DB と実体の食い違いが起きない）。500 も返さない。
///
/// 旧サイズをロック前に読んでいた頃は、並行更新が互いのサイズ差分を取り違えたり
/// 中間キーを二重削除して壊れ得た。確定的に競合を再現するのは難しいが、`tokio::join`
/// で 2 本同時に投げて「壊れない」ことを回帰として確認する。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn update_content_concurrent_updates_stay_consistent() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let uploaded = upload_file(
        &app,
        tp.tenant_id,
        None,
        "race.txt",
        "text/plain",
        b"initial",
    )
    .await;
    let file_id = file_id_of(&uploaded);

    let url = format!("{}{}", app.base_url(), content_path(tp.tenant_id, file_id));
    let body_a = "A".repeat(64);
    let body_b = "B".repeat(128);

    let client_a = app.session_client();
    let client_b = app.session_client();
    let (url_a, url_b) = (url.clone(), url.clone());
    let (send_a, send_b) = (body_a.clone(), body_b.clone());

    let (res_a, res_b) = tokio::join!(
        async move {
            client_a
                .put(&url_a)
                .json(&serde_json::json!({ "content": send_a }))
                .send()
                .await
        },
        async move {
            client_b
                .put(&url_b)
                .json(&serde_json::json!({ "content": send_b }))
                .send()
                .await
        },
    );

    let status_a = res_a.expect("request a").status();
    let status_b = res_b.expect("request b").status();
    for status in [status_a, status_b] {
        assert_ne!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "並行更新でサーバーエラーを返さない"
        );
        assert!(
            status == StatusCode::OK || status == StatusCode::PAYLOAD_TOO_LARGE,
            "更新は成功かクォータ超過のいずれか (実際: {status})"
        );
    }

    // 最終状態はどちらか一方の内容と完全一致する（部分書き込みでない）。
    let content = app
        .get_with_session(&format!("/v1/drive/files/{file_id}/content"))
        .await;
    assert_eq!(content.status(), StatusCode::OK);
    let final_body = content.text().await.expect("content body");
    assert!(
        final_body == body_a || final_body == body_b,
        "最終内容はいずれかの更新と完全一致するべき (len={})",
        final_body.len()
    );

    // メタデータの size が実体のバイト数と一致する（DB とストレージが食い違わない）。
    let meta = app
        .get_with_session(&format!(
            "/v1/tenants/{}/drive/files/{file_id}",
            tp.tenant_id
        ))
        .await;
    assert_eq!(meta.status(), StatusCode::OK);
    let meta_body: serde_json::Value = meta.json().await.expect("meta json");
    assert_eq!(
        meta_body["size"].as_i64(),
        Some(final_body.len() as i64),
        "size が配信される内容のバイト数と一致する"
    );

    app.cleanup_user(owner.id).await;
}

/// テナントレベルファイル（project_id が NULL）の配信もテナント所属者に限定される。
///
/// 配信エンドポイントは URL に tenant_id を含まずファイル ID だけで引くため、
/// テナントレベルファイルを無条件に許可すると、ID を知る第三者が未認証で内容を取得できる。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn tenant_level_file_content_requires_tenant_access() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    // folder_id を指定しないアップロードは project_id が NULL のテナントレベルファイルになる。
    let uploaded = upload_file(
        &app,
        tp.tenant_id,
        None,
        "secret.txt",
        "text/plain",
        b"tenant secret",
    )
    .await;
    let file_id = file_id_of(&uploaded);
    let path = format!("/v1/drive/files/{file_id}/content");

    // 対照: アップロードしたテナントのオーナーは従来どおり読める。
    let allowed = app.get_with_session(&path).await;
    assert_eq!(
        allowed.status(),
        StatusCode::OK,
        "テナントのオーナーは読める"
    );
    assert_eq!(
        allowed.text().await.expect("content body"),
        "tenant secret",
        "内容が欠けずに取得できる"
    );

    // 未認証では読めない。
    app.reset_session_client();
    let anonymous = app.get(&path).await;
    assert_eq!(
        anonymous.status(),
        StatusCode::FORBIDDEN,
        "未認証では読めない"
    );

    // 別テナントのユーザーでも読めない。
    let outsider = app.insert_user(false, false).await;
    app.insert_tenant_project(outsider.id).await;
    app.reset_session_client();
    app.login_session_no_content(&outsider.email, &outsider.password)
        .await;
    let cross_tenant = app.get(&path).await;
    assert_eq!(
        cross_tenant.status(),
        StatusCode::FORBIDDEN,
        "別テナントのユーザーは読めない"
    );

    app.cleanup_user(outsider.id).await;
    app.cleanup_user(owner.id).await;
}

/// PAT で配信エンドポイントを読む場合も read:drive が必要。
/// 共有トークン経路は PAT スコープと独立しており、未認証のまま利用できる。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn tenant_level_file_content_enforces_pat_scope_but_allows_share_token() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let folder_id = insert_tenant_folder(&app, tp.tenant_id, owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let uploaded = upload_file(
        &app,
        tp.tenant_id,
        Some(folder_id),
        "shared-secret.txt",
        "text/plain",
        b"shared tenant secret",
    )
    .await;
    let file_id = file_id_of(&uploaded);
    let path = format!("/v1/drive/files/{file_id}/content");

    let pat_without_drive = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadProject], None)
        .await;
    let forbidden = app.get_with_bearer(&path, &pat_without_drive).await;
    assert_eq!(
        forbidden.status(),
        StatusCode::FORBIDDEN,
        "read:drive を持たない PAT は読めない"
    );

    let pat_with_drive = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadDrive], None)
        .await;
    let allowed = app.get_with_bearer(&path, &pat_with_drive).await;
    assert_eq!(
        allowed.status(),
        StatusCode::OK,
        "read:drive を持つ PAT は読める"
    );

    let valid_token = "valid-tenant-folder-share";
    insert_token_share(&app, folder_id, owner.id, valid_token, None).await;
    app.reset_session_client();
    let shared = app.get(&format!("{path}?token={valid_token}")).await;
    assert_eq!(
        shared.status(),
        StatusCode::OK,
        "有効な共有トークンは認証や PAT スコープなしで読める"
    );
    assert_eq!(
        shared.text().await.expect("shared content body"),
        "shared tenant secret"
    );

    app.cleanup_user(owner.id).await;
}

/// テナントレベルフォルダの共有トークンは、有効なものだけを受け入れる。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn tenant_level_folder_share_token_rejects_invalid_and_expired_tokens() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let folder_id = insert_tenant_folder(&app, tp.tenant_id, owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let uploaded = upload_file(
        &app,
        tp.tenant_id,
        Some(folder_id),
        "token-guarded.txt",
        "text/plain",
        b"token guarded",
    )
    .await;
    let file_id = file_id_of(&uploaded);
    let path = format!("/v1/drive/files/{file_id}/content");

    let valid_token = "valid-folder-token";
    let expired_token = "expired-folder-token";
    insert_token_share(&app, folder_id, owner.id, valid_token, None).await;
    insert_token_share(
        &app,
        folder_id,
        owner.id,
        expired_token,
        Some(Utc::now() - chrono::Duration::minutes(1)),
    )
    .await;

    app.reset_session_client();
    let valid = app.get(&format!("{path}?token={valid_token}")).await;
    assert_eq!(valid.status(), StatusCode::OK, "有効なトークンは読める");

    let invalid = app.get(&format!("{path}?token=not-a-share-token")).await;
    assert_eq!(
        invalid.status(),
        StatusCode::FORBIDDEN,
        "無効なトークンは読めない"
    );

    let expired = app.get(&format!("{path}?token={expired_token}")).await;
    assert_eq!(
        expired.status(),
        StatusCode::FORBIDDEN,
        "期限切れトークンは読めない"
    );

    app.cleanup_user(owner.id).await;
}

/// 配信エンドポイントは URL にテナントを含まないため、PAT の束縛はファイル自身の
/// tenant_id で突き合わせる。持ち主がオーナーとして入れる別テナントでも、
/// 束縛外のテナントのプロジェクトファイルは読めない。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn project_file_content_rejects_pat_bound_to_other_tenant() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tenant_a = app.insert_tenant_project(owner.id).await;
    let tenant_b = app.insert_tenant_project(owner.id).await;
    let folder_a =
        insert_project_folder(&app, tenant_a.tenant_id, tenant_a.project_id, owner.id).await;
    let folder_b =
        insert_project_folder(&app, tenant_b.tenant_id, tenant_b.project_id, owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let file_a = file_id_of(
        &upload_file(
            &app,
            tenant_a.tenant_id,
            Some(folder_a),
            "a.txt",
            "text/plain",
            b"tenant a",
        )
        .await,
    );
    let file_b = file_id_of(
        &upload_file(
            &app,
            tenant_b.tenant_id,
            Some(folder_b),
            "b.txt",
            "text/plain",
            b"tenant b",
        )
        .await,
    );

    // セッション Cookie を落とし、PAT だけで認証させる。
    app.reset_session_client();
    let pat = app
        .insert_pat(owner.id, tenant_a.tenant_id, vec![Scope::ReadDrive], None)
        .await;

    let cross_tenant = app
        .get_with_bearer(&format!("/v1/drive/files/{file_b}/content"), &pat)
        .await;
    assert_eq!(
        cross_tenant.status(),
        StatusCode::FORBIDDEN,
        "テナント A に束縛した PAT はテナント B のファイル本体を読めない"
    );

    // 対照: 束縛先テナントのプロジェクトファイルは読める（過剰拒否でない）。
    let allowed = app
        .get_with_bearer(&format!("/v1/drive/files/{file_a}/content"), &pat)
        .await;
    assert_eq!(
        allowed.status(),
        StatusCode::OK,
        "束縛先テナントのファイル本体は読める"
    );
    assert_eq!(allowed.text().await.expect("content body"), "tenant a");

    app.cleanup_user(owner.id).await;
}

/// プロジェクト限定の PAT は、同じテナントでも `allowed_project_ids` 外のプロジェクトの
/// ファイル本体を読めない。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn project_file_content_rejects_pat_outside_allowed_projects() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await; // tenant T + project P1
    let project_2 = insert_extra_project(&app, tp.tenant_id).await;
    let folder_1 = insert_project_folder(&app, tp.tenant_id, tp.project_id, owner.id).await;
    let folder_2 = insert_project_folder(&app, tp.tenant_id, project_2, owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let file_1 = file_id_of(
        &upload_file(
            &app,
            tp.tenant_id,
            Some(folder_1),
            "p1.txt",
            "text/plain",
            b"project 1",
        )
        .await,
    );
    let file_2 = file_id_of(
        &upload_file(
            &app,
            tp.tenant_id,
            Some(folder_2),
            "p2.txt",
            "text/plain",
            b"project 2",
        )
        .await,
    );

    app.reset_session_client();
    let pat = app
        .insert_pat(
            owner.id,
            tp.tenant_id,
            vec![Scope::ReadDrive],
            Some(vec![tp.project_id]),
        )
        .await;

    let outside = app
        .get_with_bearer(&format!("/v1/drive/files/{file_2}/content"), &pat)
        .await;
    assert_eq!(
        outside.status(),
        StatusCode::FORBIDDEN,
        "P1 限定の PAT は P2 のファイル本体を読めない"
    );

    // 対照: 許可したプロジェクトのファイルは読める（過剰拒否でない）。
    let allowed = app
        .get_with_bearer(&format!("/v1/drive/files/{file_1}/content"), &pat)
        .await;
    assert_eq!(
        allowed.status(),
        StatusCode::OK,
        "P1 限定の PAT は P1 のファイル本体を読める"
    );
    assert_eq!(allowed.text().await.expect("content body"), "project 1");

    app.cleanup_user(owner.id).await;
}

/// アップロードされた HTML は、ブラウザがその場でレンダリングできない形で配信される。
///
/// 配信は API と同一オリジンで行われるため、`Content-Disposition: inline` と
/// クライアント申告の `Content-Type` をそのまま返すと、保存された HTML がセッション
/// Cookie の届くオリジンで実行できてしまう（stored XSS）。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn content_delivery_does_not_render_uploaded_html() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let body = b"<script>alert(document.domain)</script>";
    let uploaded = upload_file(&app, tp.tenant_id, None, "payload.html", "text/html", body).await;
    let file_id = file_id_of(&uploaded);

    let response = app
        .get_with_session(&format!("/v1/drive/files/{file_id}/content"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let disposition = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .expect("Content-Disposition が付く")
        .to_string();
    assert!(
        disposition.starts_with("attachment;"),
        "HTML は inline ではなく attachment で配信される: {disposition}"
    );

    let nosniff = response
        .headers()
        .get(reqwest::header::HeaderName::from_static(
            "x-content-type-options",
        ))
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    assert_eq!(
        nosniff.as_deref(),
        Some("nosniff"),
        "MIME スニッフィングを禁止するヘッダーが付く"
    );

    // 対照: 配信そのものは成功し、内容も欠けていない（過剰な拒否になっていない）。
    assert_eq!(
        response.text().await.expect("content body").as_bytes(),
        body,
        "内容は変わらず取得できる"
    );

    app.cleanup_user(owner.id).await;
}

/// 一覧の絞り込み用に、名前・MIME・作成日時を決めた行を直接入れる（本文は読まない）。
async fn insert_listed_file(
    app: &TestApp,
    tenant_id: Uuid,
    folder_id: Uuid,
    uploader: Uuid,
    name: &str,
    mime: &str,
    created_at: DateTime<Utc>,
) -> Uuid {
    let file_id = Uuid::new_v4();
    drive_files::ActiveModel {
        id: Set(file_id),
        name: Set(name.into()),
        size: Set(1),
        mime_type: Set(mime.into()),
        storage_type: Set(drive_files::StorageType::Local),
        storage_key: Set(Uuid::new_v4().to_string()),
        tenant_id: Set(tenant_id),
        project_id: Set(None),
        uploader_id: Set(uploader),
        folder_id: Set(Some(folder_id)),
        created_at: Set(created_at.into()),
        updated_at: Set(created_at.into()),
        content_hash: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert listed file");
    file_id
}

/// フォルダ内のファイル一覧を引き、`(total, files)` を返す。
async fn list_files_in(
    app: &TestApp,
    tenant_id: Uuid,
    folder_id: Uuid,
    query: &[(&str, &str)],
) -> (u64, Vec<serde_json::Value>) {
    let response = list_files_response(app, tenant_id, folder_id, query).await;
    assert_eq!(response.status(), StatusCode::OK, "query: {query:?}");
    let body: serde_json::Value = response.json().await.expect("json");
    (
        body["total"].as_u64().expect("total"),
        body["files"].as_array().expect("files").clone(),
    )
}

async fn list_files_response(
    app: &TestApp,
    tenant_id: Uuid,
    folder_id: Uuid,
    query: &[(&str, &str)],
) -> reqwest::Response {
    let folder = folder_id.to_string();
    let url = reqwest::Url::parse_with_params(
        &format!("{}/v1/tenants/{tenant_id}/drive/files", app.base_url()),
        std::iter::once(&("folder_id", folder.as_str())).chain(query),
    )
    .expect("list url");
    app.client().get(url).send().await.expect("list files")
}

fn names_of(files: &[serde_json::Value]) -> Vec<&str> {
    files
        .iter()
        .map(|f| f["name"].as_str().expect("name"))
        .collect()
}

/// `mime_type` は前方一致: 完全な型なら完全一致、末尾 `/` なら種別全体。
/// 利用者入力の `%` `_` はワイルドカードにならない。件数は既定の limit（50）を越えさせ、
/// 絞り込み後もページングと `total` が効くことを見る。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn list_files_filters_by_mime_type_prefix() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let folder = insert_tenant_folder(&app, tp.tenant_id, owner.id).await;
    let base = Utc::now();
    for i in 0..55 {
        let at = base + chrono::Duration::seconds(i);
        insert_listed_file(
            &app,
            tp.tenant_id,
            folder,
            owner.id,
            &format!("doc-{i:02}.md"),
            "text/markdown",
            at,
        )
        .await;
    }
    insert_listed_file(
        &app,
        tp.tenant_id,
        folder,
        owner.id,
        "extra.md",
        "text/markdown-extra",
        base,
    )
    .await;
    for i in 0..3 {
        insert_listed_file(
            &app,
            tp.tenant_id,
            folder,
            owner.id,
            &format!("plain-{i}.txt"),
            "text/plain",
            base,
        )
        .await;
    }
    for i in 0..2 {
        insert_listed_file(
            &app,
            tp.tenant_id,
            folder,
            owner.id,
            &format!("image-{i}.png"),
            "image/png",
            base,
        )
        .await;
    }

    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let (total, first) = list_files_in(
        &app,
        tp.tenant_id,
        folder,
        &[("mime_type", "text/markdown")],
    )
    .await;
    assert_eq!(
        total, 56,
        "text/markdown は前方一致（text/markdown-extra も含む）"
    );
    assert_eq!(first.len(), 50, "既定の limit で切れる");
    let (_, rest) = list_files_in(
        &app,
        tp.tenant_id,
        folder,
        &[("mime_type", "text/markdown"), ("offset", "50")],
    )
    .await;
    assert_eq!(rest.len(), 6, "残りは次のページに出る");
    assert!(
        first.iter().chain(&rest).all(|f| f["mime_type"]
            .as_str()
            .unwrap()
            .starts_with("text/markdown")),
        "markdown 以外が混ざらない"
    );

    let (text_total, _) =
        list_files_in(&app, tp.tenant_id, folder, &[("mime_type", "text/")]).await;
    assert_eq!(text_total, 59, "text/ は text 全体");

    for wildcard in ["%", "text_", "%markdown"] {
        let (total, _) =
            list_files_in(&app, tp.tenant_id, folder, &[("mime_type", wildcard)]).await;
        assert_eq!(total, 0, "{wildcard} はワイルドカードとして効かない");
    }

    let (all, _) = list_files_in(&app, tp.tenant_id, folder, &[]).await;
    assert_eq!(all, 61, "省略時は従来どおり全件");

    app.cleanup_user(owner.id).await;
}

/// `q` はファイル名の部分一致で、大文字小文字を区別せず、`%` `_` を字義どおりに扱う。
/// 長さは 200 文字まで（201 文字で 400）。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn list_files_searches_names_case_insensitively() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let folder = insert_tenant_folder(&app, tp.tenant_id, owner.id).await;
    let now = Utc::now();
    for name in [
        "Design-Notes.md",
        "design-review.md",
        "other.md",
        "100%_done.md",
        "100x-done.md",
    ] {
        insert_listed_file(
            &app,
            tp.tenant_id,
            folder,
            owner.id,
            name,
            "text/markdown",
            now,
        )
        .await;
    }

    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let (total, files) = list_files_in(
        &app,
        tp.tenant_id,
        folder,
        &[("q", "DESIGN"), ("sort", "name_asc")],
    )
    .await;
    assert_eq!(total, 2);
    assert_eq!(names_of(&files), ["Design-Notes.md", "design-review.md"]);

    let (_, literal) = list_files_in(&app, tp.tenant_id, folder, &[("q", "0%_")]).await;
    assert_eq!(
        names_of(&literal),
        ["100%_done.md"],
        "% と _ はワイルドカードにならない"
    );

    let max = "a".repeat(200);
    let (none, _) = list_files_in(&app, tp.tenant_id, folder, &[("q", max.as_str())]).await;
    assert_eq!(none, 0, "200 文字は受け付ける");

    let too_long = "a".repeat(201);
    let rejected =
        list_files_response(&app, tp.tenant_id, folder, &[("q", too_long.as_str())]).await;
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST, "201 文字は 400");

    app.cleanup_user(owner.id).await;
}

/// `sort` は既定の作成日時降順のほか、名前昇順・更新日時降順を選べる。未知の値は 400。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn list_files_sorts_by_requested_order() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let folder = insert_tenant_folder(&app, tp.tenant_id, owner.id).await;
    let base = Utc::now();
    // 作成順は b → c → a。名前順と作成日時順が食い違うように置く
    for (i, name) in ["b.md", "c.md", "a.md"].into_iter().enumerate() {
        let at = base + chrono::Duration::seconds(i as i64);
        insert_listed_file(
            &app,
            tp.tenant_id,
            folder,
            owner.id,
            name,
            "text/markdown",
            at,
        )
        .await;
    }

    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let (_, default) = list_files_in(&app, tp.tenant_id, folder, &[]).await;
    assert_eq!(
        names_of(&default),
        ["a.md", "c.md", "b.md"],
        "既定は新しい順"
    );

    let (_, by_name) = list_files_in(&app, tp.tenant_id, folder, &[("sort", "name_asc")]).await;
    assert_eq!(names_of(&by_name), ["a.md", "b.md", "c.md"]);

    let (_, by_updated) =
        list_files_in(&app, tp.tenant_id, folder, &[("sort", "updated_at_desc")]).await;
    assert_eq!(names_of(&by_updated), ["a.md", "c.md", "b.md"]);

    let rejected = list_files_response(&app, tp.tenant_id, folder, &[("sort", "name_desc")]).await;
    assert_eq!(
        rejected.status(),
        StatusCode::BAD_REQUEST,
        "未知の sort は 400"
    );

    app.cleanup_user(owner.id).await;
}

// ---- ETag / If-Match / If-None-Match ----

fn sha256_etag(body: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("\"sha256:{}\"", hex::encode(Sha256::digest(body)))
}

fn etag_of(file: &serde_json::Value) -> String {
    file["etag"].as_str().expect("etag is string").to_string()
}

fn header_str(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .map(|v| v.to_str().expect("header is ascii").to_string())
}

/// `If-Match` 付きで本文を更新する。
async fn put_content_if_match(
    app: &TestApp,
    tenant_id: Uuid,
    file_id: Uuid,
    if_match: Option<&str>,
    body: &str,
) -> reqwest::Response {
    let mut request = app
        .client()
        .put(format!(
            "{}{}",
            app.base_url(),
            content_path(tenant_id, file_id)
        ))
        .json(&serde_json::json!({ "content": body }));
    if let Some(tag) = if_match {
        request = request.header("If-Match", tag);
    }
    request.send().await.expect("put content request")
}

/// `If-None-Match` 付きで本文を取得する。
async fn get_content_if_none_match(
    app: &TestApp,
    file_id: Uuid,
    if_none_match: &str,
) -> reqwest::Response {
    app.client()
        .get(format!(
            "{}/v1/drive/files/{file_id}/content",
            app.base_url()
        ))
        .header("If-None-Match", if_none_match)
        .send()
        .await
        .expect("get content request")
}

async fn read_content(app: &TestApp, file_id: Uuid) -> String {
    let response = app
        .get_with_session(&format!("/v1/drive/files/{file_id}/content"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.text().await.expect("content body")
}

/// アップロード直後の `etag` は本文の SHA-256。本文取得の `ETag` ヘッダーも同じ値で、
/// `Last-Modified` は `updated_at`（秒精度）と一致する。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn upload_etag_is_content_sha256_and_served_on_get() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let body = b"# hello\n\netag";
    let uploaded = upload_file(&app, tp.tenant_id, None, "doc.md", "text/markdown", body).await;
    let file_id = file_id_of(&uploaded);
    assert_eq!(
        etag_of(&uploaded),
        sha256_etag(body),
        "アップロード直後の etag は本文の SHA-256"
    );

    let response = app
        .get_with_session(&format!("/v1/drive/files/{file_id}/content"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_str(&response, "etag").as_deref(),
        Some(sha256_etag(body).as_str()),
        "ETag ヘッダーは応答 JSON の etag と同じ値"
    );
    let last_modified = header_str(&response, "last-modified").expect("Last-Modified がある");
    let last_modified = DateTime::parse_from_rfc2822(&last_modified)
        .expect("Last-Modified は HTTP-date")
        .with_timezone(&Utc);
    assert_eq!(
        last_modified.timestamp(),
        timestamp_of(&uploaded, "updated_at").timestamp(),
        "Last-Modified は updated_at と一致する"
    );

    // メタデータ API にも同じ etag が載る
    let meta = app
        .get_with_session(&format!(
            "/v1/tenants/{}/drive/files/{file_id}",
            tp.tenant_id
        ))
        .await;
    assert_eq!(meta.status(), StatusCode::OK);
    let meta: serde_json::Value = meta.json().await.expect("meta json");
    assert_eq!(etag_of(&meta), sha256_etag(body));

    app.cleanup_user(owner.id).await;
}

/// `If-None-Match` が一致すれば 304（本文なし・ETag あり）、一致しなければ 200。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn get_content_if_none_match_returns_not_modified() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let uploaded = upload_file(&app, tp.tenant_id, None, "a.txt", "text/plain", b"cached").await;
    let file_id = file_id_of(&uploaded);
    let etag = etag_of(&uploaded);

    // 弱い ETag（中継が圧縮時に付け替えたもの）も一致として 304
    for value in [
        etag.clone(),
        format!("\"other\", {etag}"),
        "*".to_string(),
        format!("W/{etag}"),
    ] {
        let response = get_content_if_none_match(&app, file_id, &value).await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_MODIFIED,
            "If-None-Match: {value} は 304"
        );
        assert_eq!(
            header_str(&response, "etag").as_deref(),
            Some(etag.as_str())
        );
        assert!(
            response.bytes().await.expect("body").is_empty(),
            "304 は本文なし"
        );
    }

    // 不一致・弱くても値が違うものは 200 で本文を返す
    for value in [
        "\"sha256:stale\"".to_string(),
        "W/\"sha256:stale\"".to_string(),
    ] {
        let response = get_content_if_none_match(&app, file_id, &value).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "If-None-Match: {value} は 200"
        );
        assert_eq!(response.text().await.expect("body"), "cached");
    }

    app.cleanup_user(owner.id).await;
}

/// `If-Match` 一致で更新でき新しい etag が返る。不一致は 412 で本文は変わらない。
/// `*` は一致扱い、ヘッダー無しは従来どおり無条件に上書きする。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn update_content_if_match_guards_overwrite() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let uploaded = upload_file(&app, tp.tenant_id, None, "doc.md", "text/markdown", b"v1").await;
    let file_id = file_id_of(&uploaded);
    let etag_v1 = etag_of(&uploaded);

    // 一致 → 200、etag は新しい本文の SHA-256
    let response = put_content_if_match(&app, tp.tenant_id, file_id, Some(&etag_v1), "v2").await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "If-Match 一致は更新できる"
    );
    let updated: serde_json::Value = response.json().await.expect("json");
    let etag_v2 = etag_of(&updated);
    assert_eq!(etag_v2, sha256_etag(b"v2"), "応答の etag は更新後の値");
    assert_ne!(etag_v2, etag_v1);

    // 古い etag → 412、本文は v2 のまま
    let response = put_content_if_match(&app, tp.tenant_id, file_id, Some(&etag_v1), "lost").await;
    assert_eq!(
        response.status(),
        StatusCode::PRECONDITION_FAILED,
        "古い etag は 412"
    );
    assert_eq!(
        read_content(&app, file_id).await,
        "v2",
        "412 では本文が変わらない"
    );

    // 弱い ETag（Cloudflare などの中継が圧縮時に `W/` を付けたもの）も一致として更新できる（TASK-336）。
    // ブラウザは受け取った ETag をそのまま If-Match に戻すので、ここを 412 にすると保存が一切通らない
    let response = put_content_if_match(
        &app,
        tp.tenant_id,
        file_id,
        Some(&format!("W/{etag_v2}")),
        "weak",
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "弱い ETag でも値が同じなら更新できる"
    );
    let updated: serde_json::Value = response.json().await.expect("json");
    let etag_weak = etag_of(&updated);
    assert_eq!(etag_weak, sha256_etag(b"weak"));
    assert_eq!(read_content(&app, file_id).await, "weak");

    // 弱くても値が違えば 412
    let response = put_content_if_match(
        &app,
        tp.tenant_id,
        file_id,
        Some(&format!("W/{etag_v2}")),
        "lost",
    )
    .await;
    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(read_content(&app, file_id).await, "weak");

    // `*` → 200
    let response = put_content_if_match(&app, tp.tenant_id, file_id, Some("*"), "v3").await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "If-Match: * は更新できる"
    );
    assert_eq!(read_content(&app, file_id).await, "v3");

    // ヘッダー無し → 無条件に上書き
    let response = put_content_if_match(&app, tp.tenant_id, file_id, None, "v4").await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "If-Match 無しは上書きする"
    );
    let updated: serde_json::Value = response.json().await.expect("json");
    assert_eq!(etag_of(&updated), sha256_etag(b"v4"));
    assert_eq!(read_content(&app, file_id).await, "v4");

    app.cleanup_user(owner.id).await;
}

/// 同じ etag を持った 2 つの更新を同時に投げると、片方だけが通り、もう片方は 412。
/// 比較が行ロックの後なので、後から入った側は先行の更新後の値と比べることになる。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn update_content_if_match_concurrent_one_wins() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let uploaded = upload_file(
        &app,
        tp.tenant_id,
        None,
        "race.md",
        "text/markdown",
        b"base",
    )
    .await;
    let file_id = file_id_of(&uploaded);
    let etag = etag_of(&uploaded);

    let (res_a, res_b) = tokio::join!(
        put_content_if_match(&app, tp.tenant_id, file_id, Some(&etag), "from A"),
        put_content_if_match(&app, tp.tenant_id, file_id, Some(&etag), "from B"),
    );
    let mut statuses = [res_a.status(), res_b.status()];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::OK, StatusCode::PRECONDITION_FAILED],
        "片方が 200、片方が 412"
    );

    let winner = if res_a.status() == StatusCode::OK {
        "from A"
    } else {
        "from B"
    };
    assert_eq!(
        read_content(&app, file_id).await,
        winner,
        "本文は通った側の内容"
    );

    app.cleanup_user(owner.id).await;
}

/// `content_hash` を持たない既存行は `"ts:<updated_at ミリ秒>-<size>"` を etag にし、
/// `If-Match` もその値で効く。更新後はハッシュの etag に切り替わる。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn legacy_row_without_hash_uses_timestamp_etag() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;

    let uploaded = upload_file(&app, tp.tenant_id, None, "old.txt", "text/plain", b"legacy").await;
    let file_id = file_id_of(&uploaded);

    // マイグレーション前に作られた行を再現する
    let model = drive_files::Entity::find_by_id(file_id)
        .one(&app.state.db)
        .await
        .expect("load file")
        .expect("file exists");
    let mut active: drive_files::ActiveModel = model.into();
    active.content_hash = Set(None);
    let legacy = active.update(&app.state.db).await.expect("drop hash");
    let expected = format!(
        "\"ts:{}-{}\"",
        legacy.updated_at.timestamp_millis(),
        legacy.size
    );

    let meta = app
        .get_with_session(&format!(
            "/v1/tenants/{}/drive/files/{file_id}",
            tp.tenant_id
        ))
        .await;
    assert_eq!(meta.status(), StatusCode::OK);
    let meta: serde_json::Value = meta.json().await.expect("meta json");
    assert_eq!(etag_of(&meta), expected, "ハッシュが無い行は ts: 形式");

    let response = get_content_if_none_match(&app, file_id, &expected).await;
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    // サイズ違いの ts: は不一致
    let stale = format!("\"ts:{}-0\"", legacy.updated_at.timestamp_millis());
    let response = put_content_if_match(&app, tp.tenant_id, file_id, Some(&stale), "x").await;
    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(read_content(&app, file_id).await, "legacy");

    let response = put_content_if_match(&app, tp.tenant_id, file_id, Some(&expected), "new").await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "ts: の etag で更新できる"
    );
    let updated: serde_json::Value = response.json().await.expect("json");
    assert_eq!(
        etag_of(&updated),
        sha256_etag(b"new"),
        "更新後はハッシュの etag になる"
    );

    app.cleanup_user(owner.id).await;
}
