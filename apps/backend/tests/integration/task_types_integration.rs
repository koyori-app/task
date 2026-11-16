//! タスク種別（`project_task_types` と `tasks.task_type_id`）の統合テスト。
//! 定義の CRUD、既定の種別、タスクへの付与、一覧の絞り込み、削除時の扱いを見る。

use crate::common::{TestApp, TestTenantProject, create_status, insert_extra_project, json_body};
use axum::http::StatusCode;
use entity::scopes::Scope;
use sea_orm::{ConnectionTrait, EntityTrait, PaginatorTrait};
use serde_json::{Value, json};
use service::task_types::DEFAULT_TASK_TYPES;
use uuid::Uuid;

/// 既定の種別を入れる SQL。マイグレーション（`m20261116000000_task_types`）と同じファイルを使う。
/// テストハーネスはエンティティからスキーマを組むのでマイグレーションは流れない。
const BACKFILL_SQL: &str = include_str!("../../sql/backfill_default_task_types.sql");

fn types_base(tenant_id: Uuid, project_id: Uuid) -> String {
    format!("/v1/tenants/{tenant_id}/projects/{project_id}/task-types")
}

fn tasks_base(tp: &TestTenantProject) -> String {
    format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    )
}

async fn create_type(app: &TestApp, base: &str, name: &str, color: &str) -> Value {
    let response = app
        .post_json_with_session(base, json!({ "name": name, "color": color }))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED, "create type {name}");
    json_body(response).await
}

async fn list_types(app: &TestApp, base: &str) -> Vec<Value> {
    let response = app.get_with_session(base).await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await.as_array().expect("array").clone()
}

fn names(types: &[Value]) -> Vec<String> {
    types
        .iter()
        .map(|t| t["name"].as_str().expect("name").to_string())
        .collect()
}

