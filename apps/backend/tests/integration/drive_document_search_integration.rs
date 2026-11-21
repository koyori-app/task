// テストケースの一覧
// - 本文の索引（アップロード、本文の更新、write:task の無い PAT、Markdown 以外、NUL、削除、移動）
// - 本文の検索（見出しの経路・抜粋、語の AND、ページング、q の規則、認可、PAT の束縛と scope、共有）
// - backfill（取得数の境界、冪等、実体の無いファイル、読んだ後に更新された本文）
//
// Drive の行を作るので、他の Drive の試験と同じ鍵で直列化する
// （理由は `drive_file_content_integration` の冒頭）。

use crate::common::{TestApp, TestUser, file_id_of, insert_extra_project, upload_drive_file};
use axum::http::StatusCode;
use chrono::Utc;
use entity::{
    drive_file_sections, drive_files, drive_folder_shares, drive_folders, project_members,
    project_statuses, projects, scopes::Scope, task_attachments, tasks,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QuerySelect, TransactionTrait,
};
use service::drive_sections::{BackfillReport, MAX_INDEXED_BYTES, backfill_missing};
use uuid::Uuid;

async fn insert_folder(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Option<Uuid>,
    parent_id: Option<Uuid>,
    created_by: Uuid,
) -> Uuid {
    let folder_id = Uuid::new_v4();
    drive_folders::ActiveModel {
        id: Set(folder_id),
        name: Set(format!("folder-{folder_id}")),
        parent_id: Set(parent_id),
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

async fn login(app: &mut TestApp, user: &TestUser) {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
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

fn search_url(app: &TestApp, tenant_id: Uuid, project_id: Uuid, params: &[(&str, &str)]) -> String {
    let query: Vec<String> = params
        .iter()
        .map(|(name, value)| format!("{name}={}", urlencoding::encode(value)))
        .collect();
    format!(
        "{}/v1/tenants/{tenant_id}/projects/{project_id}/documents/search?{}",
        app.base_url(),
        query.join("&")
    )
}

/// ログイン中のセッションで検索する。
async fn search(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    params: &[(&str, &str)],
) -> reqwest::Response {
    app.client()
        .get(search_url(app, tenant_id, project_id, params))
        .send()
        .await
        .expect("search request")
}

/// PAT で検索する（セッションの cookie は付けない）。
async fn search_with_bearer(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    token: &str,
    q: &str,
) -> reqwest::Response {
    reqwest::Client::new()
        .get(search_url(app, tenant_id, project_id, &[("q", q)]))
        .bearer_auth(token)
        .send()
        .await
        .expect("search request")
}

/// ログイン中のセッションで検索し、`200` の応答の JSON を返す。
async fn search_ok(
    app: &TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    params: &[(&str, &str)],
) -> serde_json::Value {
    let response = search(app, tenant_id, project_id, params).await;
    assert_eq!(response.status(), StatusCode::OK, "検索 {params:?}");
    response.json().await.expect("search json")
}

async fn total(app: &TestApp, tenant_id: Uuid, project_id: Uuid, q: &str) -> u64 {
    search_ok(app, tenant_id, project_id, &[("q", q)]).await["total"]
        .as_u64()
        .expect("total")
}

async fn message_of(response: reqwest::Response) -> String {
    let body: serde_json::Value = response.json().await.expect("error json");
    body["message"].as_str().unwrap_or_default().to_owned()
}

async fn section_count(app: &TestApp, file_id: Uuid) -> u64 {
    drive_file_sections::Entity::find()
        .filter(drive_file_sections::Column::DriveFileId.eq(file_id))
        .count(&app.state.db)
        .await
        .expect("count sections")
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

/// オーナー・テナント・プロジェクトと、そのプロジェクトのルートフォルダを作ってログインする。
async fn setup(app: &mut TestApp) -> (TestUser, Uuid, Uuid, Uuid) {
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    login(app, &owner).await;
    let folder = insert_folder(app, tp.tenant_id, Some(tp.project_id), None, owner.id).await;
    (owner, tp.tenant_id, tp.project_id, folder)
}

const DOC: &str = "前置きの文 prefaceword\n\n# 認証\n\n概要です overviewword\n\n## トークン\n\n更新は Refresh_Token を使う\n\n```sh\n# codeword_in_block\necho 1\n```\n\n## 別の節\n\n関係ない本文\n";

/// 本文の語で節が引ける。見出しの経路・位置・抜粋・ファイルの情報が返る。
#[tokio::test]
#[serial_test::serial(drive)]
async fn search_returns_matching_sections() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;
    let sub = insert_folder(&app, tenant_id, Some(project_id), Some(folder), owner.id).await;

    let file_id = upload_md(&app, tenant_id, Some(folder), DOC).await;
    assert_eq!(section_count(&app, file_id).await, 4);

    // 大文字小文字を無視した部分一致
    let body = search_ok(&app, tenant_id, project_id, &[("q", "refresh_token")]).await;
    assert_eq!(body["total"], 1);
    let hit = &body["hits"][0];
    assert_eq!(hit["file_id"], file_id.to_string());
    assert_eq!(hit["file_name"], "spec.md");
    assert_eq!(hit["folder_id"], folder.to_string());
    assert_eq!(hit["heading_path"], serde_json::json!(["認証", "トークン"]));
    assert_eq!(hit["position"], 2);
    let excerpt = hit["excerpt"].as_str().expect("excerpt");
    assert!(excerpt.contains("更新は Refresh_Token を使う"), "{excerpt}");
    assert!(!excerpt.contains("<em>"), "抜粋は素の文字列");
    assert!(hit["updated_at"].is_string());

    // コードブロックの中の語、日本語の語、最初の見出しより前の節
    let body = search_ok(&app, tenant_id, project_id, &[("q", "codeword_in_block")]).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["hits"][0]["position"], 2);
    let body = search_ok(&app, tenant_id, project_id, &[("q", "概要です")]).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["hits"][0]["heading_path"], serde_json::json!(["認証"]));
    let body = search_ok(&app, tenant_id, project_id, &[("q", "prefaceword")]).await;
    assert_eq!(body["hits"][0]["heading_path"], serde_json::json!([]));
    assert_eq!(body["hits"][0]["position"], 0);

    // サブフォルダのファイルも当たる
    let in_sub = upload_md(&app, tenant_id, Some(sub), "# sub\n\nsubfolderword\n").await;
    let body = search_ok(&app, tenant_id, project_id, &[("q", "subfolderword")]).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["hits"][0]["file_id"], in_sub.to_string());
    assert_eq!(body["hits"][0]["folder_id"], sub.to_string());

    // Markdown 以外は索引されない
    let plain = file_id_of(
        &upload_drive_file(
            &app,
            tenant_id,
            Some(folder),
            "notes.txt",
            "text/plain",
            b"# plain\n\nplainonlyword\n",
        )
        .await,
    );
    assert_eq!(section_count(&app, plain).await, 0);
    assert_eq!(total(&app, tenant_id, project_id, "plainonlyword").await, 0);

    // テナント一般のファイルは索引されるが、プロジェクトの検索には出ない
    let general = upload_md(&app, tenant_id, None, "# general\n\ngeneralonlyword\n").await;
    assert_eq!(section_count(&app, general).await, 1);
    assert_eq!(
        total(&app, tenant_id, project_id, "generalonlyword").await,
        0
    );

    app.cleanup_user(owner.id).await;
}

