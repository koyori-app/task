use crate::common::{TestApp, ensure_tenant_member_for_project};
use axum::http::StatusCode;
use entity::{project_members, scopes::Scope, users};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::Value;
use uuid::Uuid;

// 他の利用者のプロフィール（TASK-257）の統合テスト。
//
// - 閲覧できるのはテナントのオーナーとテナントメンバー（客分は 403）
// - 対象はそのテナントのオーナー・メンバー・客分。それ以外は実在しても 404
// - 返すのは id / username / bio / avatar_url だけで、メール等の本人向け項目は返さない

fn profile_path(tenant_id: Uuid, user_id: Uuid) -> String {
    format!("/v1/tenants/{tenant_id}/users/{user_id}")
}

async fn add_project_member(app: &TestApp, project_id: Uuid, user_id: Uuid) {
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

fn sorted_keys(body: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = body
        .as_object()
        .expect("profile must be an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    keys
}

#[tokio::test]
async fn tenant_members_can_view_profiles_of_people_in_the_tenant() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let viewer = app.insert_user(false, false).await;
    let colleague = app.insert_user(false, false).await;
    let guest = app.insert_user(false, false).await;
    let stranger = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    ensure_tenant_member_for_project(&app.state.db, tp.project_id, viewer.id).await;
    ensure_tenant_member_for_project(&app.state.db, tp.project_id, colleague.id).await;
    // 客分: テナントメンバーではなく、共有プロジェクトに明示指定されているだけの人
    add_project_member(&app, tp.project_id, guest.id).await;
    // 別テナントの客分は、このテナントから見ると無関係な人
    let other = app.insert_tenant_project(owner.id).await;
    add_project_member(&app, other.project_id, stranger.id).await;

    users::ActiveModel {
        id: Set(colleague.id),
        bio: Set(Some("同僚の自己紹介".to_string())),
        avatar_url: Set(Some("https://example.com/colleague.png".to_string())),
        ..Default::default()
    }
    .update(&app.state.db)
    .await
    .expect("update colleague profile");

    app.reset_session_client();
    app.login_session_no_content(&viewer.email, &viewer.password)
        .await;

    let res = app
        .get_with_session(&profile_path(tp.tenant_id, colleague.id))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: Value = res.json().await.expect("profile json");
    assert_eq!(
        sorted_keys(&body),
        vec!["avatar_url", "bio", "id", "username"],
        "メール等の本人向け項目を返さない"
    );
    assert_eq!(body["id"], colleague.id.to_string());
    assert_eq!(body["bio"], "同僚の自己紹介");
    assert_eq!(body["avatar_url"], "https://example.com/colleague.png");

    // オーナーは tenant_members に行を持たないが、閲覧の対象になる
    for (target, label) in [
        (owner.id, "オーナー"),
        (guest.id, "客分"),
        (viewer.id, "自分自身"),
    ] {
        let res = app
            .get_with_session(&profile_path(tp.tenant_id, target))
            .await;
        assert_eq!(res.status(), StatusCode::OK, "{label}のプロフィール");
        let body: Value = res.json().await.expect("profile json");
        assert!(body.get("email").is_none(), "{label}でもメールを返さない");
    }

    // テナントに関わらない人は、実在しても存在しない UUID と同じ 404
    for (target, label) in [
        (stranger.id, "別テナントの客分"),
        (Uuid::new_v4(), "存在しない UUID"),
    ] {
        assert_eq!(
            app.get_with_session(&profile_path(tp.tenant_id, target))
                .await
                .status(),
            StatusCode::NOT_FOUND,
            "{label}"
        );
    }
}

#[tokio::test]
async fn profiles_are_forbidden_outside_the_tenant() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let member = app.insert_user(false, false).await;
    let guest = app.insert_user(false, false).await;
    let outsider = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    ensure_tenant_member_for_project(&app.state.db, tp.project_id, member.id).await;
    add_project_member(&app, tp.project_id, guest.id).await;
    let path = profile_path(tp.tenant_id, member.id);

    assert_eq!(
        app.get(&path).await.status(),
        StatusCode::UNAUTHORIZED,
        "未ログイン"
    );

    for (viewer, label) in [(&outsider, "テナント外の人"), (&guest, "客分")] {
        app.reset_session_client();
        app.login_session_no_content(&viewer.email, &viewer.password)
            .await;
        assert_eq!(
            app.get_with_session(&path).await.status(),
            StatusCode::FORBIDDEN,
            "{label}はテナントの利用者を閲覧できない"
        );
    }

    // PAT はメンバー一覧と同じ read:tenant を要求する
    let readable = app
        .insert_pat(member.id, tp.tenant_id, vec![Scope::ReadTenant], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&profile_path(tp.tenant_id, owner.id), &readable)
            .await
            .status(),
        StatusCode::OK
    );
    let task_only = app
        .insert_pat(member.id, tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&profile_path(tp.tenant_id, owner.id), &task_only)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}
