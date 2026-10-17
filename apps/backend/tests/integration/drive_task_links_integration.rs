//! Drive の Markdown 本文の `KEY-N` を task_attachments（source = body）へ結び付ける
//! （TASK-277、docs/features/drive.md §8.5）。
//!
//! Drive の行を作るので、他の Drive テストと同じ鍵で直列化する
//! （drive_file_content_integration.rs 冒頭のコメント参照）。

use std::collections::HashMap;

use crate::common::{
    TestApp, TestUser, file_id_of, insert_extra_project, upload_drive_file as upload_file,
};
use axum::http::StatusCode;
use chrono::Utc;
use entity::{
    drive_folders, project_members, project_statuses, projects, scopes::Scope, task_attachments,
    tasks,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

struct Project {
    id: Uuid,
    key: String,
    status_id: Uuid,
}

async fn project(app: &TestApp, project_id: Uuid) -> Project {
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
    Project {
        id: project_id,
        key,
        status_id,
    }
}

async fn insert_task(app: &TestApp, project: &Project, created_by: Uuid, seq_id: i32) -> Uuid {
    let now = Utc::now();
    tasks::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(project.id),
        seq_id: Set(seq_id),
        title: Set(format!("task {seq_id}")),
        description: Set(None),
        status_id: Set(project.status_id),
        priority: Set(tasks::TaskPriority::Medium),
        progress_pct: Set(0),
        parent_task_id: Set(None),
        milestone_id: Set(None),
        sprint_id: Set(None),
        soft_deadline: Set(None),
        hard_deadline: Set(None),
        planned_start_on: Set(None),
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
    .id
}

/// 指定ユーザーだけをメンバーにして、プロジェクトをテナント全体から閉じる。
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
}

async fn login(app: &mut TestApp, user: &TestUser) {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
}

/// ファイルに結び付いた行（task_id → source）
async fn links(app: &TestApp, file_id: Uuid) -> HashMap<Uuid, String> {
    task_attachments::Entity::find()
        .filter(task_attachments::Column::DriveFileId.eq(file_id))
        .all(&app.state.db)
        .await
        .expect("load attachments")
        .into_iter()
        .map(|row| (row.task_id, row.source))
        .collect()
}

async fn attachment_id(app: &TestApp, task_id: Uuid, file_id: Uuid) -> Uuid {
    task_attachments::Entity::find()
        .filter(task_attachments::Column::TaskId.eq(task_id))
        .filter(task_attachments::Column::DriveFileId.eq(file_id))
        .one(&app.state.db)
        .await
        .expect("load attachment")
        .expect("attachment exists")
        .id
}