/// 語は AND で絞る。`%` と `_` は字義どおりに扱う。
#[tokio::test]
#[serial_test::serial(drive)]
async fn search_terms_are_anded_and_wildcards_are_literal() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;
    upload_md(
        &app,
        tenant_id,
        Some(folder),
        "# a\nalphaword betaword\n# b\nalphaword only\n# c\n100% done\n# d\n100x done\n# e\nsnake_case\n# f\nsnakeXcase\n",
    )
    .await;

    assert_eq!(total(&app, tenant_id, project_id, "alphaword").await, 2);
    let both = search_ok(&app, tenant_id, project_id, &[("q", "betaword  alphaword")]).await;
    assert_eq!(both["total"], 1);
    assert_eq!(both["hits"][0]["heading_path"], serde_json::json!(["a"]));

    let percent = search_ok(&app, tenant_id, project_id, &[("q", "100%")]).await;
    assert_eq!(percent["total"], 1);
    assert_eq!(percent["hits"][0]["heading_path"], serde_json::json!(["c"]));
    let underscore = search_ok(&app, tenant_id, project_id, &[("q", "snake_case")]).await;
    assert_eq!(underscore["total"], 1);
    assert_eq!(
        underscore["hits"][0]["heading_path"],
        serde_json::json!(["e"])
    );

    app.cleanup_user(owner.id).await;
}

/// `limit` の既定は 20、上限は 100。`offset` で重複も欠落も無く辿れる。
#[tokio::test]
#[serial_test::serial(drive)]
async fn search_pages_through_hits() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;
    // 上限の 100 を越える節数
    const SECTIONS: usize = 105;
    let body: String = (0..SECTIONS)
        .map(|i| format!("# h{i}\npageword\n"))
        .collect();
    upload_md(&app, tenant_id, Some(folder), &body).await;

    let default = search_ok(&app, tenant_id, project_id, &[("q", "pageword")]).await;
    assert_eq!(default["hits"].as_array().expect("hits").len(), 20);
    assert_eq!(default["total"], SECTIONS);

    let clamped = search_ok(
        &app,
        tenant_id,
        project_id,
        &[("q", "pageword"), ("limit", "101")],
    )
    .await;
    assert_eq!(clamped["hits"].as_array().expect("hits").len(), 100);
    assert_eq!(clamped["total"], SECTIONS);

    let zero = search_ok(
        &app,
        tenant_id,
        project_id,
        &[("q", "pageword"), ("limit", "0")],
    )
    .await;
    assert_eq!(zero["hits"].as_array().expect("hits").len(), 1);

    // offset が末尾を越えても total は全数
    let beyond = search_ok(
        &app,
        tenant_id,
        project_id,
        &[("q", "pageword"), ("offset", "1000")],
    )
    .await;
    assert_eq!(beyond["hits"].as_array().expect("hits").len(), 0);
    assert_eq!(beyond["total"], SECTIONS);

    let mut positions = Vec::new();
    for offset in ["0", "50", "100"] {
        let page = search_ok(
            &app,
            tenant_id,
            project_id,
            &[("q", "pageword"), ("limit", "50"), ("offset", offset)],
        )
        .await;
        positions.extend(
            page["hits"]
                .as_array()
                .expect("hits")
                .iter()
                .map(|hit| hit["position"].as_u64().expect("position")),
        );
    }
    assert_eq!(positions, (0..SECTIONS as u64).collect::<Vec<_>>());

    app.cleanup_user(owner.id).await;
}

