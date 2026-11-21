//! MCP の文書のツール（`search_documents`、`get_document`）。規則は docs/dev/features/mcp.md。
//!
//! - `search_documents`: 本文検索の API を呼び直す。指したプロジェクトの節だけが返り、
//!   入れないプロジェクト・束縛の外・scope の不足は `isError` の結果になる
//! - `get_document`: 本文の配信 API を呼び直し、Markdown の本文をそのまま返す。届く範囲は配信 API と同じ
//!
//! 拒否と対照の成功を組で確かめる。Drive の行を作るので、他の Drive の試験と同じ鍵で直列化する
//! （理由は `drive_file_content_integration` の冒頭）。

use crate::common::{TestApp, TestUser, file_id_of, insert_extra_project, upload_drive_file};
use axum::http::StatusCode;
use chrono::Utc;
use entity::{drive_folder_shares, drive_folders, project_members, projects, scopes::Scope};
use reqwest::Response;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use serde_json::{Value, json};
use uuid::Uuid;

const DOC: &str = "前置き\n\n# 認証\n\n概要です\n\n## トークン\n\n更新は mcpdocword を使う\n";

fn tool_call_body(name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments },
    })
}

/// ツールの結果。`is_error` と、本文の文字列（成功は API の応答、失敗はエラー文）。
struct ToolOutcome {
    is_error: bool,
    text: String,
}

impl ToolOutcome {
    fn json(&self) -> Value {
        serde_json::from_str(self.text()).expect("tool result json")
    }

    fn text(&self) -> &str {
        assert!(!self.is_error, "ツールが失敗した: {}", self.text);
        &self.text
    }

    fn error(&self) -> &str {
        assert!(self.is_error, "ツールが成功した: {}", self.text);
        &self.text
    }
}

async fn outcome_of(response: Response) -> ToolOutcome {
    let status = response.status();
    let body: Value = response.json().await.expect("mcp json");
    assert_eq!(status, StatusCode::OK, "tools/call: {body}");
    let result = &body["result"];
    assert!(result.is_object(), "tools/call に result が無い: {body}");
    ToolOutcome {
        is_error: result["isError"].as_bool().unwrap_or(false),
        text: result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("content[0].text が無い: {body}"))
            .to_string(),
    }
}

async fn call(app: &TestApp, token: &str, name: &str, arguments: Value) -> ToolOutcome {
    outcome_of(app.mcp(Some(token), tool_call_body(name, arguments)).await).await
}

async fn search(app: &TestApp, token: &str, project: &str, q: &str) -> ToolOutcome {
    call(
        app,
        token,
        "search_documents",
        json!({ "project": project, "q": q }),
    )
    .await
}

async fn get_document(app: &TestApp, token: &str, file_id: Uuid) -> ToolOutcome {
    call(
        app,
        token,
        "get_document",
        json!({ "document": file_id.to_string() }),
    )
    .await
}

