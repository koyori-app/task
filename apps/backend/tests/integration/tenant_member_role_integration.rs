use crate::common::TestApp;
use axum::http::StatusCode;
use entity::scopes::Scope;
use serde_json::Value;
use uuid::Uuid;

/// /v1/tenants の `member_role` の契約を API の側で固定する。
///
/// GUI は Admin 境界の口（鍵の発行など）の選択肢をこの欄で見分ける。
/// これまで frontend の手書き mock しか検めておらず、
/// `TenantListItemResponse::from_parts` の条件が反転しても
/// 「API は 201 を返すのに Admin の画面から発行ボタンが消える」形で壊れ、
/// どの試験も赤くならなかった。ここで固定する契約:
///
///   主（owner）           membership=Owner  member_role=null
///   role=Admin の member  membership=Member member_role=Admin
///   role=Member の member membership=Member member_role=Member
///   客分（guest）         membership=Guest  member_role=null
///
/// Session 経路で四者を検め、PAT 経路（from_parts の別の呼び所）も
/// Admin で検める。
fn find_tenant(items: &[Value], tenant_id: Uuid) -> &Value {
    items
        .iter()
        .find(|t| t["id"] == tenant_id.to_string())
        .expect("対象テナントが一覧に出る")
}

#[tokio::test]
async fn member_role_reflects_actual_role_via_api() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let admin = app.insert_user(false, false).await;
    let member = app.insert_user(false, false).await;
    let guest = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let tenant_id = tp.tenant_id;

    // owner が API 経由で顔ぶれを組む（fixture を DB 直書きにしない——
    // 作成側の検査も同じ経路で兼ねる）
    let members_path = format!("/v1/tenants/{tenant_id}/members");
    let project_members_path =
        format!("/v1/tenants/{tenant_id}/projects/{}/members", tp.project_id);
    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    for (user_id, role) in [
        (admin.id, "Admin"),
        (member.id, "Member"),
        (guest.id, "Member"),
    ] {
        let res = app
            .post_json_with_session(
                &members_path,
                serde_json::json!({ "user_id": user_id, "role": role }),
            )
            .await;
        assert_eq!(res.status(), StatusCode::CREATED, "メンバー追加 {role}");
    }
    // guest を project の明示メンバーにしてからテナントを除名 → project-only の客分
    let res = app
        .post_json_with_session(
            &project_members_path,
            serde_json::json!({ "user_id": guest.id, "role": "Member" }),
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "guest を project へ明示指定"
    );
    let res = app
        .delete_with_session(&format!("{members_path}/{}", guest.id))
        .await;
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "guest をテナントから除名"
    );

    // 四者それぞれの目で /v1/tenants を直に叩く
    for (user, want_membership, want_role) in [
        (&owner, "Owner", Value::Null),
        (&admin, "Member", Value::String("Admin".into())),
        (&member, "Member", Value::String("Member".into())),
        (&guest, "Guest", Value::Null),
    ] {
        app.reset_session_client();
        app.login_session(&user.email, &user.password).await;
        let res = app.get_with_session("/v1/tenants").await;
        assert_eq!(res.status(), StatusCode::OK, "{want_membership} の一覧");
        let items: Vec<Value> = res.json().await.expect("tenants json");
        let item = find_tenant(&items, tenant_id);
        assert_eq!(
            item["membership"], *want_membership,
            "membership（{want_membership} の目）"
        );
        assert_eq!(
            item["member_role"], want_role,
            "member_role（{want_membership} の目）——Owner/Guest は null、Member は実 role"
        );
    }

    // PAT 経路も from_parts を通る。Admin の PAT で member_role=Admin が出ることを固定
    let token = app
        .insert_pat(admin.id, tenant_id, vec![Scope::AdminTenant], None)
        .await;
    let res = app.get_with_bearer("/v1/tenants", &token).await;
    assert_eq!(res.status(), StatusCode::OK, "PAT の一覧");
    let items: Vec<Value> = res.json().await.expect("tenants json (PAT)");
    let item = find_tenant(&items, tenant_id);
    assert_eq!(item["membership"], "Member", "PAT: membership");
    assert_eq!(item["member_role"], "Admin", "PAT: member_role");

    app.cleanup_user(owner.id).await;
    app.cleanup_user(admin.id).await;
    app.cleanup_user(member.id).await;
    app.cleanup_user(guest.id).await;
}