async fn put_content(app: &TestApp, tenant_id: Uuid, file_id: Uuid, content: &str) {
    let response = app
        .put_json_with_session(
            &format!("/v1/tenants/{tenant_id}/drive/files/{file_id}/content"),
            serde_json::json!({ "content": content }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK, "本文更新は成功するはず");
}

fn attachments_path(tenant_id: Uuid, project_id: Uuid, task_id: Uuid) -> String {
    format!("/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{task_id}/attachments")
}

fn tasks_path(tenant_id: Uuid, file_id: Uuid) -> String {
    format!("/v1/tenants/{tenant_id}/drive/files/{file_id}/tasks")
}

fn task_ids_of(body: &serde_json::Value) -> Vec<Uuid> {
    body["tasks"]
        .as_array()
        .expect("tasks array")
        .iter()
        .map(|t| {
            t["task_id"]
                .as_str()
                .expect("task_id")
                .parse()
                .expect("uuid")
        })
        .collect()
}

/// アップロードで body が付き、本文更新で付け直される。manual は本文に負けず、
/// 手動添付で body が manual に昇格する。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn body_links_follow_content_and_never_override_manual() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let p = project(&app, tp.project_id).await;
    let t1 = insert_task(&app, &p, owner.id, 1).await;
    let t2 = insert_task(&app, &p, owner.id, 2).await;
    let t3 = insert_task(&app, &p, owner.id, 3).await;
    login(&mut app, &owner).await;
    let key = &p.key;

    let uploaded = upload_file(
        &app,
        tp.tenant_id,
        None,
        "spec.md",
        "text/markdown",
        format!("# 設計\n{key}-1 と {key}-2 を参照").as_bytes(),
    )
    .await;
    let file_id = file_id_of(&uploaded);
    assert_eq!(
        links(&app, file_id).await,
        HashMap::from([(t1, "body".into()), (t2, "body".into())]),
        "アップロードで本文の参照が body で付く"
    );

    // 手動添付: 新規は 201 manual、body の行は 200 で manual へ昇格
    let attach = app
        .post_json_with_session(
            &attachments_path(tp.tenant_id, p.id, t3),
            serde_json::json!({ "drive_file_id": file_id }),
        )
        .await;
    assert_eq!(attach.status(), StatusCode::CREATED);
    let attach: serde_json::Value = attach.json().await.expect("attach json");
    assert_eq!(attach["source"], "manual");
    let promote = app
        .post_json_with_session(
            &attachments_path(tp.tenant_id, p.id, t1),
            serde_json::json!({ "drive_file_id": file_id }),
        )
        .await;
    assert_eq!(promote.status(), StatusCode::OK, "body の行は昇格する");
    let promote: serde_json::Value = promote.json().await.expect("promote json");
    assert_eq!(promote["source"], "manual");
    let again = app
        .post_json_with_session(
            &attachments_path(tp.tenant_id, p.id, t1),
            serde_json::json!({ "drive_file_id": file_id }),
        )
        .await;
    assert_eq!(
        again.status(),
        StatusCode::CONFLICT,
        "manual の二重添付は 409"
    );

    // t3 を本文に書いても manual のまま。t1・t2 を消すと body の t2 だけ消える
    put_content(&app, tp.tenant_id, file_id, &format!("{key}-3 だけ")).await;
    assert_eq!(
        links(&app, file_id).await,
        HashMap::from([(t1, "manual".into()), (t3, "manual".into())]),
        "body は消え、manual は残り、manual を body に書き換えない"
    );

    // 一覧 API にも source が載る
    let list = app
        .get_with_session(&attachments_path(tp.tenant_id, p.id, t1))
        .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list: serde_json::Value = list.json().await.expect("list json");
    assert_eq!(list["attachments"][0]["source"], "manual");

    put_content(&app, tp.tenant_id, file_id, "").await;
    assert_eq!(
        links(&app, file_id).await.len(),
        2,
        "manual は本文に依らない"
    );

    // 消えた body の参照は、本文に書き戻せば付き直す
    put_content(&app, tp.tenant_id, file_id, &format!("{key}-2")).await;
    assert_eq!(
        links(&app, file_id).await.get(&t2).map(String::as_str),
        Some("body")
    );

    app.cleanup_user(owner.id).await;
}

/// body の解除は 409、manual は 204。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn detach_rejects_body_links() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let p = project(&app, tp.project_id).await;
    let t1 = insert_task(&app, &p, owner.id, 1).await;
    let t2 = insert_task(&app, &p, owner.id, 2).await;
    login(&mut app, &owner).await;

    let uploaded = upload_file(
        &app,
        tp.tenant_id,
        None,
        "spec.md",
        "text/markdown",
        format!("{}-1", p.key).as_bytes(),
    )
    .await;
    let file_id = file_id_of(&uploaded);
    let attach = app
        .post_json_with_session(
            &attachments_path(tp.tenant_id, p.id, t2),
            serde_json::json!({ "drive_file_id": file_id }),
        )
        .await;
    assert_eq!(attach.status(), StatusCode::CREATED);

    let body_id = attachment_id(&app, t1, file_id).await;
    let rejected = app
        .delete_with_session(&format!(
            "{}/{body_id}",
            attachments_path(tp.tenant_id, p.id, t1)
        ))
        .await;
    assert_eq!(rejected.status(), StatusCode::CONFLICT);
    let error: serde_json::Value = rejected.json().await.expect("error json");
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|d| d.contains("本文を編集")),
        "本文を直すよう案内する: {error}"
    );

    let manual_id = attachment_id(&app, t2, file_id).await;
    let detached = app
        .delete_with_session(&format!(
            "{}/{manual_id}",
            attachments_path(tp.tenant_id, p.id, t2)
        ))
        .await;
    assert_eq!(detached.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        links(&app, file_id).await,
        HashMap::from([(t1, "body".into())])
    );

    app.cleanup_user(owner.id).await;
}