async fn first_project_task(app: &TestApp, project_id: Uuid, created_by: Uuid) -> (String, Uuid) {
    let key = projects::Entity::find_by_id(project_id)
        .one(&app.state.db)
        .await
        .expect("load project")
        .expect("project exists")
        .key;
    let status_id = Uuid::new_v4();
    project_statuses::ActiveModel {
        id: Set(status_id),
        project_id: Set(project_id),
        name: Set("Todo".into()),
        color: Set("#336699".into()),
        position: Set(1),
        is_default: Set(true),
        is_done_state: Set(false),
        is_default_done: Set(false),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert status");
    let now = Utc::now();
    let task_id = tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        seq_id: Set(1),
        title: Set("task 1".into()),
        description: Set(None),
        status_id: Set(status_id),
        priority: Set(tasks::TaskPriority::Medium),
        progress_pct: Set(0),
        parent_task_id: Set(None),
        milestone_id: Set(None),
        sprint_id: Set(None),
        soft_deadline: Set(None),
        hard_deadline: Set(None),
        planned_start_on: Set(None),
        require_all_assignees_done: Set(false),
        task_type_id: Set(None),
        estimated_minutes: Set(None),
        is_archived: Set(false),
        created_by: Set(created_by),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        completed_at: Set(None),
        deleted_at: Set(None),
    }
    .insert(&app.state.db)
    .await
    .expect("insert task")
    .id;
    (key, task_id)
}

async fn body_links(app: &TestApp, file_id: Uuid) -> u64 {
    task_attachments::Entity::find()
        .filter(task_attachments::Column::DriveFileId.eq(file_id))
        .count(&app.state.db)
        .await
        .expect("count attachments")
}