async fn insert_folder(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Option<Uuid>,
    created_by: Uuid,
) -> Uuid {
    let folder_id = Uuid::new_v4();
    drive_folders::ActiveModel {
        id: Set(folder_id),
        name: Set(format!("folder-{folder_id}")),
        parent_id: Set(None),
        tenant_id: Set(tenant_id),
        project_id: Set(project_id),
        created_by: Set(created_by),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert folder");
    folder_id
}

async fn upload_md(app: &TestApp, tenant_id: Uuid, folder_id: Option<Uuid>, body: &str) -> Uuid {
    file_id_of(
        &upload_drive_file(
            app,
            tenant_id,
            folder_id,
            "spec.md",
            "text/markdown",
            body.as_bytes(),
        )
        .await,
    )
}

/// 指定した利用者だけをメンバーにして、プロジェクトをテナント全体から閉じる。
async fn restrict_to(app: &TestApp, project_id: Uuid, user_id: Uuid) {
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
    crate::common::stop_inheriting(&app.state.db, project_id).await;
}

/// 同じテナントの 2 つのプロジェクト（A・B）と、同じオーナーの別テナントのプロジェクト。
/// どれもルートフォルダに、同じ語を含む Markdown を 1 つ持つ。
struct Fixture {
    owner: TestUser,
    tenant_id: Uuid,
    project_a: Uuid,
    folder_b: Uuid,
    project_b: Uuid,
    file_a: Uuid,
    file_b: Uuid,
    other_tenant_id: Uuid,
    other_project: Uuid,
    other_file: Uuid,
}

/// アップロードにセッションが要るので、`app` のクライアントはオーナーでログイン済みになる。
async fn fixture(app: &mut TestApp) -> Fixture {
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let project_b = insert_extra_project(app, tp.tenant_id).await;
    let other = app.insert_tenant_project(owner.id).await;

    let folder_a = insert_folder(app, tp.tenant_id, Some(tp.project_id), owner.id).await;
    let folder_b = insert_folder(app, tp.tenant_id, Some(project_b), owner.id).await;
    let other_folder = insert_folder(app, other.tenant_id, Some(other.project_id), owner.id).await;
    let file_a = upload_md(app, tp.tenant_id, Some(folder_a), DOC).await;
    let file_b = upload_md(app, tp.tenant_id, Some(folder_b), "# b\n\nmcpdocword\n").await;
    let other_file = upload_md(
        app,
        other.tenant_id,
        Some(other_folder),
        "# other\n\nmcpdocword\n",
    )
    .await;

    Fixture {
        owner,
        tenant_id: tp.tenant_id,
        project_a: tp.project_id,
        folder_b,
        project_b,
        file_a,
        file_b,
        other_tenant_id: other.tenant_id,
        other_project: other.project_id,
        other_file,
    }
}

/// オーナーの、`read:drive` だけを持つ PAT（テナントに束縛、プロジェクトは絞らない）。
async fn drive_pat(app: &TestApp, fx: &Fixture) -> String {
    app.insert_pat(fx.owner.id, fx.tenant_id, vec![Scope::ReadDrive], None)
        .await
}

/// プロジェクト B を `member` だけに閉じ、B に入れないテナントメンバーを返す。
async fn close_project_b(app: &TestApp, fx: &Fixture) -> (TestUser, TestUser) {
    let member = app.insert_user(false, false).await;
    let non_member = app.insert_user(false, false).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, fx.project_b, non_member.id)
        .await;
    restrict_to(app, fx.project_b, member.id).await;
    (member, non_member)
}

// ---- search_documents ----

/// `read:drive` だけの PAT で、指したプロジェクトの節が返る。結果のファイルはそのまま
/// `get_document` に渡せる。
#[tokio::test]
#[serial_test::serial(drive)]
async fn search_documents_returns_the_sections_of_the_project() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = drive_pat(&app, &fx).await;

    // `tenant` を省き、`project` を UUID で指す（名前解決の scope が要らない）
    let found = search(&app, &token, &fx.project_a.to_string(), "mcpdocword")
        .await
        .json();
    // 同じ語は B と別テナントにもあるが、指したプロジェクトの節だけが返る
    assert_eq!(found["total"], 1, "{found}");
    assert_eq!(found["hits"].as_array().map(Vec::len), Some(1), "{found}");
    let hit = &found["hits"][0];
    assert_eq!(hit["file_id"], fx.file_a.to_string());
    assert_eq!(hit["file_name"], "spec.md");
    assert_eq!(hit["heading_path"], json!(["認証", "トークン"]));
    let excerpt = hit["excerpt"].as_str().expect("excerpt");
    assert!(excerpt.contains("更新は mcpdocword を使う"), "{excerpt}");

    // `limit` と `offset` は API へ渡る
    let paged = call(
        &app,
        &token,
        "search_documents",
        json!({
            "project": fx.project_a.to_string(),
            "q": "mcpdocword",
            "limit": 1,
            "offset": 1,
            "tenant": "",
        }),
    )
    .await
    .json();
    assert_eq!(paged["total"], 1, "{paged}");
    assert_eq!(paged["hits"], json!([]), "{paged}");

    // 結果の `file_id` で本文が取れる
    let file_id = Uuid::parse_str(hit["file_id"].as_str().expect("file_id")).expect("uuid");
    assert_eq!(get_document(&app, &token, file_id).await.text(), DOC);

    app.cleanup_user(fx.owner.id).await;
}