/// コードブロック・インラインコードの中の参照、text/plain のファイルは結び付かない。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn code_and_non_markdown_are_ignored() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let p = project(&app, tp.project_id).await;
    let t1 = insert_task(&app, &p, owner.id, 1).await;
    insert_task(&app, &p, owner.id, 2).await;
    insert_task(&app, &p, owner.id, 3).await;
    login(&mut app, &owner).await;
    let key = &p.key;

    let md = upload_file(
        &app,
        tp.tenant_id,
        None,
        "code.md",
        "text/markdown",
        format!("```\n{key}-2\n```\n`{key}-3` は例、{key}-1 は本物").as_bytes(),
    )
    .await;
    assert_eq!(
        links(&app, file_id_of(&md)).await,
        HashMap::from([(t1, "body".into())]),
        "コードの中は拾わない"
    );

    let text = upload_file(
        &app,
        tp.tenant_id,
        None,
        "notes.txt",
        "text/plain",
        format!("{key}-1").as_bytes(),
    )
    .await;
    let text_id = file_id_of(&text);
    assert!(
        links(&app, text_id).await.is_empty(),
        "アップロードでは走らない"
    );
    put_content(&app, tp.tenant_id, text_id, &format!("{key}-2")).await;
    assert!(
        links(&app, text_id).await.is_empty(),
        "本文更新でも走らない"
    );

    app.cleanup_user(owner.id).await;
}

/// 保存した利用者が入れないプロジェクトのタスクと、別テナントのタスクは結び付かない。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn only_tasks_visible_to_the_saver_are_linked() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let member = app.insert_user(false, false).await;
    let outsider = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    let open = project(&app, tp.project_id).await;
    let closed = project(&app, insert_extra_project(&app, tp.tenant_id).await).await;
    restrict_to(&app, closed.id, outsider.id).await;
    let open_task = insert_task(&app, &open, owner.id, 1).await;
    let closed_task = insert_task(&app, &closed, owner.id, 1).await;

    let other_owner = app.insert_user(false, false).await;
    let other_tp = app.insert_tenant_project(other_owner.id).await;
    let other = project(&app, other_tp.project_id).await;
    let other_task = insert_task(&app, &other, other_owner.id, 1).await;

    let body = format!("{}-1 {}-1 {}-1", open.key, closed.key, other.key);
    login(&mut app, &member).await;
    let by_member = upload_file(
        &app,
        tp.tenant_id,
        None,
        "member.md",
        "text/markdown",
        body.as_bytes(),
    )
    .await;
    assert_eq!(
        links(&app, file_id_of(&by_member)).await,
        HashMap::from([(open_task, "body".into())]),
        "入れないプロジェクトと別テナントのタスクは結ばない"
    );

    // 対照: オーナーは閉じたプロジェクトにも入れる（別テナントは結ばない）
    login(&mut app, &owner).await;
    let by_owner = upload_file(
        &app,
        tp.tenant_id,
        None,
        "owner.md",
        "text/markdown",
        body.as_bytes(),
    )
    .await;
    let owner_links = links(&app, file_id_of(&by_owner)).await;
    assert_eq!(
        owner_links,
        HashMap::from([(open_task, "body".into()), (closed_task, "body".into())])
    );
    assert!(!owner_links.contains_key(&other_task));

    for user in [owner, member, outsider, other_owner] {
        app.cleanup_user(user.id).await;
    }
}