/// 本文の更新で索引が作り直される。`write:task` の無い PAT の保存でも索引は更新される
/// （タスクへの結び付けは従来どおり作られない）。
#[tokio::test]
#[serial_test::serial(drive)]
async fn saving_reindexes_even_without_write_task() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;
    let (key, _task) = first_project_task(&app, project_id, owner.id).await;

    // セッションでの本文の更新
    let file_id = upload_md(&app, tenant_id, Some(folder), "# old\n\noldsessionword\n").await;
    assert_eq!(
        total(&app, tenant_id, project_id, "oldsessionword").await,
        1
    );
    let response = app
        .put_json_with_session(
            &format!("/v1/tenants/{tenant_id}/drive/files/{file_id}/content"),
            serde_json::json!({ "content": "# new\n\nnewsessionword\n\n## more\n\ntext\n" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        total(&app, tenant_id, project_id, "oldsessionword").await,
        0
    );
    assert_eq!(
        total(&app, tenant_id, project_id, "newsessionword").await,
        1
    );
    assert_eq!(section_count(&app, file_id).await, 2);

    // write:drive だけの PAT でのアップロードと本文の更新
    let drive_only = app
        .insert_pat(owner.id, tenant_id, vec![Scope::WriteDrive], None)
        .await;
    let form = reqwest::multipart::Form::new()
        .text("folder_id", folder.to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(
                format!("# pat\n\npatuploadword {key}-1\n").into_bytes(),
            )
            .file_name("pat.md")
            .mime_str("text/markdown")
            .expect("mime"),
        );
    let uploaded = reqwest::Client::new()
        .post(format!(
            "{}/v1/tenants/{tenant_id}/drive/files",
            app.base_url()
        ))
        .bearer_auth(&drive_only)
        .multipart(form)
        .send()
        .await
        .expect("upload request");
    assert_eq!(uploaded.status(), StatusCode::CREATED);
    let pat_file = file_id_of(&uploaded.json().await.expect("upload json"));
    assert_eq!(total(&app, tenant_id, project_id, "patuploadword").await, 1);
    assert_eq!(body_links(&app, pat_file).await, 0, "結び付けは作られない");

    let updated = reqwest::Client::new()
        .put(format!(
            "{}/v1/tenants/{tenant_id}/drive/files/{pat_file}/content",
            app.base_url()
        ))
        .bearer_auth(&drive_only)
        .json(&serde_json::json!({ "content": format!("# pat\n\npatupdatedword {key}-1\n") }))
        .send()
        .await
        .expect("put request");
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(total(&app, tenant_id, project_id, "patuploadword").await, 0);
    assert_eq!(
        total(&app, tenant_id, project_id, "patupdatedword").await,
        1
    );
    assert_eq!(body_links(&app, pat_file).await, 0, "結び付けは作られない");

    // 対照: セッション（write:task を含む）での保存は結び付けを作る
    let response = app
        .put_json_with_session(
            &format!("/v1/tenants/{tenant_id}/drive/files/{pat_file}/content"),
            serde_json::json!({ "content": format!("# pat\n\n{key}-1\n") }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_links(&app, pat_file).await, 1);

    app.cleanup_user(owner.id).await;
}

/// NUL を含む Markdown も保存でき、NUL を除いた本文で当たる。
#[tokio::test]
#[serial_test::serial(drive)]
async fn markdown_with_nul_is_saved_and_indexed() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;

    let file_id = upload_md(&app, tenant_id, Some(folder), "# t\n\nnul\0word here\n").await;
    assert_eq!(total(&app, tenant_id, project_id, "nulword").await, 1);

    let response = app
        .put_json_with_session(
            &format!("/v1/tenants/{tenant_id}/drive/files/{file_id}/content"),
            serde_json::json!({ "content": "# t\n\nput\0nul here\n" }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(total(&app, tenant_id, project_id, "putnul").await, 1);

    app.cleanup_user(owner.id).await;
}

async fn patch(app: &TestApp, path: &str, body: serde_json::Value) {
    let response = app.patch_json_with_session(path, body).await;
    assert_eq!(response.status(), StatusCode::OK, "PATCH {path}");
}

/// 移動したファイルは移動先のプロジェクトの検索に出る（本文は読み直さない）。削除で索引も消える。
#[tokio::test]
#[serial_test::serial(drive)]
async fn moved_and_deleted_files_follow_drive_files() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_a, folder_a) = setup(&mut app).await;
    let project_b = insert_extra_project(&app, tenant_id).await;
    let folder_b = insert_folder(&app, tenant_id, Some(project_b), None, owner.id).await;
    let general = insert_folder(&app, tenant_id, None, None, owner.id).await;

    // ファイルの移動
    let file_id = upload_md(&app, tenant_id, Some(folder_a), "# m\n\nmovedfileword\n").await;
    assert_eq!(total(&app, tenant_id, project_a, "movedfileword").await, 1);
    assert_eq!(total(&app, tenant_id, project_b, "movedfileword").await, 0);
    let file_path = format!("/v1/tenants/{tenant_id}/drive/files/{file_id}");
    patch(
        &app,
        &file_path,
        serde_json::json!({ "folder_id": folder_b }),
    )
    .await;
    assert_eq!(total(&app, tenant_id, project_a, "movedfileword").await, 0);
    assert_eq!(total(&app, tenant_id, project_b, "movedfileword").await, 1);

    // フォルダごとの移動
    let sub = insert_folder(&app, tenant_id, Some(project_a), Some(folder_a), owner.id).await;
    upload_md(&app, tenant_id, Some(sub), "# s\n\nmovedfolderword\n").await;
    assert_eq!(
        total(&app, tenant_id, project_a, "movedfolderword").await,
        1
    );
    patch(
        &app,
        &format!("/v1/tenants/{tenant_id}/drive/folders/{sub}"),
        serde_json::json!({ "parent_id": folder_b }),
    )
    .await;
    assert_eq!(
        total(&app, tenant_id, project_a, "movedfolderword").await,
        0
    );
    assert_eq!(
        total(&app, tenant_id, project_b, "movedfolderword").await,
        1
    );

    // テナント一般のフォルダへ移すと、どのプロジェクトの検索にも出ない
    patch(
        &app,
        &file_path,
        serde_json::json!({ "folder_id": general }),
    )
    .await;
    assert_eq!(total(&app, tenant_id, project_a, "movedfileword").await, 0);
    assert_eq!(total(&app, tenant_id, project_b, "movedfileword").await, 0);
    assert_eq!(section_count(&app, file_id).await, 1, "索引は残る");

    // 削除
    patch(
        &app,
        &file_path,
        serde_json::json!({ "folder_id": folder_a }),
    )
    .await;
    assert_eq!(total(&app, tenant_id, project_a, "movedfileword").await, 1);
    let deleted = app.delete_with_session(&file_path).await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(total(&app, tenant_id, project_a, "movedfileword").await, 0);
    assert_eq!(section_count(&app, file_id).await, 0);

    app.cleanup_user(owner.id).await;
}

/// プロジェクトに入れる人だけが検索できる。
#[tokio::test]
#[serial_test::serial(drive)]
async fn search_requires_project_access() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;
    upload_md(&app, tenant_id, Some(folder), "# a\n\nauthzword\n").await;
    // 客分の対照に使うタスク検索が引けるよう、ステータスとタスクを置く
    first_project_task(&app, project_id, owner.id).await;

    let member = app.insert_user(false, false).await;
    let non_member = app.insert_user(false, false).await;
    let outsider = app.insert_user(false, false).await;
    let guest = app.insert_user(false, false).await;
    // 非公開にする前に、メンバーでないテナントメンバーを足す
    crate::common::ensure_tenant_member_for_project(&app.state.db, project_id, non_member.id).await;
    restrict_to(&app, project_id, member.id).await;
    // テナントに所属せず、プロジェクトにだけ明示指定された利用者
    project_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project_id),
        user_id: Set(guest.id),
        role: Set(project_members::ProjectRole::Member),
    }
    .insert(&app.state.db)
    .await
    .expect("insert guest project member");

    let scopes = vec![Scope::ReadDrive, Scope::ReadTask];
    let pat = |user_id: Uuid| app.insert_pat(user_id, tenant_id, scopes.clone(), None);

    // メンバーとテナントオーナー（メンバーではない）は通る
    let response = search_with_bearer(
        &app,
        tenant_id,
        project_id,
        &pat(member.id).await,
        "authzword",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json");
    assert_eq!(body["total"], 1);
    let response = search_with_bearer(
        &app,
        tenant_id,
        project_id,
        &pat(owner.id).await,
        "authzword",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // 非公開プロジェクトのメンバーでないテナントメンバー
    let response = search_with_bearer(
        &app,
        tenant_id,
        project_id,
        &pat(non_member.id).await,
        "authzword",
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(message_of(response).await, "forbidden");

    // 別テナントの利用者
    let response = search_with_bearer(
        &app,
        tenant_id,
        project_id,
        &pat(outsider.id).await,
        "authzword",
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(message_of(response).await, "tenant-membership-missing");

    // プロジェクトにだけ明示指定された利用者。タスクの検索は通るが、本文の検索は通さない
    let guest_pat = pat(guest.id).await;
    let tasks = reqwest::Client::new()
        .get(format!(
            "{}/v1/tenants/{tenant_id}/projects/{project_id}/tasks/search?q=task",
            app.base_url()
        ))
        .bearer_auth(&guest_pat)
        .send()
        .await
        .expect("task search request");
    assert_eq!(tasks.status(), StatusCode::OK, "対照: タスクの検索は通る");
    let response = search_with_bearer(&app, tenant_id, project_id, &guest_pat, "authzword").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(message_of(response).await, "forbidden");

    // 未認証
    let anonymous = reqwest::Client::new()
        .get(search_url(
            &app,
            tenant_id,
            project_id,
            &[("q", "authzword")],
        ))
        .send()
        .await
        .expect("anonymous request");
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    for user in [&member, &non_member, &outsider, &guest] {
        app.cleanup_user(user.id).await;
    }
    app.cleanup_user(owner.id).await;
}

/// PAT の束縛（プロジェクト・テナント）と scope で絞る。
#[tokio::test]
#[serial_test::serial(drive)]
async fn search_honors_pat_binding_and_scope() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_a, folder_a) = setup(&mut app).await;
    let project_b = insert_extra_project(&app, tenant_id).await;
    let folder_b = insert_folder(&app, tenant_id, Some(project_b), None, owner.id).await;
    let other = app.insert_tenant_project(owner.id).await;
    upload_md(&app, tenant_id, Some(folder_a), "# a\n\npatword\n").await;
    upload_md(&app, tenant_id, Some(folder_b), "# b\n\npatword\n").await;

    // プロジェクト A に束縛した PAT
    let bound = app
        .insert_pat(
            owner.id,
            tenant_id,
            vec![Scope::ReadDrive],
            Some(vec![project_a]),
        )
        .await;
    let response = search_with_bearer(&app, tenant_id, project_a, &bound, "patword").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json");
    assert_eq!(body["total"], 1, "束縛したプロジェクトの節だけ");
    let response = search_with_bearer(&app, tenant_id, project_b, &bound, "patword").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // 別テナントに束縛した PAT
    let other_tenant = app
        .insert_pat(owner.id, other.tenant_id, vec![Scope::ReadDrive], None)
        .await;
    let response = search_with_bearer(&app, tenant_id, project_a, &other_tenant, "patword").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // scope。read:drive だけで足り、read:task だけでは足りない
    let drive = app
        .insert_pat(owner.id, tenant_id, vec![Scope::ReadDrive], None)
        .await;
    let response = search_with_bearer(&app, tenant_id, project_a, &drive, "patword").await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = search_with_bearer(&app, tenant_id, project_b, &drive, "patword").await;
    assert_eq!(response.status(), StatusCode::OK);
    let task_only = app
        .insert_pat(owner.id, tenant_id, vec![Scope::ReadTask], None)
        .await;
    let response = search_with_bearer(&app, tenant_id, project_a, &task_only, "patword").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    app.cleanup_user(owner.id).await;
}

/// フォルダを利用者指定で共有された人は、本文は読めても検索はできない。
#[tokio::test]
#[serial_test::serial(drive)]
async fn user_share_does_not_grant_search() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;
    let file_id = upload_md(&app, tenant_id, Some(folder), "# a\n\nsharedword\n").await;

    let member = app.insert_user(false, false).await;
    let shared = app.insert_user(false, false).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, project_id, shared.id).await;
    restrict_to(&app, project_id, member.id).await;
    drive_folder_shares::ActiveModel {
        id: Set(Uuid::new_v4()),
        folder_id: Set(folder),
        shared_with_user_id: Set(Some(shared.id)),
        share_token: Set(None),
        permission: Set(drive_folder_shares::SharePermission::Viewer),
        created_by: Set(owner.id),
        expires_at: Set(None),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert user share");

    let token = app
        .insert_pat(shared.id, tenant_id, vec![Scope::ReadDrive], None)
        .await;
    let content = reqwest::Client::new()
        .get(format!(
            "{}/v1/drive/files/{file_id}/content",
            app.base_url()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .expect("content request");
    assert_eq!(content.status(), StatusCode::OK, "対照: 共有で本文は読める");
    let response = search_with_bearer(&app, tenant_id, project_id, &token, "sharedword").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    app.cleanup_user(member.id).await;
    app.cleanup_user(shared.id).await;
    app.cleanup_user(owner.id).await;
}

/// `q` の規則と、存在しないプロジェクト。
#[tokio::test]
#[serial_test::serial(drive)]
async fn search_validates_query_and_project() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, _folder) = setup(&mut app).await;
    let other = app.insert_tenant_project(owner.id).await;

    let status = |q: String| {
        let app = &app;
        async move {
            search(app, tenant_id, project_id, &[("q", q.as_str())])
                .await
                .status()
        }
    };
    assert_eq!(status(String::new()).await, StatusCode::BAD_REQUEST);
    assert_eq!(status("  \t ".into()).await, StatusCode::BAD_REQUEST);
    // 文字数の境界（文字で数える。バイトではない）
    assert_eq!(status("あ".repeat(255)).await, StatusCode::OK);
    assert_eq!(status("あ".repeat(256)).await, StatusCode::BAD_REQUEST);
    // 前後の空白は数えない
    assert_eq!(
        status(format!("  {}  ", "a".repeat(255))).await,
        StatusCode::OK
    );
    // 語数の境界
    assert_eq!(status(["w"; 10].join(" ")).await, StatusCode::OK);
    assert_eq!(status(["w"; 11].join(" ")).await, StatusCode::BAD_REQUEST);
    // NUL は DB のエラー（500）にせず弾く
    assert_eq!(status("a\0b".into()).await, StatusCode::BAD_REQUEST);
    // q が無い
    let missing = search(&app, tenant_id, project_id, &[]).await;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

    // 存在しないプロジェクトと、別テナントのプロジェクト
    let response = search(&app, tenant_id, Uuid::new_v4(), &[("q", "x")]).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = search(&app, tenant_id, other.project_id, &[("q", "x")]).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    // 対照: そのテナントのプロジェクトなら通る
    let response = search(&app, other.tenant_id, other.project_id, &[("q", "x")]).await;
    assert_eq!(response.status(), StatusCode::OK);

    app.cleanup_user(owner.id).await;
}

