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