/// `task_type_changed` を古い順に返す（API は新しい順）。
async fn task_activities(app: &TestApp, task_path: &str) -> Vec<Value> {
    let response = app
        .get_with_session(&format!("{task_path}/activities"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut events: Vec<Value> = json_body(response).await["activities"]
        .as_array()
        .expect("activities")
        .iter()
        .filter(|a| a["event_type"] == "task_type_changed")
        .cloned()
        .collect();
    events.reverse();
    events
}

#[tokio::test]
async fn task_type_definitions_crud_and_isolation() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let other_project = insert_extra_project(&app, tp.tenant_id).await;
    let base = types_base(tp.tenant_id, tp.project_id);

    // 成功系: 作成は 201、並び順は末尾に付き、一覧は position 順
    let bug = create_type(&app, &base, "不具合", "#ef4444").await;
    let idea = create_type(&app, &base, "アイデア", "#A1b2C3").await;
    assert_eq!(bug["position"], 0);
    assert_eq!(idea["position"], 1);
    assert_eq!(idea["color"], "#A1b2C3");
    assert_eq!(
        names(&list_types(&app, &base).await),
        ["不具合", "アイデア"]
    );

    // 成功系: 名前と色の更新（片方だけでも通る）
    let id = bug["id"].as_str().expect("id");
    let renamed = app
        .put_json_with_session(&format!("{base}/{id}"), json!({ "name": "バグ報告" }))
        .await;
    assert_eq!(renamed.status(), StatusCode::OK);
    let renamed = json_body(renamed).await;
    assert_eq!(renamed["name"], "バグ報告");
    assert_eq!(renamed["color"], "#ef4444");
    let recolored = app
        .put_json_with_session(&format!("{base}/{id}"), json!({ "color": "#000000" }))
        .await;
    assert_eq!(json_body(recolored).await["color"], "#000000");

    // 拒否系: 同名の作成と、同名への変更は 409
    let duplicate = app
        .post_json_with_session(&base, json!({ "name": "アイデア", "color": "#111111" }))
        .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    let collide = app
        .put_json_with_session(&format!("{base}/{id}"), json!({ "name": "アイデア" }))
        .await;
    assert_eq!(collide.status(), StatusCode::CONFLICT);
    // 対照: 別のプロジェクトでは同じ名前を使える
    let other_base = types_base(tp.tenant_id, other_project);
    create_type(&app, &other_base, "アイデア", "#111111").await;

    // 拒否系: 別プロジェクトの種別は、更新も削除も 404 で、元の種別は残る
    let foreign = list_types(&app, &other_base).await[0]["id"]
        .as_str()
        .expect("foreign id")
        .to_string();
    let put = app
        .put_json_with_session(&format!("{base}/{foreign}"), json!({ "name": "改名" }))
        .await;
    assert_eq!(put.status(), StatusCode::NOT_FOUND);
    let delete = app.delete_with_session(&format!("{base}/{foreign}")).await;
    assert_eq!(delete.status(), StatusCode::NOT_FOUND);
    assert_eq!(names(&list_types(&app, &other_base).await), ["アイデア"]);

    // 成功系: 削除は 204 で、2 回目は 404
    let deleted = app.delete_with_session(&format!("{base}/{id}")).await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let again = app.delete_with_session(&format!("{base}/{id}")).await;
    assert_eq!(again.status(), StatusCode::NOT_FOUND);
    assert_eq!(names(&list_types(&app, &base).await), ["アイデア"]);
}

#[tokio::test]
async fn task_type_definitions_reject_outsiders_and_missing_scopes() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let base = types_base(tp.tenant_id, tp.project_id);
    let read_only = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    let writer = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::WriteTask], None)
        .await;

    // 拒否系: プロジェクトに入れない人は一覧も作成も 403
    let outsider = app.insert_user_default().await;
    app.login_session_no_content(&outsider.email, &outsider.password)
        .await;
    assert_eq!(
        app.get_with_session(&base).await.status(),
        StatusCode::FORBIDDEN
    );
    let denied = app
        .post_json_with_session(&base, json!({ "name": "x", "color": "#111111" }))
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    // 拒否系: 読み取りだけの PAT は作成できない。対照: 読める
    app.reset_session_client();
    assert_eq!(
        app.get_with_bearer(&base, &read_only).await.status(),
        StatusCode::OK
    );
    let no_write = app
        .post_json_with_bearer(
            &base,
            json!({ "name": "x", "color": "#111111" }),
            &read_only,
        )
        .await;
    assert_eq!(no_write.status(), StatusCode::FORBIDDEN);
    // 対照: 書き込みの PAT は作れる
    let created = app
        .post_json_with_bearer(&base, json!({ "name": "x", "color": "#111111" }), &writer)
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn task_type_validation_boundaries() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let base = types_base(tp.tenant_id, tp.project_id);

    // 拒否系: 名前が空・101 文字、色が `#RRGGBB` でない値は 400
    for body in [
        json!({ "name": "", "color": "#111111" }),
        json!({ "name": "あ".repeat(101), "color": "#111111" }),
        json!({ "name": "ok", "color": "red" }),
        json!({ "name": "ok", "color": "#12345" }),
    ] {
        let response = app.post_json_with_session(&base, body.clone()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
    // 更新も同じ検証を通る
    let created = create_type(&app, &base, "境界", "#111111").await;
    let id = created["id"].as_str().expect("id");
    for body in [json!({ "name": "" }), json!({ "color": "#12345" })] {
        let response = app
            .put_json_with_session(&format!("{base}/{id}"), body.clone())
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }

    // 対照: 名前が 100 文字、色が大小混じりの 16 進は通る
    let long = "あ".repeat(100);
    let ok = create_type(&app, &base, &long, "#A1b2C3").await;
    assert_eq!(ok["name"], long);
}

#[tokio::test]
async fn task_type_reorder_requires_exactly_the_projects_types() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let other_project = insert_extra_project(&app, tp.tenant_id).await;
    let base = types_base(tp.tenant_id, tp.project_id);
    let a = create_type(&app, &base, "A", "#111111").await["id"].clone();
    let b = create_type(&app, &base, "B", "#111111").await["id"].clone();
    let c = create_type(&app, &base, "C", "#111111").await["id"].clone();
    let foreign = create_type(
        &app,
        &types_base(tp.tenant_id, other_project),
        "Z",
        "#111111",
    )
    .await["id"]
        .clone();
    let reorder = format!("{base}/reorder");

    // 拒否系: 足りない・重複する・別プロジェクトの ID が混ざる並びは 400 で、並びは変わらない
    for ids in [
        json!([a, b]),
        json!([a, a, b]),
        json!([a, b, foreign]),
        json!([a, b, c, c]),
    ] {
        let response = app
            .put_json_with_session(&reorder, json!({ "ids": ids }))
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{ids}");
    }
    assert_eq!(names(&list_types(&app, &base).await), ["A", "B", "C"]);

    // 成功系: 過不足のない並びは 200 で、応答も一覧も新しい順
    let response = app
        .put_json_with_session(&reorder, json!({ "ids": [c, a, b] }))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(names(body.as_array().expect("array")), ["C", "A", "B"]);
    assert_eq!(names(&list_types(&app, &base).await), ["C", "A", "B"]);
    // 並べ替えたあとの作成は末尾に付く
    let d = create_type(&app, &base, "D", "#111111").await;
    assert_eq!(d["position"], 3);
}