/// backfill はテナントを跨いで全行を見るので、Drive を空にしてから始める。
async fn app_with_empty_drive() -> TestApp {
    let app = TestApp::new().await;
    app.state
        .db
        .execute_unprepared("TRUNCATE TABLE drive_files, drive_folders CASCADE")
        .await
        .expect("truncate drive tables");
    app
}

async fn run_backfill(app: &TestApp) -> BackfillReport {
    backfill_missing(&app.state.db, app.state.storage.as_ref())
        .await
        .expect("backfill")
}

/// 索引の無い Markdown ファイルを、ストレージの本文から索引する。
#[tokio::test]
#[serial_test::serial(drive)]
async fn backfill_indexes_files_without_sections() {
    let mut app = app_with_empty_drive().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;

    // 1 回の取得数（100）を越える件数
    const FILES: u64 = 101;
    let mut file_ids = Vec::new();
    for i in 0..FILES {
        let body = format!("# doc {i}\n\nbackfillword\n\n## sub\n\ntext\n");
        file_ids.push(upload_md(&app, tenant_id, Some(folder), &body).await);
    }
    // Markdown 以外は対象にならない
    upload_drive_file(
        &app,
        tenant_id,
        Some(folder),
        "n.txt",
        "text/plain",
        b"backfillword",
    )
    .await;
    // ストレージに実体の無いファイル
    let broken = upload_md(&app, tenant_id, Some(folder), "# broken\n\nbackfillword\n").await;
    drive_files::Entity::update_many()
        .col_expr(
            drive_files::Column::StorageKey,
            sea_orm::sea_query::Expr::value(Uuid::new_v4().to_string()),
        )
        .filter(drive_files::Column::Id.eq(broken))
        .exec(&app.state.db)
        .await
        .expect("break storage key");
    // 索引の無い状態（この変更より前に上げたファイル）にする
    drive_file_sections::Entity::delete_many()
        .exec(&app.state.db)
        .await
        .expect("delete sections");
    assert_eq!(total(&app, tenant_id, project_id, "backfillword").await, 0);

    let report = run_backfill(&app).await;
    assert_eq!(
        report,
        BackfillReport {
            indexed: FILES,
            skipped: 0,
            failed: 1
        }
    );
    assert_eq!(
        total(&app, tenant_id, project_id, "backfillword").await,
        FILES
    );
    for file_id in &file_ids {
        assert_eq!(section_count(&app, *file_id).await, 2);
    }
    assert_eq!(section_count(&app, broken).await, 0);

    // 冪等。失敗したファイルだけをもう一度試す
    let again = run_backfill(&app).await;
    assert_eq!(
        again,
        BackfillReport {
            indexed: 0,
            skipped: 0,
            failed: 1
        }
    );
    assert_eq!(
        total(&app, tenant_id, project_id, "backfillword").await,
        FILES
    );

    app.cleanup_user(owner.id).await;
}