/// GET /drive/files/{id}/tasks はファイルの閲覧権限を確かめ、入れないプロジェクトのタスクを隠す。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn body_links_to_projects_the_saver_cannot_see_are_kept() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let member = app.insert_user(false, false).await;
    let outsider = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    let open = project(&app, tp.project_id).await;
    let closed = project(&app, insert_extra_project(&app, tp.tenant_id).await).await;
    restrict_to(&app, closed.id, outsider.id).await;
    let open_task = insert_task(&app, &open, owner.id, 1).await;
    let open_task2 = insert_task(&app, &open, owner.id, 2).await;
    let closed_task = insert_task(&app, &closed, owner.id, 1).await;

    // オーナーが両方のプロジェクトのタスクを結ぶ
    login(&mut app, &owner).await;
    let body = format!("{}-1 {}-1", open.key, closed.key);
    let file_id = file_id_of(
        &upload_file(
            &app,
            tp.tenant_id,
            None,
            "shared.md",
            "text/markdown",
            body.as_bytes(),
        )
        .await,
    );
    assert_eq!(
        links(&app, file_id).await,
        HashMap::from([(open_task, "body".into()), (closed_task, "body".into())])
    );

    // 閉じたプロジェクトに入れないメンバーが保存しても、その行は残る。
    // 入れるプロジェクトの行は本文どおりに作り直す
    login(&mut app, &member).await;
    let body = format!("{}-2 {}-1", open.key, closed.key);
    put_content(&app, tp.tenant_id, file_id, &body).await;
    assert_eq!(
        links(&app, file_id).await,
        HashMap::from([(open_task2, "body".into()), (closed_task, "body".into())]),
        "入れないプロジェクトへの body 行は保存者で消えない"
    );

    // 対照: オーナーが本文から外せば消える
    login(&mut app, &owner).await;
    put_content(&app, tp.tenant_id, file_id, &format!("{}-2", open.key)).await;
    assert_eq!(
        links(&app, file_id).await,
        HashMap::from([(open_task2, "body".into())])
    );

    for user in [owner, member, outsider] {
        app.cleanup_user(user.id).await;
    }
}

#[tokio::test]
#[serial_test::file_serial(drive)]
async fn file_tasks_respects_file_and_project_access() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let member = app.insert_user(false, false).await;
    let outsider = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    crate::common::ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    let open = project(&app, tp.project_id).await;
    let closed = project(&app, insert_extra_project(&app, tp.tenant_id).await).await;
    restrict_to(&app, closed.id, outsider.id).await;
    let open_task = insert_task(&app, &open, owner.id, 1).await;
    insert_task(&app, &closed, owner.id, 1).await;

    let closed_folder = Uuid::new_v4();
    drive_folders::ActiveModel {
        id: Set(closed_folder),
        name: Set("closed".into()),
        parent_id: Set(None),
        tenant_id: Set(tp.tenant_id),
        project_id: Set(Some(closed.id)),
        created_by: Set(owner.id),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert folder");

    login(&mut app, &owner).await;
    let body = format!("{}-1 {}-1", open.key, closed.key);
    let shared = file_id_of(
        &upload_file(
            &app,
            tp.tenant_id,
            None,
            "shared.md",
            "text/markdown",
            body.as_bytes(),
        )
        .await,
    );
    let hidden = file_id_of(
        &upload_file(
            &app,
            tp.tenant_id,
            Some(closed_folder),
            "hidden.md",
            "text/markdown",
            body.as_bytes(),
        )
        .await,
    );

    let as_owner = app
        .get_with_session(&tasks_path(tp.tenant_id, shared))
        .await;
    assert_eq!(as_owner.status(), StatusCode::OK);
    let as_owner: serde_json::Value = as_owner.json().await.expect("owner json");
    assert_eq!(
        task_ids_of(&as_owner).len(),
        2,
        "対照: オーナーには両方見える"
    );
    let first = &as_owner["tasks"][0];
    assert_eq!(first["source"], "body");
    assert_eq!(first["status"]["name"], "Todo");
    assert!(first["seq_key"].as_str().is_some_and(|k| k.ends_with("-1")));
    assert!(first["project"]["key"].is_string());
    assert_eq!(
        app.get_with_session(&tasks_path(tp.tenant_id, hidden))
            .await
            .status(),
        StatusCode::OK,
        "対照: オーナーは閉じたプロジェクトのファイルも読める"
    );

    // PAT は read:drive が要る
    let without = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadProject], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&tasks_path(tp.tenant_id, shared), &without)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let with = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadDrive], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&tasks_path(tp.tenant_id, shared), &with)
            .await
            .status(),
        StatusCode::OK
    );

    login(&mut app, &member).await;
    let as_member = app
        .get_with_session(&tasks_path(tp.tenant_id, shared))
        .await;
    assert_eq!(as_member.status(), StatusCode::OK);
    let as_member: serde_json::Value = as_member.json().await.expect("member json");
    assert_eq!(
        task_ids_of(&as_member),
        vec![open_task],
        "入れないプロジェクトのタスクは出ない"
    );
    assert_eq!(
        app.get_with_session(&tasks_path(tp.tenant_id, hidden))
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "閲覧できないファイルは本文取得と同じく拒否"
    );

    for user in [owner, member, outsider] {
        app.cleanup_user(user.id).await;
    }
}

