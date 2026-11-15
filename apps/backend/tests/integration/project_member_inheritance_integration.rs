//! プロジェクトがテナントのメンバーを継承する決まり（TASK-501）。
//!
//! 継承する（既定）プロジェクトでは、メンバーを載せても、載せていないテナントのメンバーと BOT が
//! 入れる。継承を切った時だけ、載せた人に絞られる。入れるかを読む所（一覧、入る検め、担当者の候補、
//! 通知の宛先）は、同じ決まり（`service::access::tenant_member_can_enter`）を通る。
use std::collections::BTreeSet;

use crate::common::{
    TestApp, ensure_tenant_member_for_project, execute_sql, insert_extra_project, json_body,
};
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use entity::scopes::Scope;
use sea_orm::prelude::Uuid;
use serde_json::json;

struct Fixture {
    app: TestApp,
    tenant_id: Uuid,
    /// 本番の TASK に当たる、メンバーを載せるプロジェクト
    project: Uuid,
    /// メンバーに載せる人（プロジェクトの Admin）
    listed: Uuid,
    listed_token: String,
    /// 載せていないテナントの Member
    member: Uuid,
    member_token: String,
    /// 許すプロジェクトを指定しない BOT（役は Member）
    bot: Uuid,
    bot_token: String,
}

async fn fixture() -> Fixture {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    let tenant = app.insert_tenant_project(owner.id).await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let project = insert_extra_project(&app, tenant.tenant_id).await;

    let scopes = || {
        vec![
            Scope::ReadProject,
            Scope::WriteProject,
            Scope::ReadTask,
            Scope::WriteTask,
        ]
    };
    let listed = app.insert_user_default().await;
    ensure_tenant_member_for_project(&app.state.db, tenant.project_id, listed.id).await;
    let listed_token = app
        .insert_pat(listed.id, tenant.tenant_id, scopes(), None)
        .await;
    let member = app.insert_user_default().await;
    ensure_tenant_member_for_project(&app.state.db, tenant.project_id, member.id).await;
    let member_token = app
        .insert_pat(member.id, tenant.tenant_id, scopes(), None)
        .await;

    let res = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/bots", tenant.tenant_id),
            json!({"username": "inherit-bot", "role": "Member"}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let bot = Uuid::parse_str(json_body(res).await["id"].as_str().unwrap()).unwrap();
    let res = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/bots/{bot}/tokens", tenant.tenant_id),
            json!({
                "name": "inherit",
                "scopes": ["read:project", "read:task", "write:task"],
                "project_ids": null,
                "expires_at": Utc::now() + Duration::days(30),
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let bot_token = json_body(res).await["token"].as_str().unwrap().to_owned();

    let f = Fixture {
        app,
        tenant_id: tenant.tenant_id,
        project,
        listed: listed.id,
        listed_token,
        member: member.id,
        member_token,
        bot,
        bot_token,
    };
    // 本番の TASK と同じく、管理者の役を付けるために一人だけを載せる
    let res = f
        .app
        .post_json_with_session(
            &f.members_path(),
            json!({"user_id": f.listed, "role": "Admin"}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    f
}

impl Fixture {
    fn project_path(&self) -> String {
        format!("/v1/tenants/{}/projects/{}", self.tenant_id, self.project)
    }

    fn members_path(&self) -> String {
        format!("{}/members", self.project_path())
    }

    async fn listed_projects(&self, token: &str) -> BTreeSet<Uuid> {
        let res = self
            .app
            .get_with_bearer(&format!("/v1/tenants/{}/projects", self.tenant_id), token)
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        json_body(res)
            .await
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Uuid::parse_str(p["id"].as_str().unwrap()).unwrap())
            .collect()
    }

    async fn reads_tasks(&self, token: &str) -> StatusCode {
        self.app
            .get_with_bearer(&format!("{}/tasks", self.project_path()), token)
            .await
            .status()
    }

    async fn set_inherit_as_owner(&self, inherit: bool) {
        let res = self
            .app
            .put_json_with_session(
                &self.project_path(),
                json!({"inherit_tenant_members": inherit}),
            )
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(json_body(res).await["inherit_tenant_members"], inherit);
    }
}

#[tokio::test]
async fn inheriting_project_stays_open_to_unlisted_members_and_bots() {
    let f = fixture().await;

    // 既定は継承する
    let res = f
        .app
        .get_with_bearer(&f.project_path(), &f.member_token)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(json_body(res).await["inherit_tenant_members"], true);

    // 一人を載せても、載せていないテナントの Member と BOT が一覧に見え、課題を読める
    for (who, token) in [("member", &f.member_token), ("bot", &f.bot_token)] {
        assert!(
            f.listed_projects(token).await.contains(&f.project),
            "{who} は継承するプロジェクトを一覧に見る"
        );
        assert_eq!(
            f.reads_tasks(token).await,
            StatusCode::OK,
            "{who} は課題を読める"
        );
    }
    assert!(
        f.listed_projects(&f.listed_token)
            .await
            .contains(&f.project)
    );
}

#[tokio::test]
async fn non_inheriting_project_is_limited_to_listed_members() {
    let f = fixture().await;
    f.set_inherit_as_owner(false).await;

    for (who, token) in [("member", &f.member_token), ("bot", &f.bot_token)] {
        assert!(
            !f.listed_projects(token).await.contains(&f.project),
            "{who} は継承しないプロジェクトを一覧に見ない"
        );
        assert_eq!(
            f.reads_tasks(token).await,
            StatusCode::FORBIDDEN,
            "{who} は課題を読めない"
        );
    }
    assert!(
        f.listed_projects(&f.listed_token)
            .await
            .contains(&f.project)
    );
    assert_eq!(f.reads_tasks(&f.listed_token).await, StatusCode::OK);

    // 継承へ戻すと、また入れる
    f.set_inherit_as_owner(true).await;
    assert!(f.listed_projects(&f.bot_token).await.contains(&f.project));
}

#[tokio::test]
async fn listed_role_overrides_inherited_member_role() {
    let f = fixture().await;
    // 載せた人は載せた役（Admin）で入り、メンバーを変えられる
    let res = f
        .app
        .get_with_bearer(&f.members_path(), &f.listed_token)
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    // 継承で入る人はメンバーの役なので、メンバーを変えられない
    let res = f
        .app
        .post_json_with_bearer(
            &f.members_path(),
            json!({"user_id": f.member, "role": "Admin"}),
            &f.member_token,
        )
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    // 継承で入っている BOT を、役を変えるために載せても Admin にはできない（TASK-407 の上限）
    let res = f
        .app
        .post_json_with_bearer(
            &f.members_path(),
            json!({"user_id": f.bot, "role": "Admin"}),
            &f.listed_token,
        )
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn only_member_managers_can_toggle_inheritance() {
    let f = fixture().await;
    let toggle = |token: String| {
        let path = f.project_path();
        let app = &f.app;
        async move {
            app.put_json_with_bearer(&path, json!({"inherit_tenant_members": false}), &token)
                .await
                .status()
        }
    };
    // 継承で入っているだけのテナントの Member は切り替えられない
    assert_eq!(toggle(f.member_token.clone()).await, StatusCode::FORBIDDEN);
    // プロジェクトの Admin（テナントでは Member）は切り替えられる
    assert_eq!(toggle(f.listed_token.clone()).await, StatusCode::OK);
    // 切った後は、載せていない人は名前などの更新も切り替えもできない
    assert_eq!(toggle(f.member_token.clone()).await, StatusCode::FORBIDDEN);

    // プロジェクトに入れるテナントの Admin は切り替えられる
    f.set_inherit_as_owner(true).await;
    execute_sql(
        &f.app.state.db,
        "UPDATE tenant_members SET role = 'Admin' WHERE tenant_id = $1 AND user_id = $2",
        vec![f.tenant_id.into(), f.member.into()],
    )
    .await;
    assert_eq!(toggle(f.member_token.clone()).await, StatusCode::OK);
}

#[tokio::test]
async fn allowed_project_ids_still_bind_the_token() {
    let f = fixture().await;
    let other = insert_extra_project(&f.app, f.tenant_id).await;
    let res = f
        .app
        .post_json_with_session(
            &format!("/v1/tenants/{}/bots/{}/tokens", f.tenant_id, f.bot),
            json!({
                "name": "bound",
                "scopes": ["read:project", "read:task"],
                "project_ids": [other],
                "expires_at": Utc::now() + Duration::days(30),
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let bound = json_body(res).await["token"].as_str().unwrap().to_owned();
    // 継承で入れるプロジェクトでも、PAT が許していなければ読めない
    assert_eq!(f.reads_tasks(&bound).await, StatusCode::FORBIDDEN);
    assert_eq!(
        f.app
            .get_with_bearer(
                &format!("/v1/tenants/{}/projects/{other}/tasks", f.tenant_id),
                &bound
            )
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn personal_project_does_not_take_inheritance() {
    let f = fixture().await;
    let res = f
        .app
        .get_with_bearer(
            &format!("/v1/tenants/{}/users/me/personal-project", f.tenant_id),
            &f.member_token,
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let personal = json_body(res).await["id"].as_str().unwrap().to_owned();
    let res = f
        .app
        .put_json_with_session(
            &format!("/v1/tenants/{}/projects/{personal}", f.tenant_id),
            json!({"inherit_tenant_members": true}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    // 他人の個人プロジェクトは、継承の列にかかわらず見えない
    assert!(
        !f.listed_projects(&f.bot_token)
            .await
            .contains(&Uuid::parse_str(&personal).unwrap())
    );
}

/// 入れるかを読む所が、同じ決まりを通る。
#[tokio::test]
async fn every_access_reader_follows_the_same_rule() {
    let f = fixture().await;
    let db = &f.app.state.db;
    for inherit in [true, false] {
        f.set_inherit_as_owner(inherit).await;
        let assignable: BTreeSet<Uuid> =
            service::access::assignable_user_ids(db, f.tenant_id, f.project)
                .await
                .unwrap()
                .into_iter()
                .collect();
        let recipients = service::access::project_accessible_user_ids(db, f.project)
            .await
            .unwrap();
        for (who, user_id) in [("listed", f.listed), ("member", f.member), ("bot", f.bot)] {
            let expected = inherit || user_id == f.listed;
            let one = service::access::project_is_open_or_member(db, f.project, user_id)
                .await
                .unwrap();
            let listed = service::access::visible_project_ids(db, vec![f.project], user_id)
                .await
                .unwrap()
                .contains(&f.project);
            let can_assign =
                service::access::require_project_access(db, f.tenant_id, f.project, user_id)
                    .await
                    .is_ok();
            let got = [
                one,
                listed,
                can_assign,
                assignable.contains(&user_id),
                recipients.contains(&user_id),
            ];
            assert_eq!(
                got, [expected; 5],
                "inherit={inherit} {who}: 入る検め・一覧・担当者の検め・担当者の候補・通知の宛先"
            );
        }
    }
}

#[tokio::test]
async fn last_listed_row_is_removable_only_while_inheriting() {
    let f = fixture().await;
    let path = format!("{}/{}", f.members_path(), f.listed);

    // 継承しない時は、最後の 1 行を外すとオーナーのほかは誰も入れなくなるので外せない
    f.set_inherit_as_owner(false).await;
    assert_eq!(
        f.app.delete_with_session(&path).await.status(),
        StatusCode::CONFLICT
    );

    // 継承する時は、最後の 1 行は Admin でも外せる（TASK-217）。外しても他の人は入れたまま
    f.set_inherit_as_owner(true).await;
    assert_eq!(
        f.app.delete_with_session(&path).await.status(),
        StatusCode::NO_CONTENT
    );
    assert!(f.listed_projects(&f.bot_token).await.contains(&f.project));
    assert!(
        f.listed_projects(&f.listed_token)
            .await
            .contains(&f.project)
    );
}

#[tokio::test]
async fn last_viewer_row_of_non_inheriting_project_is_not_removable() {
    let f = fixture().await;
    let other = insert_extra_project(&f.app, f.tenant_id).await;
    execute_sql(
        &f.app.state.db,
        "INSERT INTO project_members (id, project_id, user_id, role) VALUES ($1, $2, $3, 'Viewer')",
        vec![Uuid::new_v4().into(), other.into(), f.member.into()],
    )
    .await;
    execute_sql(
        &f.app.state.db,
        "UPDATE projects SET inherit_tenant_members = false WHERE id = $1",
        vec![other.into()],
    )
    .await;
    // Admin でない最後の 1 行でも、外すと誰も入れなくなるので外せない
    let res = f
        .app
        .delete_with_session(&format!(
            "/v1/tenants/{}/projects/{other}/members/{}",
            f.tenant_id, f.member
        ))
        .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn inheritance_cannot_be_stopped_while_no_one_is_listed() {
    let f = fixture().await;
    let empty = insert_extra_project(&f.app, f.tenant_id).await;
    let path = format!("/v1/tenants/{}/projects/{empty}", f.tenant_id);

    // 載っている人が居ないのに切ると、オーナーのほかは誰も入れなくなるので断る
    let res = f
        .app
        .put_json_with_session(&path, json!({"inherit_tenant_members": false}))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(res).await["message"], "inheritance-needs-members");
    let res = f.app.get_with_bearer(&path, &f.member_token).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(json_body(res).await["inherit_tenant_members"], true);

    // 一人を載せれば切れる
    let res = f
        .app
        .post_json_with_session(
            &format!("{path}/members"),
            json!({"user_id": f.listed, "role": "Admin"}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let res = f
        .app
        .put_json_with_session(&path, json!({"inherit_tenant_members": false}))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
}