/// 本文を読んだ後に本文が更新されたファイルは、古い本文で索引しない。
#[tokio::test]
#[serial_test::serial(drive)]
async fn backfill_skips_a_file_updated_after_it_was_read() {
    let mut app = app_with_empty_drive().await;
    let (owner, tenant_id, _project_id, folder) = setup(&mut app).await;
    let file_id = upload_md(&app, tenant_id, Some(folder), "# old\n\nstaleword\n").await;
    drive_file_sections::Entity::delete_many()
        .exec(&app.state.db)
        .await
        .expect("delete sections");

    // 本文の更新と同じ行ロックを先に取る。backfill は本文を読んだ後、このロックを待つ
    let txn = app.state.db.begin().await.expect("begin");
    drive_files::Entity::find_by_id(file_id)
        .lock_exclusive()
        .one(&txn)
        .await
        .expect("lock file")
        .expect("file exists");
    let db = app.state.db.clone();
    let storage = app.state.storage.clone();
    let backfill = tokio::spawn(async move { backfill_missing(&db, storage.as_ref()).await });

    // backfill がロック待ちに入るまで待つ
    let mut waiting = false;
    for _ in 0..200 {
        let blocked: Option<i64> = drive_files::Entity::find()
            .select_only()
            .column_as(
                sea_orm::sea_query::Expr::cust("(SELECT COUNT(*) FROM pg_locks WHERE NOT granted)"),
                "blocked",
            )
            .into_tuple()
            .one(&app.state.db)
            .await
            .expect("read pg_locks");
        if blocked.unwrap_or(0) > 0 {
            waiting = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(waiting, "backfill が行ロックを待っている");

    // 本文の更新が済んだ形にする（ストレージキーが変わる）
    drive_files::Entity::update_many()
        .col_expr(
            drive_files::Column::StorageKey,
            sea_orm::sea_query::Expr::value(Uuid::new_v4().to_string()),
        )
        .filter(drive_files::Column::Id.eq(file_id))
        .exec(&txn)
        .await
        .expect("replace storage key");
    txn.commit().await.expect("commit");

    let report = backfill.await.expect("join").expect("backfill");
    assert_eq!(
        report,
        BackfillReport {
            indexed: 0,
            skipped: 1,
            failed: 0
        }
    );
    assert_eq!(
        section_count(&app, file_id).await,
        0,
        "古い本文で索引しない"
    );

    app.cleanup_user(owner.id).await;
}

/// 節の本文（位置の順）。
async fn section_rows(app: &TestApp, file_id: Uuid) -> Vec<(i32, Vec<String>, String)> {
    use sea_orm::QueryOrder;
    drive_file_sections::Entity::find()
        .filter(drive_file_sections::Column::DriveFileId.eq(file_id))
        .order_by_asc(drive_file_sections::Column::Position)
        .all(&app.state.db)
        .await
        .expect("load sections")
        .into_iter()
        .map(|row| (row.position, row.heading_path, row.body))
        .collect()
}

/// NUL の多い本文でも、backfill は保存の経路と同じ索引を作る
/// （NUL を除いた後の長さで上限を数える。生の長さで読むのを止めない）。
#[tokio::test]
#[serial_test::serial(drive)]
async fn backfill_matches_the_save_path_for_a_body_full_of_nul() {
    let mut app = app_with_empty_drive().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;
    // 生の長さでは索引の上限の 2 倍ある NUL の前置き。上限をわずかに越えるだけでは、
    // ストレージが小分けに返す最後のかたまりに本文が入り、生の長さで止めても読めてしまう
    let body = format!(
        "{}# Title\n\nnulprefixword\n",
        "\0".repeat(MAX_INDEXED_BYTES * 2)
    );
    let file_id = upload_md(&app, tenant_id, Some(folder), &body).await;
    let saved = section_rows(&app, file_id).await;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].2, "# Title\n\nnulprefixword\n");
    assert_eq!(total(&app, tenant_id, project_id, "nulprefixword").await, 1);

    drive_file_sections::Entity::delete_many()
        .exec(&app.state.db)
        .await
        .expect("delete sections");
    let report = run_backfill(&app).await;
    assert_eq!(
        report,
        BackfillReport {
            indexed: 1,
            skipped: 0,
            failed: 0
        }
    );
    assert_eq!(section_rows(&app, file_id).await, saved);
    assert_eq!(total(&app, tenant_id, project_id, "nulprefixword").await, 1);

    app.cleanup_user(owner.id).await;
}

