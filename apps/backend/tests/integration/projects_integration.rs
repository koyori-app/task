use crate::common::{TestApp, TestUser, insert_tenant};
use axum::http::StatusCode;
use entity::scopes::Scope;
use uuid::Uuid;

/// create_project が既定ステータスを seed することの回帰テスト（#368）。
/// 修正前は statuses が空配列で fail する。
#[tokio::test]
async fn create_project_seeds_default_statuses() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;

    let response = app
        .post_json_with_session(
            &format!("/v1/tenants/{tenant_id}/projects"),
            serde_json::json!({"name": "Seeded Project", "key": "SEED"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let project: serde_json::Value = response.json().await.unwrap();
    let project_id = project["id"].as_str().unwrap();

    let statuses: Vec<serde_json::Value> = app
        .get_with_session(&format!(
            "/v1/tenants/{tenant_id}/projects/{project_id}/statuses"
        ))
        .await
        .json()
        .await
        .unwrap();

    let names: Vec<&str> = statuses.iter().filter_map(|s| s["name"].as_str()).collect();
    assert_eq!(names, vec!["Backlog", "Todo", "In Progress", "Done"]);

    let default: Vec<&str> = statuses
        .iter()
        .filter(|s| s["is_default"] == true)
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(default, vec!["Todo"]);

    let done: Vec<&str> = statuses
        .iter()
        .filter(|s| s["is_done_state"] == true)
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(done, vec!["Done"]);
}

// テナント `Admin` による共有プロジェクトの作成・更新・削除（TASK-372）。
// 修正前は `require_tenant_owner` でオーナー以外が 403 になる。

/// オーナーでログインし、`user` をテナントに `role` で登録する。
async fn add_tenant_member(
    app: &mut TestApp,
    tenant_id: Uuid,
    owner: &TestUser,
    user: &TestUser,
    role: &str,
) {
    app.reset_session_client();
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let res = app
        .post_json_with_session(
            &format!("/v1/tenants/{tenant_id}/members"),
            serde_json::json!({ "user_id": user.id, "role": role }),
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "add tenant member {role}"
    );
}

async fn login(app: &mut TestApp, user: &TestUser) {
    app.reset_session_client();
    app.login_session_no_content(&user.email, &user.password)
        .await;
}

fn unique_key(prefix: &str) -> String {
    format!(
        "{prefix}{}",
        Uuid::new_v4().simple().to_string()[..6].to_uppercase()
    )
}

/// キーの一意はテナント内だけで、別テナントなら同じキーを指定できる。同じテナント内の重複は 409。
/// 修正前は schema sync が `key` だけの UNIQUE を作るので、別テナントの作成が 409 で fail する。
#[tokio::test]
async fn explicit_project_key_is_unique_only_within_tenant() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    login(&mut app, &user).await;
    let tenant_a = insert_tenant(&app.state.db, user.id).await;
    let tenant_b = insert_tenant(&app.state.db, user.id).await;
    let key = unique_key("K");

    for tenant_id in [tenant_a, tenant_b] {
        let response = app
            .post_json_with_session(
                &format!("/v1/tenants/{tenant_id}/projects"),
                serde_json::json!({"name": "same key", "key": key}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let duplicate = app
        .post_json_with_session(
            &format!("/v1/tenants/{tenant_a}/projects"),
            serde_json::json!({"name": "duplicate", "key": key}),
        )
        .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
}

/// 英数字のない名前に付く既定キー `PROJ` も、別テナントならそのまま使える。
/// 修正前は schema sync が `key` だけの UNIQUE を作るので、別テナントの作成が 500 で fail する。
#[tokio::test]
async fn default_project_key_is_reused_in_another_tenant() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    login(&mut app, &user).await;

    for _ in 0..2 {
        let tenant_id = insert_tenant(&app.state.db, user.id).await;
        let response = app
            .post_json_with_session(
                &format!("/v1/tenants/{tenant_id}/projects"),
                serde_json::json!({"name": "日本語の名前"}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let project: serde_json::Value = response.json().await.unwrap();
        assert_eq!(project["key"], "PROJ");
    }
}

/// `key` を省略した作成で自動生成キーが衝突したら、先頭 6 文字とランダムな 4 文字に作り直す。
/// 修正前は UNIQUE 違反で中止されたトランザクションへ INSERT し直し、25P02 で 500 になる。
/// 明示した `key` の重複は作り直さず 409 のまま。
#[tokio::test]
async fn create_project_regenerates_colliding_generated_key() {
    let mut app = TestApp::new().await;
    let user = app.insert_user_default().await;
    login(&mut app, &user).await;
    let tenant_id = insert_tenant(&app.state.db, user.id).await;
    let url = format!("/v1/tenants/{tenant_id}/projects");
    let name = unique_key("R");

    let first = app
        .post_json_with_session(&url, serde_json::json!({ "name": name }))
        .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first: serde_json::Value = first.json().await.unwrap();
    assert_eq!(first["key"], name.as_str());

    let second = app
        .post_json_with_session(&url, serde_json::json!({ "name": name }))
        .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second: serde_json::Value = second.json().await.unwrap();
    let second_key = second["key"].as_str().unwrap();
    assert_eq!(second_key.len(), 10, "{second_key}");
    assert!(second_key.starts_with(&name[..6]), "{second_key}");

    // 作り直した試行のトランザクションで既定ステータスまで確定している
    let statuses: Vec<serde_json::Value> = app
        .get_with_session(&format!(
            "{url}/{}/statuses",
            second["id"].as_str().unwrap()
        ))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(statuses.len(), 4);

    let explicit = app
        .post_json_with_session(&url, serde_json::json!({ "name": name, "key": name }))
        .await;
    assert_eq!(explicit.status(), StatusCode::CONFLICT);

    app.cleanup_user(user.id).await;
}

/// テナント `Admin` は共有プロジェクトを作成・更新・削除できる。作ったプロジェクトは継承で
/// テナント全員に開く。`Member` の作成と `Viewer` の更新は従来どおり 403。
#[tokio::test]
async fn tenant_admin_creates_updates_and_deletes_shared_projects() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let member = app.insert_user_default().await;
    let viewer = app.insert_user_default().await;
    let tenant_id = insert_tenant(&app.state.db, owner.id).await;
    add_tenant_member(&mut app, tenant_id, &owner, &admin, "Admin").await;
    add_tenant_member(&mut app, tenant_id, &owner, &member, "Member").await;
    add_tenant_member(&mut app, tenant_id, &owner, &viewer, "Viewer").await;
    let projects_path = format!("/v1/tenants/{tenant_id}/projects");

    login(&mut app, &member).await;
    let denied = app
        .post_json_with_session(
            &projects_path,
            serde_json::json!({"name": "by member", "key": unique_key("M")}),
        )
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN, "Member は作れない");

    login(&mut app, &admin).await;
    let created = app
        .post_json_with_session(
            &projects_path,
            serde_json::json!({"name": "by admin", "key": unique_key("A")}),
        )
        .await;
    assert_eq!(
        created.status(),
        StatusCode::CREATED,
        "テナント Admin は作れる"
    );
    let project: serde_json::Value = created.json().await.unwrap();
    let project_id = project["id"].as_str().unwrap().to_string();
    let project_path = format!("{projects_path}/{project_id}");

    let updated = app
        .put_json_with_session(&project_path, serde_json::json!({"name": "renamed"}))
        .await;
    assert_eq!(
        updated.status(),
        StatusCode::OK,
        "作った Admin が更新できる"
    );

    login(&mut app, &member).await;
    let seen = app.get_with_session(&project_path).await;
    assert_eq!(seen.status(), StatusCode::OK, "継承でテナント全員に開く");
    assert_eq!(
        seen.json::<serde_json::Value>().await.unwrap()["name"],
        "renamed"
    );

    login(&mut app, &viewer).await;
    let denied = app
        .put_json_with_session(&project_path, serde_json::json!({"name": "by viewer"}))
        .await;
    assert_eq!(
        denied.status(),
        StatusCode::FORBIDDEN,
        "Viewer は更新できない"
    );

    login(&mut app, &admin).await;
    let deleted = app.delete_with_session(&project_path).await;
    assert_eq!(
        deleted.status(),
        StatusCode::NO_CONTENT,
        "作った Admin が削除できる"
    );

    for u in [&owner, &admin, &member, &viewer] {
        app.cleanup_user(u.id).await;
    }
}

/// PAT: テナント `Admin` の非束縛 PAT（`write:project`）は作成できる。プロジェクト束縛 PAT は
/// テナント全体の口を通らず、削除はセッション専用のまま（PAT では 403）。
#[tokio::test]
async fn tenant_admin_pat_creates_but_cannot_delete_projects() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    add_tenant_member(&mut app, tp.tenant_id, &owner, &admin, "Admin").await;
    let projects_path = format!("/v1/tenants/{}/projects", tp.tenant_id);

    let unbound = app
        .insert_pat(admin.id, tp.tenant_id, vec![Scope::WriteProject], None)
        .await;
    let bound = app
        .insert_pat(
            admin.id,
            tp.tenant_id,
            vec![Scope::WriteProject],
            Some(vec![tp.project_id]),
        )
        .await;

    app.reset_session_client();
    let denied = app
        .post_json_with_bearer(
            &projects_path,
            serde_json::json!({"name": "bound", "key": unique_key("B")}),
            &bound,
        )
        .await;
    assert_eq!(
        denied.status(),
        StatusCode::FORBIDDEN,
        "束縛 PAT は作れない"
    );

    let created = app
        .post_json_with_bearer(
            &projects_path,
            serde_json::json!({"name": "unbound", "key": unique_key("U")}),
            &unbound,
        )
        .await;
    assert_eq!(
        created.status(),
        StatusCode::CREATED,
        "Admin の非束縛 PAT は作れる"
    );
    let project_id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let deleted = app
        .client()
        .delete(format!("{}{projects_path}/{project_id}", app.base_url()))
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {unbound}"),
        )
        .send()
        .await
        .expect("delete with bearer");
    assert_eq!(
        deleted.status(),
        StatusCode::FORBIDDEN,
        "削除はセッション専用"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(admin.id).await;
}

/// 見える範囲は変えない: 明示 ACE で閉じたプロジェクトに入っていないテナント `Admin` は 403。
/// `Viewer` の ACE で入れていれば、テナントロールで更新・削除できる（プロジェクト Admin ではなく
/// テナント Admin として通ることの証明）。降格されれば触れなくなる。
#[tokio::test]
async fn tenant_admin_manages_closed_projects_only_while_inside() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let alice = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    add_tenant_member(&mut app, tp.tenant_id, &owner, &admin, "Admin").await;
    add_tenant_member(&mut app, tp.tenant_id, &owner, &alice, "Member").await;
    let project_path = format!("/v1/tenants/{}/projects/{}", tp.tenant_id, tp.project_id);
    let members_path = format!("{project_path}/members");

    // alice だけの ACE で閉じる（オーナーが操作）
    let closed = app
        .post_json_with_session(
            &members_path,
            serde_json::json!({ "user_id": alice.id, "role": "Admin" }),
        )
        .await;
    assert_eq!(closed.status(), StatusCode::CREATED);
    // TASK-501 より前と同じく、載せた人だけが入れる形にする
    crate::common::stop_inheriting(&app.state.db, tp.project_id).await;

    login(&mut app, &admin).await;
    assert_eq!(
        app.put_json_with_session(&project_path, serde_json::json!({"name": "x"}))
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "閉じたプロジェクトに入っていない Admin は更新できない"
    );
    assert_eq!(
        app.delete_with_session(&project_path).await.status(),
        StatusCode::FORBIDDEN,
        "削除もできない"
    );

    // Viewer の ACE を置くと入れる → テナントロールで管理できる
    login(&mut app, &owner).await;
    let ace = app
        .post_json_with_session(
            &members_path,
            serde_json::json!({ "user_id": admin.id, "role": "Viewer" }),
        )
        .await;
    assert_eq!(ace.status(), StatusCode::CREATED);

    login(&mut app, &admin).await;
    assert_eq!(
        app.put_json_with_session(
            &project_path,
            serde_json::json!({"name": "by tenant admin"})
        )
        .await
        .status(),
        StatusCode::OK,
        "Viewer ACE しか無いテナント Admin が更新できる"
    );

    // 降格すると触れない
    login(&mut app, &owner).await;
    let demoted = app
        .put_json_with_session(
            &format!("/v1/tenants/{}/members/{}", tp.tenant_id, admin.id),
            serde_json::json!({ "role": "Member" }),
        )
        .await;
    assert_eq!(demoted.status(), StatusCode::OK);

    login(&mut app, &admin).await;
    assert_eq!(
        app.put_json_with_session(&project_path, serde_json::json!({"name": "after demotion"}))
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "降格後は更新できない"
    );

    for u in [&owner, &admin, &alice] {
        app.cleanup_user(u.id).await;
    }
}

/// 個人プロジェクト（Inbox）は対象外。`Viewer` の ACE で他人の Inbox に入れるテナント `Admin` でも
/// 本体の更新・削除はできない。客分（テナント非所属）は作成できない。
#[tokio::test]
async fn tenant_admin_cannot_touch_personal_projects_and_guests_cannot_create() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let admin = app.insert_user_default().await;
    let bob = app.insert_user_default().await;
    let guest = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    add_tenant_member(&mut app, tp.tenant_id, &owner, &admin, "Admin").await;
    add_tenant_member(&mut app, tp.tenant_id, &owner, &bob, "Member").await;

    // 客分: テナントに居ないが共有プロジェクトの明示メンバー
    let guest_ace = app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/members",
                tp.tenant_id, tp.project_id
            ),
            serde_json::json!({ "user_id": guest.id, "role": "Member" }),
        )
        .await;
    assert_eq!(guest_ace.status(), StatusCode::CREATED);

    login(&mut app, &bob).await;
    let personal = app
        .get_with_session(&format!(
            "/v1/tenants/{}/users/me/personal-project",
            tp.tenant_id
        ))
        .await;
    assert_eq!(personal.status(), StatusCode::OK);
    let personal_id = personal.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let personal_path = format!("/v1/tenants/{}/projects/{personal_id}", tp.tenant_id);

    // 本人（自動 Admin 行）が admin を Viewer として Inbox に入れる
    let ace = app
        .post_json_with_session(
            &format!("{personal_path}/members"),
            serde_json::json!({ "user_id": admin.id, "role": "Viewer" }),
        )
        .await;
    assert_eq!(ace.status(), StatusCode::CREATED);

    login(&mut app, &admin).await;
    assert_eq!(
        app.get_with_session(&personal_path).await.status(),
        StatusCode::OK,
        "ACE があるので入れはする（前提の確認）"
    );
    assert_eq!(
        app.put_json_with_session(&personal_path, serde_json::json!({"name": "hijack"}))
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "他人の Inbox は更新できない"
    );
    assert_eq!(
        app.delete_with_session(&personal_path).await.status(),
        StatusCode::FORBIDDEN,
        "他人の Inbox は削除できない"
    );

    login(&mut app, &guest).await;
    let denied = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/projects", tp.tenant_id),
            serde_json::json!({"name": "by guest", "key": unique_key("G")}),
        )
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN, "客分は作れない");

    for u in [&owner, &admin, &bob, &guest] {
        app.cleanup_user(u.id).await;
    }
}