#[tokio::test]
async fn new_projects_start_with_the_default_task_types() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;

    let created = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/projects", tp.tenant_id),
            json!({
                "name": "種別の既定",
                "key": format!("T{}", &Uuid::new_v4().simple().to_string()[..6].to_uppercase()),
            }),
        )
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let project_id: Uuid = json_body(created).await["id"]
        .as_str()
        .expect("project id")
        .parse()
        .expect("uuid");

    let personal = app
        .get_with_session(&format!(
            "/v1/tenants/{}/users/me/personal-project",
            tp.tenant_id
        ))
        .await;
    assert_eq!(personal.status(), StatusCode::OK);
    let personal_id: Uuid = json_body(personal).await["id"]
        .as_str()
        .expect("personal project id")
        .parse()
        .expect("uuid");

    // 通常のプロジェクトも個人プロジェクトも、同じ名前・色・並びで 3 件入る
    for id in [project_id, personal_id] {
        let types = list_types(&app, &types_base(tp.tenant_id, id)).await;
        let actual: Vec<(String, String, i64)> = types
            .iter()
            .map(|t| {
                (
                    t["name"].as_str().expect("name").to_string(),
                    t["color"].as_str().expect("color").to_string(),
                    t["position"].as_i64().expect("position"),
                )
            })
            .collect();
        let expected: Vec<(String, String, i64)> = DEFAULT_TASK_TYPES
            .iter()
            .enumerate()
            .map(|(i, (name, color))| (name.to_string(), color.to_string(), i as i64))
            .collect();
        assert_eq!(actual, expected, "project {id}");
    }
}

#[tokio::test]
async fn backfill_adds_defaults_only_to_projects_without_types() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let own_types = insert_extra_project(&app, tp.tenant_id).await;
    let empty_base = types_base(tp.tenant_id, tp.project_id);
    let own_base = types_base(tp.tenant_id, own_types);
    create_type(&app, &own_base, "独自", "#123456").await;
    assert!(list_types(&app, &empty_base).await.is_empty());

    app.state
        .db
        .execute_unprepared(BACKFILL_SQL)
        .await
        .expect("backfill");
    // 2 回流しても増えない
    app.state
        .db
        .execute_unprepared(BACKFILL_SQL)
        .await
        .expect("backfill again");

    // 種別の無いプロジェクトには既定の 3 件が入る
    let backfilled = list_types(&app, &empty_base).await;
    let expected: Vec<String> = DEFAULT_TASK_TYPES
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();
    assert_eq!(names(&backfilled), expected);
    let colors: Vec<&str> = backfilled
        .iter()
        .map(|t| t["color"].as_str().expect("color"))
        .collect();
    assert_eq!(
        colors,
        DEFAULT_TASK_TYPES
            .iter()
            .map(|(_, c)| *c)
            .collect::<Vec<_>>()
    );
    // 自前の種別を持つプロジェクトには足さない
    assert_eq!(names(&list_types(&app, &own_base).await), ["独自"]);
    let total = entity::project_task_types::Entity::find()
        .count(&app.state.db)
        .await
        .expect("count");
    assert!(total >= 4);
}