/// 入れないプロジェクト、束縛していないテナント、別テナントのプロジェクト、`q` の誤り。
#[tokio::test]
#[serial_test::serial(drive)]
async fn search_documents_refuses_projects_out_of_reach() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let (member, non_member) = close_project_b(&app, &fx).await;
    let owner_token = drive_pat(&app, &fx).await;
    let pat = |user_id: Uuid| app.insert_pat(user_id, fx.tenant_id, vec![Scope::ReadDrive], None);

    // 非公開のプロジェクトで、メンバーでない
    let token = pat(non_member.id).await;
    let denied = search(&app, &token, &fx.project_b.to_string(), "mcpdocword").await;
    assert_eq!(denied.error(), "403 forbidden");
    // 対照: 同じ PAT で、入れるプロジェクトは引ける。B の節は混ざらない
    let found = search(&app, &token, &fx.project_a.to_string(), "mcpdocword")
        .await
        .json();
    assert_eq!(found["total"], 1, "{found}");
    assert_eq!(found["hits"][0]["file_id"], fx.file_a.to_string());
    // 対照: B のメンバーは B を引ける
    let found = search(
        &app,
        &pat(member.id).await,
        &fx.project_b.to_string(),
        "mcpdocword",
    )
    .await
    .json();
    assert_eq!(found["total"], 1, "{found}");
    assert_eq!(found["hits"][0]["file_id"], fx.file_b.to_string());

    // 束縛していないテナント（持ち主は同じ）
    let denied = call(
        &app,
        &owner_token,
        "search_documents",
        json!({
            "project": fx.other_project.to_string(),
            "q": "mcpdocword",
            "tenant": fx.other_tenant_id.to_string(),
        }),
    )
    .await;
    assert_eq!(denied.error(), "403 forbidden");
    // 対照: そのテナントに束縛した PAT なら引ける
    let other_token = app
        .insert_pat(
            fx.owner.id,
            fx.other_tenant_id,
            vec![Scope::ReadDrive],
            None,
        )
        .await;
    let found = search(
        &app,
        &other_token,
        &fx.other_project.to_string(),
        "mcpdocword",
    )
    .await
    .json();
    assert_eq!(found["total"], 1, "{found}");

    // 別テナントのプロジェクトを、自分のテナントで指す
    let missing = search(
        &app,
        &owner_token,
        &fx.other_project.to_string(),
        "mcpdocword",
    )
    .await;
    assert_eq!(missing.error(), "404 not-found");

    // `q` の検証は API がそのまま返す
    let invalid = search(&app, &owner_token, &fx.project_a.to_string(), "  ").await;
    assert_eq!(invalid.error(), "400 bad-request");

    for user in [&member, &non_member] {
        app.cleanup_user(user.id).await;
    }
    app.cleanup_user(fx.owner.id).await;
}

/// scope とプロジェクトの束縛。`project` をキーで渡すと `read:project` も要る。
#[tokio::test]
#[serial_test::serial(drive)]
async fn search_documents_honors_scope_and_project_binding() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let pat = |scopes: Vec<Scope>, projects: Option<Vec<Uuid>>| {
        app.insert_pat(fx.owner.id, fx.tenant_id, scopes, projects)
    };

    // プロジェクト A に束縛した PAT
    let bound = pat(vec![Scope::ReadDrive], Some(vec![fx.project_a])).await;
    let denied = search(&app, &bound, &fx.project_b.to_string(), "mcpdocword").await;
    assert_eq!(denied.error(), "403 forbidden");
    let found = search(&app, &bound, &fx.project_a.to_string(), "mcpdocword")
        .await
        .json();
    assert_eq!(found["total"], 1, "{found}");

    // `read:drive` の無い PAT
    let no_drive = pat(vec![Scope::ReadProject, Scope::ReadTask], None).await;
    let denied = search(&app, &no_drive, &fx.project_a.to_string(), "mcpdocword").await;
    assert_eq!(denied.error(), "403 forbidden");

    // キーで指すと、ID を引くのに `read:project` が要る
    let key = projects::Entity::find_by_id(fx.project_a)
        .one(&app.state.db)
        .await
        .expect("find project")
        .expect("project")
        .key;
    let drive_only = pat(vec![Scope::ReadDrive], None).await;
    let denied = search(&app, &drive_only, &key, "mcpdocword").await;
    assert_eq!(
        denied.error(),
        format!(
            "403 forbidden (could not look up project key \"{key}\": the token may lack the read:project scope, or it is not allowed in this tenant; if the scope is missing, pass the project UUID instead)"
        )
    );
    let with_project = pat(vec![Scope::ReadDrive, Scope::ReadProject], None).await;
    let found = search(&app, &with_project, &key, "mcpdocword").await.json();
    assert_eq!(found["total"], 1, "{found}");
    assert_eq!(found["hits"][0]["file_id"], fx.file_a.to_string());

    app.cleanup_user(fx.owner.id).await;
}

// ---- get_document ----