/// 1 回の INSERT の行数（500）と節の上限（2000）を越える本文が、欠けも重なりも無く入る。
#[tokio::test]
#[serial_test::serial(drive)]
async fn large_file_is_indexed_across_insert_chunks_and_capped() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;
    const HEADINGS: usize = 2001;
    let body: String = (0..HEADINGS)
        .map(|i| format!("# h{i}\n\nchunkword{i}x\n\n"))
        .collect();
    let file_id = upload_md(&app, tenant_id, Some(folder), &body).await;

    let rows = section_rows(&app, file_id).await;
    assert_eq!(rows.len(), 2000);
    assert_eq!(
        rows.iter().map(|row| row.0).collect::<Vec<_>>(),
        (0..2000).collect::<Vec<_>>(),
        "位置は 0 から続く"
    );
    let joined: String = rows.iter().map(|row| row.2.as_str()).collect();
    assert_eq!(joined, body, "節をつなぐと本文に戻る");

    // INSERT の区切り（500 行）の前後の節
    for i in [499usize, 500, 501, 1999] {
        let hit = search_ok(
            &app,
            tenant_id,
            project_id,
            &[("q", &format!("chunkword{i}x"))],
        )
        .await;
        assert_eq!(hit["total"], 1, "chunkword{i}x");
        assert_eq!(hit["hits"][0]["position"], i);
        assert_eq!(
            hit["hits"][0]["heading_path"],
            serde_json::json!([format!("h{i}")])
        );
    }
    // 上限を越えた見出しは最後の節に入る
    let last = search_ok(&app, tenant_id, project_id, &[("q", "chunkword2000x")]).await;
    assert_eq!(last["total"], 1);
    assert_eq!(last["hits"][0]["position"], 1999);
    assert_eq!(
        last["hits"][0]["heading_path"],
        serde_json::json!(["h1999"])
    );

    app.cleanup_user(owner.id).await;
}