#[tokio::test]
async fn tasks_take_a_type_on_create_and_update() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let other_project = insert_extra_project(&app, tp.tenant_id).await;
    let status_id = create_status(&app, &tp, "Todo", false).await;
    let base = types_base(tp.tenant_id, tp.project_id);
    let bug = create_type(&app, &base, "バグ", "#ef4444").await;
    let feature = create_type(&app, &base, "機能", "#3b82f6").await;
    let foreign = create_type(
        &app,
        &types_base(tp.tenant_id, other_project),
        "別",
        "#000000",
    )
    .await;
    let tasks = tasks_base(&tp);

    // 拒否系: 別プロジェクトの種別と、存在しない UUID は 404 で、タスクは作られない
    for type_id in [foreign["id"].clone(), json!(Uuid::new_v4())] {
        let response = app
            .post_json_with_session(
                &tasks,
                json!({ "title": "作られない", "status_id": status_id, "task_type_id": type_id }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    let listed = json_body(app.get_with_session(&tasks).await).await;
    assert_eq!(listed["tasks"].as_array().expect("tasks").len(), 0);

    // 成功系: 種別つきは 201 で埋め込まれ、無指定は null
    let typed = app
        .post_json_with_session(
            &tasks,
            json!({ "title": "種別あり", "status_id": status_id, "task_type_id": bug["id"] }),
        )
        .await;
    assert_eq!(typed.status(), StatusCode::CREATED);
    let typed = json_body(typed).await;
    assert_eq!(typed["task_type"]["id"], bug["id"]);
    assert_eq!(typed["task_type"]["name"], "バグ");
    assert_eq!(typed["task_type"]["color"], "#ef4444");
    let untyped = app
        .post_json_with_session(
            &tasks,
            json!({ "title": "種別なし", "status_id": status_id }),
        )
        .await;
    assert_eq!(untyped.status(), StatusCode::CREATED);
    let untyped = json_body(untyped).await;
    assert!(untyped["task_type"].is_null());

    // 拒否系: 更新で別プロジェクトの種別を指すと 404 で、値もアクティビティも変わらない
    let typed_path = format!("{tasks}/{}", typed["id"].as_str().expect("id"));
    let rejected = app
        .put_json_with_session(&typed_path, json!({ "task_type_id": foreign["id"] }))
        .await;
    assert_eq!(rejected.status(), StatusCode::NOT_FOUND);
    let detail = json_body(app.get_with_session(&typed_path).await).await;
    assert_eq!(detail["task_type"]["name"], "バグ");
    assert!(task_activities(&app, &typed_path).await.is_empty());

    // 成功系: 付け替え。アクティビティは名前で記録される
    let switched = app
        .put_json_with_session(&typed_path, json!({ "task_type_id": feature["id"] }))
        .await;
    assert_eq!(switched.status(), StatusCode::OK);
    assert_eq!(json_body(switched).await["task_type"]["name"], "機能");
    let events = task_activities(&app, &typed_path).await;
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0]["payload"],
        json!({ "from": "バグ", "to": "機能" })
    );

    // 同じ値の再指定では記録が増えない
    let same = app
        .put_json_with_session(&typed_path, json!({ "task_type_id": feature["id"] }))
        .await;
    assert_eq!(same.status(), StatusCode::OK);
    assert_eq!(task_activities(&app, &typed_path).await.len(), 1);

    // 値と解除の同時指定は、解除が勝つ
    let cleared_with_value = app
        .put_json_with_session(
            &typed_path,
            json!({ "task_type_id": bug["id"], "clear_task_type_id": true }),
        )
        .await;
    assert_eq!(cleared_with_value.status(), StatusCode::OK);
    assert!(json_body(cleared_with_value).await["task_type"].is_null());
    let events = task_activities(&app, &typed_path).await;
    assert_eq!(events.len(), 2);
    assert_eq!(events[1]["payload"], json!({ "from": "機能", "to": null }));

    // 種別の無いタスクの解除は記録しない。付け直しは 200
    let clear_again = app
        .put_json_with_session(&typed_path, json!({ "clear_task_type_id": true }))
        .await;
    assert_eq!(clear_again.status(), StatusCode::OK);
    assert_eq!(task_activities(&app, &typed_path).await.len(), 2);
    let reattached = app
        .put_json_with_session(&typed_path, json!({ "task_type_id": bug["id"] }))
        .await;
    assert_eq!(json_body(reattached).await["task_type"]["name"], "バグ");
    let events = task_activities(&app, &typed_path).await;
    assert_eq!(events[2]["payload"], json!({ "from": null, "to": "バグ" }));
}