/// Markdown の本文を、JSON にせずそのまま返す。Markdown でないファイルと、引数の誤りは断る。
#[tokio::test]
#[serial_test::serial(drive)]
async fn get_document_returns_the_markdown_text() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = drive_pat(&app, &fx).await;

    let document = get_document(&app, &token, fx.file_a).await;
    assert_eq!(document.text(), DOC);

    // `read:drive` の無い PAT
    let no_drive = app
        .insert_pat(
            fx.owner.id,
            fx.tenant_id,
            vec![Scope::ReadProject, Scope::ReadTask],
            None,
        )
        .await;
    let denied = get_document(&app, &no_drive, fx.file_a).await;
    assert_eq!(denied.error(), "403 forbidden");

    // Markdown でないファイル。テナント直下に置く
    let plain = upload_drive_file(
        &app,
        fx.tenant_id,
        None,
        "notes.txt",
        "text/plain",
        b"plain text\n",
    )
    .await;
    let mime = plain["mime_type"].as_str().expect("mime_type");
    let refused = get_document(&app, &token, file_id_of(&plain)).await;
    assert_eq!(
        refused.error(),
        format!("The content type {mime} is not supported")
    );
    // 対照: テナント直下の Markdown は返る
    let general = upload_md(&app, fx.tenant_id, None, "# general\n\n本文\n").await;
    assert_eq!(
        get_document(&app, &token, general).await.text(),
        "# general\n\n本文\n"
    );

    let malformed = call(
        &app,
        &token,
        "get_document",
        json!({ "document": "spec.md" }),
    )
    .await;
    assert_eq!(malformed.error(), "document must be a UUID");
    let missing = get_document(&app, &token, Uuid::new_v4()).await;
    assert_eq!(missing.error(), "404 not-found");

    app.cleanup_user(fx.owner.id).await;
}

/// 別テナントのファイル、入れないプロジェクトのファイル、束縛の外のプロジェクトのファイル。
#[tokio::test]
#[serial_test::serial(drive)]
async fn get_document_refuses_files_out_of_reach() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let (member, non_member) = close_project_b(&app, &fx).await;
    let token = drive_pat(&app, &fx).await;

    // 別テナントのファイル（持ち主は同じ。PAT はそのテナントに束縛していない）
    let other_general = upload_md(&app, fx.other_tenant_id, None, "# general\n\n本文\n").await;
    for file_id in [fx.other_file, other_general] {
        let denied = get_document(&app, &token, file_id).await;
        assert_eq!(denied.error(), "403 forbidden");
    }
    // 対照: そのテナントに束縛した PAT なら読める
    let other_token = app
        .insert_pat(
            fx.owner.id,
            fx.other_tenant_id,
            vec![Scope::ReadDrive],
            None,
        )
        .await;
    for file_id in [fx.other_file, other_general] {
        assert!(!get_document(&app, &other_token, file_id).await.is_error);
    }

    // 非公開のプロジェクトで、メンバーでない
    let outsider = app
        .insert_pat(non_member.id, fx.tenant_id, vec![Scope::ReadDrive], None)
        .await;
    let denied = get_document(&app, &outsider, fx.file_b).await;
    assert_eq!(denied.error(), "403 forbidden");
    assert_eq!(get_document(&app, &outsider, fx.file_a).await.text(), DOC);

    // プロジェクト A に束縛した PAT
    let bound = app
        .insert_pat(
            fx.owner.id,
            fx.tenant_id,
            vec![Scope::ReadDrive],
            Some(vec![fx.project_a]),
        )
        .await;
    let denied = get_document(&app, &bound, fx.file_b).await;
    assert_eq!(denied.error(), "403 forbidden");
    assert_eq!(get_document(&app, &bound, fx.file_a).await.text(), DOC);

    for user in [&member, &non_member] {
        app.cleanup_user(user.id).await;
    }
    app.cleanup_user(fx.owner.id).await;
}

/// 届く範囲は配信 API と同じ。フォルダを利用者指定で共有された人は、検索はできないが本文は読める。
#[tokio::test]
#[serial_test::serial(drive)]
async fn get_document_follows_user_shares() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let (member, shared) = close_project_b(&app, &fx).await;
    let token = app
        .insert_pat(shared.id, fx.tenant_id, vec![Scope::ReadDrive], None)
        .await;

    let denied = get_document(&app, &token, fx.file_b).await;
    assert_eq!(denied.error(), "403 forbidden");

    drive_folder_shares::ActiveModel {
        id: Set(Uuid::new_v4()),
        folder_id: Set(fx.folder_b),
        shared_with_user_id: Set(Some(shared.id)),
        share_token: Set(None),
        permission: Set(drive_folder_shares::SharePermission::Viewer),
        created_by: Set(fx.owner.id),
        expires_at: Set(None),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert user share");

    assert_eq!(
        get_document(&app, &token, fx.file_b).await.text(),
        "# b\n\nmcpdocword\n"
    );
    let denied = search(&app, &token, &fx.project_b.to_string(), "mcpdocword").await;
    assert_eq!(denied.error(), "403 forbidden");

    for user in [&member, &shared] {
        app.cleanup_user(user.id).await;
    }
    app.cleanup_user(fx.owner.id).await;
}