/// 抜粋は長い節の途中からでも、語の前後 100 文字で切れる（節の本文の全体は受け取らない）。
#[tokio::test]
#[serial_test::serial(drive)]
async fn excerpt_is_cut_from_the_middle_of_a_long_section() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;
    let body = format!(
        "{} DeepNeedle 日本語の語 {}",
        "あ".repeat(5000),
        "い".repeat(5000)
    );
    upload_md(&app, tenant_id, Some(folder), &body).await;

    let expected = format!(
        "…{} DeepNeedle 日本語の語 {}…",
        "あ".repeat(99),
        "い".repeat(100 - " 日本語の語 ".chars().count())
    );
    // 大文字小文字が違っても同じ位置で切れる
    for q in ["DeepNeedle", "deepneedle"] {
        let hit = search_ok(&app, tenant_id, project_id, &[("q", q)]).await;
        assert_eq!(hit["total"], 1);
        assert_eq!(hit["hits"][0]["excerpt"], expected, "{q}");
    }
    // 上限の長さ（255 文字）の語でも、語と前後 100 文字が抜粋に収まる
    let long_term = "z".repeat(255);
    upload_md(
        &app,
        tenant_id,
        Some(folder),
        &format!("{}{long_term}{}", "か".repeat(3000), "き".repeat(3000)),
    )
    .await;
    let hit = search_ok(&app, tenant_id, project_id, &[("q", &long_term)]).await;
    assert_eq!(hit["total"], 1);
    assert_eq!(
        hit["hits"][0]["excerpt"],
        format!("…{}{long_term}{}…", "か".repeat(100), "き".repeat(100))
    );

    // 2 つ目の語が先に現れても、1 つ目の語のまわりで切る
    let hit = search_ok(
        &app,
        tenant_id,
        project_id,
        &[("q", "日本語の語 deepneedle")],
    )
    .await;
    let excerpt = hit["hits"][0]["excerpt"].as_str().expect("excerpt");
    assert!(excerpt.starts_with('…') && excerpt.ends_with('…'));
    assert_eq!(
        excerpt.chars().count(),
        1 + 100 + "日本語の語".chars().count() + 100 + 1
    );
    assert!(excerpt.contains("DeepNeedle 日本語の語"));

    app.cleanup_user(owner.id).await;
}

/// 公開リンクのトークンでは検索できない（検索はトークンを受け取らず、未認証として扱う）。
#[tokio::test]
#[serial_test::serial(drive)]
async fn public_link_token_does_not_grant_search() {
    let mut app = TestApp::new().await;
    let (owner, tenant_id, project_id, folder) = setup(&mut app).await;
    let file_id = upload_md(&app, tenant_id, Some(folder), "# a\n\ntokenword\n").await;
    let token = format!("tok-{}", Uuid::new_v4().simple());
    drive_folder_shares::ActiveModel {
        id: Set(Uuid::new_v4()),
        folder_id: Set(folder),
        shared_with_user_id: Set(None),
        share_token: Set(Some(token.clone())),
        permission: Set(drive_folder_shares::SharePermission::Viewer),
        created_by: Set(owner.id),
        expires_at: Set(None),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert token share");

    let anonymous = reqwest::Client::new();
    let content = anonymous
        .get(format!(
            "{}/v1/drive/files/{file_id}/content?token={token}",
            app.base_url()
        ))
        .send()
        .await
        .expect("content request");
    assert_eq!(
        content.status(),
        StatusCode::OK,
        "対照: トークンで本文は読める"
    );
    let response = anonymous
        .get(search_url(
            &app,
            tenant_id,
            project_id,
            &[("q", "tokenword"), ("token", &token)],
        ))
        .send()
        .await
        .expect("search request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    app.cleanup_user(owner.id).await;
}