#[tokio::test]
async fn task_list_filters_by_type_and_embeds_it() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let status_id = create_status(&app, &tp, "Todo", false).await;
    let base = types_base(tp.tenant_id, tp.project_id);
    let bug = create_type(&app, &base, "バグ", "#ef4444").await;
    let feature = create_type(&app, &base, "機能", "#3b82f6").await;
    let tasks = tasks_base(&tp);
    for (title, type_id) in [
        ("バグ 1", Some(bug["id"].clone())),
        ("バグ 2", Some(bug["id"].clone())),
        ("機能 1", Some(feature["id"].clone())),
        ("種別なし", None),
    ] {
        let mut body = json!({ "title": title, "status_id": status_id });
        if let Some(type_id) = type_id {
            body["task_type_id"] = type_id;
        }
        let response = app.post_json_with_session(&tasks, body).await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let titles = |body: &Value| -> Vec<String> {
        let mut titles: Vec<String> = body["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .map(|t| t["title"].as_str().expect("title").to_string())
            .collect();
        titles.sort();
        titles
    };

    // 成功系: 絞り込みは一致するものだけ。無指定は全件で、各行に task_type が載る
    let bugs = json_body(
        app.get_with_session(&format!(
            "{tasks}?task_type_id={}",
            bug["id"].as_str().unwrap()
        ))
        .await,
    )
    .await;
    assert_eq!(titles(&bugs), ["バグ 1", "バグ 2"]);
    let all = json_body(app.get_with_session(&tasks).await).await;
    assert_eq!(titles(&all).len(), 4);
    let by_title: std::collections::HashMap<&str, &Value> = all["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|t| (t["title"].as_str().expect("title"), &t["task_type"]))
        .collect();
    assert_eq!(by_title["機能 1"]["name"], "機能");
    assert_eq!(by_title["機能 1"]["color"], "#3b82f6");
    assert!(by_title["種別なし"].is_null());

    // 拒否系: 付いていない種別の ID や存在しない ID では、対象のタスクが返らない
    let nothing = json_body(
        app.get_with_session(&format!("{tasks}?task_type_id={}", Uuid::new_v4()))
            .await,
    )
    .await;
    assert!(titles(&nothing).is_empty());
}

#[tokio::test]
async fn deleting_a_type_keeps_its_tasks_untyped() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(true, false).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let status_id = create_status(&app, &tp, "Todo", false).await;
    let base = types_base(tp.tenant_id, tp.project_id);
    let bug = create_type(&app, &base, "バグ", "#ef4444").await;
    let tasks = tasks_base(&tp);
    let created = app
        .post_json_with_session(
            &tasks,
            json!({ "title": "残る", "status_id": status_id, "task_type_id": bug["id"] }),
        )
        .await;
    let task_id = json_body(created).await["id"]
        .as_str()
        .expect("id")
        .to_string();

    let deleted = app
        .delete_with_session(&format!("{base}/{}", bug["id"].as_str().unwrap()))
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    // タスクは消えず、種別だけが未設定に戻る
    let detail = json_body(app.get_with_session(&format!("{tasks}/{task_id}")).await).await;
    assert_eq!(detail["title"], "残る");
    assert!(detail["task_type"].is_null());
}