// プロジェクトの応答に載る連携先 `repository`（TASK-371）。
// 修正前は欄が無いので、連携ありの応答の比較で fail する。

use crate::forge_commit_links_integration::{REPO_NAME, REPO_OWNER, insert_integration};

fn linked_repository() -> serde_json::Value {
    serde_json::json!({
        "host": "github",
        "host_url": "https://github.com",
        "repo_owner": REPO_OWNER,
        "repo_name": REPO_NAME,
        "html_url": format!("https://github.com/{REPO_OWNER}/{REPO_NAME}"),
    })
}

fn find_project<'a>(list: &'a [serde_json::Value], id: &str) -> &'a serde_json::Value {
    list.iter()
        .find(|project| project["id"] == id)
        .unwrap_or_else(|| panic!("project {id} in the list"))
}

/// 連携のあるプロジェクトは `repository` に連携先、無いプロジェクトは `null`。
/// 一覧・単体・作成・更新・個人プロジェクト・ダッシュボードのどれも同じ意味で返す。
#[tokio::test]
async fn project_responses_carry_the_linked_repository() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    insert_integration(&app, tp.project_id, 4242, owner.id).await;
    login(&mut app, &owner).await;
    let projects_path = format!("/v1/tenants/{}/projects", tp.tenant_id);
    let linked_id = tp.project_id.to_string();

    let created = app
        .post_json_with_session(
            &projects_path,
            serde_json::json!({"name": "unlinked", "key": unique_key("N")}),
        )
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: serde_json::Value = created.json().await.unwrap();
    assert!(
        created.as_object().unwrap().contains_key("repository"),
        "作成の応答に欄がある"
    );
    assert!(created["repository"].is_null(), "作成直後は連携なし");
    let unlinked_id = created["id"].as_str().unwrap().to_string();

    let list: Vec<serde_json::Value> = app
        .get_with_session(&projects_path)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        find_project(&list, &linked_id)["repository"],
        linked_repository()
    );
    assert!(find_project(&list, &unlinked_id)["repository"].is_null());

    let single: serde_json::Value = app
        .get_with_session(&format!("{projects_path}/{linked_id}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(single["repository"], linked_repository());
    let single: serde_json::Value = app
        .get_with_session(&format!("{projects_path}/{unlinked_id}"))
        .await
        .json()
        .await
        .unwrap();
    assert!(single["repository"].is_null());

    let updated = app
        .put_json_with_session(
            &format!("{projects_path}/{linked_id}"),
            serde_json::json!({"name": "renamed"}),
        )
        .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated: serde_json::Value = updated.json().await.unwrap();
    assert_eq!(updated["repository"], linked_repository());

    let personal = app
        .get_with_session(&format!(
            "/v1/tenants/{}/users/me/personal-project",
            tp.tenant_id
        ))
        .await;
    assert_eq!(personal.status(), StatusCode::OK);
    let personal: serde_json::Value = personal.json().await.unwrap();
    assert!(personal.as_object().unwrap().contains_key("repository"));
    assert!(personal["repository"].is_null());

    let dashboard = app
        .get_with_session(&format!("/v1/tenants/{}/users/me/dashboard", tp.tenant_id))
        .await;
    assert_eq!(dashboard.status(), StatusCode::OK);
    let dashboard: serde_json::Value = dashboard.json().await.unwrap();
    let dashboard_projects: Vec<serde_json::Value> = dashboard["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["project"].clone())
        .collect();
    assert_eq!(
        find_project(&dashboard_projects, &linked_id)["repository"],
        linked_repository()
    );
    assert!(find_project(&dashboard_projects, &unlinked_id)["repository"].is_null());

    // トークン類は応答に出ない
    let raw = app
        .get_with_session(&projects_path)
        .await
        .text()
        .await
        .unwrap();
    assert!(!raw.contains("installation_id") && !raw.contains("access_token"));

    app.cleanup_user(owner.id).await;
}

/// `read:project` だけの PAT で連携先を読める。scope の無い PAT・別テナントの PAT・
/// 制限付き PAT の許可外は、プロジェクトと同じく読めない。
#[tokio::test]
async fn linked_repository_is_readable_only_where_the_project_is() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    let other = app.insert_tenant_project(owner.id).await;
    insert_integration(&app, tp.project_id, 4243, owner.id).await;
    login(&mut app, &owner).await;
    let projects_path = format!("/v1/tenants/{}/projects", tp.tenant_id);
    let linked_path = format!("{projects_path}/{}", tp.project_id);

    let second = app
        .post_json_with_session(
            &projects_path,
            serde_json::json!({"name": "second", "key": unique_key("S")}),
        )
        .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_id: Uuid = second.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    insert_integration(&app, second_id, 4243, owner.id).await;
    let second_path = format!("{projects_path}/{second_id}");
    app.reset_session_client();

    let read_project = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadProject], None)
        .await;
    let list = app.get_with_bearer(&projects_path, &read_project).await;
    assert_eq!(list.status(), StatusCode::OK);
    let list: Vec<serde_json::Value> = list.json().await.unwrap();
    assert_eq!(
        find_project(&list, &tp.project_id.to_string())["repository"],
        linked_repository(),
        "read:project だけで一覧の連携先を読める"
    );
    let single = app.get_with_bearer(&linked_path, &read_project).await;
    assert_eq!(single.status(), StatusCode::OK);
    assert_eq!(
        single.json::<serde_json::Value>().await.unwrap()["repository"],
        linked_repository()
    );

    let no_scope = app
        .insert_pat(owner.id, tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&projects_path, &no_scope)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.get_with_bearer(&linked_path, &no_scope).await.status(),
        StatusCode::FORBIDDEN
    );

    let other_tenant = app
        .insert_pat(owner.id, other.tenant_id, vec![Scope::ReadProject], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&projects_path, &other_tenant)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.get_with_bearer(&linked_path, &other_tenant)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );

    let restricted = app
        .insert_pat(
            owner.id,
            tp.tenant_id,
            vec![Scope::ReadProject],
            Some(vec![tp.project_id]),
        )
        .await;
    assert_eq!(
        app.get_with_bearer(&projects_path, &restricted)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "制限付き PAT は一覧を読めない"
    );
    assert_eq!(
        app.get_with_bearer(&second_path, &restricted)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "許可外のプロジェクトは読めない"
    );
    let allowed = app.get_with_bearer(&linked_path, &restricted).await;
    assert_eq!(allowed.status(), StatusCode::OK);
    assert_eq!(
        allowed.json::<serde_json::Value>().await.unwrap()["repository"],
        linked_repository()
    );

    app.cleanup_user(owner.id).await;
}