/// 1 ファイルの参照は 200 件で止まる（201 件目は捨てる）。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn links_are_capped_at_200_per_file() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let p = project(&app, tp.project_id).await;
    let mut last = Uuid::nil();
    for seq in 1..=201 {
        last = insert_task(&app, &p, owner.id, seq).await;
    }
    login(&mut app, &owner).await;

    let body = (1..=201)
        .map(|n| format!("{}-{n}", p.key))
        .collect::<Vec<_>>()
        .join("\n");
    let file_id = file_id_of(
        &upload_file(
            &app,
            tp.tenant_id,
            None,
            "many.md",
            "text/markdown",
            body.as_bytes(),
        )
        .await,
    );
    let rows = links(&app, file_id).await;
    assert_eq!(rows.len(), 200);
    assert!(!rows.contains_key(&last), "201 件目は捨てる");

    app.cleanup_user(owner.id).await;
}

/// プロジェクトファイルは同じプロジェクトのタスクにだけ結ぶ（手動添付と同じ範囲）。
/// 対照: テナント一般ファイルはどのプロジェクトのタスクにも結ぶ。
#[tokio::test]
#[serial_test::file_serial(drive)]
async fn project_file_links_only_its_own_project() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let a = project(&app, tp.project_id).await;
    let b = project(&app, insert_extra_project(&app, tp.tenant_id).await).await;
    let a_task = insert_task(&app, &a, owner.id, 1).await;
    let b_task = insert_task(&app, &b, owner.id, 1).await;

    let a_folder = Uuid::new_v4();
    drive_folders::ActiveModel {
        id: Set(a_folder),
        name: Set("a".into()),
        parent_id: Set(None),
        tenant_id: Set(tp.tenant_id),
        project_id: Set(Some(a.id)),
        created_by: Set(owner.id),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert folder");

    login(&mut app, &owner).await;
    let body = format!("{}-1 {}-1", a.key, b.key);
    let in_a = upload_file(
        &app,
        tp.tenant_id,
        Some(a_folder),
        "a.md",
        "text/markdown",
        body.as_bytes(),
    )
    .await;
    assert_eq!(
        links(&app, file_id_of(&in_a)).await,
        HashMap::from([(a_task, "body".into())]),
        "別プロジェクトのタスクへの参照はリンクにならない"
    );

    let general = upload_file(
        &app,
        tp.tenant_id,
        None,
        "general.md",
        "text/markdown",
        body.as_bytes(),
    )
    .await;
    assert_eq!(
        links(&app, file_id_of(&general)).await,
        HashMap::from([(a_task, "body".into()), (b_task, "body".into())]),
        "テナント一般ファイルは両方に結ぶ"
    );

    app.cleanup_user(owner.id).await;
}