/// 一覧で見えないプロジェクトの連携先は、応答のどこにも出ない。
#[tokio::test]
async fn hidden_project_does_not_leak_its_repository_into_the_list() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let alice = app.insert_user_default().await;
    let bob = app.insert_user_default().await;
    let tp = app.insert_tenant_project(owner.id).await;
    insert_integration(&app, tp.project_id, 4244, owner.id).await;
    add_tenant_member(&mut app, tp.tenant_id, &owner, &alice, "Member").await;
    add_tenant_member(&mut app, tp.tenant_id, &owner, &bob, "Member").await;
    let projects_path = format!("/v1/tenants/{}/projects", tp.tenant_id);

    // alice だけが入れるプロジェクトにする
    let closed = app
        .post_json_with_session(
            &format!("{projects_path}/{}/members", tp.project_id),
            serde_json::json!({ "user_id": alice.id, "role": "Member" }),
        )
        .await;
    assert_eq!(closed.status(), StatusCode::CREATED);
    crate::common::stop_inheriting(&app.state.db, tp.project_id).await;

    login(&mut app, &bob).await;
    let hidden = app.get_with_session(&projects_path).await;
    assert_eq!(hidden.status(), StatusCode::OK);
    let hidden = hidden.text().await.unwrap();
    assert!(
        !hidden.contains(REPO_NAME) && !hidden.contains(&tp.project_id.to_string()),
        "入れない人の一覧に連携先が出ない: {hidden}"
    );

    login(&mut app, &alice).await;
    let visible: Vec<serde_json::Value> = app
        .get_with_session(&projects_path)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        find_project(&visible, &tp.project_id.to_string())["repository"],
        linked_repository(),
        "入れる人には出る"
    );

    for u in [&owner, &alice, &bob] {
        app.cleanup_user(u.id).await;
    }
}
