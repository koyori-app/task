use crate::common::{TestApp, TestTenantProject, TestUser, json_body};
use axum::http::StatusCode;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseTransaction, Statement, TransactionTrait};
use uuid::Uuid;

// プロジェクトメンバー管理（#317）の統合テスト。
//
// メンバー管理 UI が名前・アバターを表示できるよう、メンバー系レスポンスに
// `user`（UserSummary）を同梱したことの回帰テストを中心に置く。
// `user` フィールドの検証は変更前の main では fail する（フィールド自体が無い）。

/// 一覧・追加・変更のレスポンスに表示用のユーザー情報が同梱される。
#[tokio::test]
async fn project_member_responses_embed_user_summary() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let member = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;

    let tenant_members_path = format!("/v1/tenants/{}/members", tp.tenant_id);
    let project_members_path = format!(
        "/v1/tenants/{}/projects/{}/members",
        tp.tenant_id, tp.project_id
    );

    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;

    // プロジェクトメンバーにするには先にテナントメンバーであることが要る（#568）
    let added_tenant = app
        .post_json_with_session(
            &tenant_members_path,
            serde_json::json!({ "user_id": member.id, "role": "Member" }),
        )
        .await;
    assert_eq!(added_tenant.status(), StatusCode::CREATED);
    let added_tenant_body = json_body(added_tenant).await;
    assert_eq!(
        added_tenant_body["user"]["id"],
        member.id.to_string(),
        "テナントメンバー追加のレスポンスにユーザー情報が同梱される"
    );
    assert!(
        added_tenant_body["user"]["username"].is_string(),
        "username を含む"
    );

    // 追加のレスポンス
    let added = app
        .post_json_with_session(
            &project_members_path,
            serde_json::json!({ "user_id": member.id, "role": "Member" }),
        )
        .await;
    assert_eq!(added.status(), StatusCode::CREATED);
    let added_body = json_body(added).await;
    assert_eq!(added_body["user_id"], member.id.to_string());
    assert_eq!(added_body["user"]["id"], member.id.to_string());
    assert!(added_body["user"]["username"].is_string());
    assert!(
        added_body["user"].get("email").is_none(),
        "UserSummary にメールアドレスは含めない"
    );

    // 一覧のレスポンス
    let list = app.get_with_session(&project_members_path).await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = json_body(list).await;
    let rows = list_body.as_array().expect("member list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["user"]["id"], member.id.to_string());
    assert!(rows[0]["user"]["username"].is_string());

    // 変更のレスポンス
    let updated = app
        .put_json_with_session(
            &format!("{project_members_path}/{}", member.id),
            serde_json::json!({ "role": "Admin" }),
        )
        .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated_body = json_body(updated).await;
    assert_eq!(updated_body["role"], "Admin");
    assert_eq!(updated_body["user"]["id"], member.id.to_string());

    // テナントメンバー一覧のレスポンス
    let tenant_list = app.get_with_session(&tenant_members_path).await;
    assert_eq!(tenant_list.status(), StatusCode::OK);
    let tenant_rows = json_body(tenant_list).await;
    let tenant_rows = tenant_rows.as_array().expect("tenant member list");
    // owner は tenant_members に行を持たないが、一覧では read-only の synthetic row として
    // 先頭に並ぶ。埋め込みの user summary は owner 行にも要る
    assert_eq!(tenant_rows.len(), 2);
    assert_eq!(tenant_rows[0]["user"]["id"], owner.id.to_string());
    assert!(tenant_rows[0]["user"]["username"].is_string());
    assert_eq!(tenant_rows[1]["user"]["id"], member.id.to_string());
    assert!(tenant_rows[1]["user"]["username"].is_string());

    app.cleanup_user(owner.id).await;
    app.cleanup_user(member.id).await;
}

/// 一覧を見られるのはオーナー・プロジェクト Admin・入れるテナント Admin だけ（UI の 403 表示の前提）。
/// Admin へ昇格すると見られるようになる対照付き。
#[tokio::test]
async fn project_member_list_requires_project_admin() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let member = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;

    let tenant_members_path = format!("/v1/tenants/{}/members", tp.tenant_id);
    let project_members_path = format!(
        "/v1/tenants/{}/projects/{}/members",
        tp.tenant_id, tp.project_id
    );

    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    assert_eq!(
        app.post_json_with_session(
            &tenant_members_path,
            serde_json::json!({ "user_id": member.id, "role": "Member" }),
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        app.post_json_with_session(
            &project_members_path,
            serde_json::json!({ "user_id": member.id, "role": "Member" }),
        )
        .await
        .status(),
        StatusCode::CREATED
    );

    // Member ロールでは一覧を見られない
    app.reset_session_client();
    app.login_session(&member.email, &member.password).await;
    assert_eq!(
        app.get_with_session(&project_members_path).await.status(),
        StatusCode::FORBIDDEN,
        "プロジェクト Admin でないメンバーは一覧を見られない"
    );

    // Admin へ昇格すると見られる（過剰拒否でないことの対照）
    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    assert_eq!(
        app.put_json_with_session(
            &format!("{project_members_path}/{}", member.id),
            serde_json::json!({ "role": "Admin" }),
        )
        .await
        .status(),
        StatusCode::OK
    );

    app.reset_session_client();
    app.login_session(&member.email, &member.password).await;
    let list = app.get_with_session(&project_members_path).await;
    assert_eq!(list.status(), StatusCode::OK, "Admin なら一覧を見られる");

    app.cleanup_user(owner.id).await;
    app.cleanup_user(member.id).await;
}

/// 同時に相手を降格させても、Admin が 0 人にはならない。
///
/// 「最後の Admin を残す」判定は数えてから書くので、直列化しないと双方が
/// 「まだもう 1 人いる」を読んで両方通る。Admin が 0 人になるとプロジェクト側から
/// 誰も直せなくなり、admin_ids が空になるので 409 のガード自体も効かなくなる。
#[tokio::test]
async fn concurrent_demotions_cannot_drop_the_last_admin() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let alice = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;

    let tenant_members_path = format!("/v1/tenants/{}/members", tp.tenant_id);
    let project_members_path = format!(
        "/v1/tenants/{}/projects/{}/members",
        tp.tenant_id, tp.project_id
    );

    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    for user in [&alice, &bob] {
        assert_eq!(
            app.post_json_with_session(
                &tenant_members_path,
                serde_json::json!({ "user_id": user.id, "role": "Member" }),
            )
            .await
            .status(),
            StatusCode::CREATED
        );
        assert_eq!(
            app.post_json_with_session(
                &project_members_path,
                serde_json::json!({ "user_id": user.id, "role": "Admin" }),
            )
            .await
            .status(),
            StatusCode::CREATED
        );
    }

    // 2 人の Admin を同時に Viewer へ落とす
    let alice_path = format!("{project_members_path}/{}", alice.id);
    let bob_path = format!("{project_members_path}/{}", bob.id);
    let viewer = serde_json::json!({ "role": "Viewer" });
    let (first, second) = tokio::join!(
        app.put_json_with_session(&alice_path, viewer.clone()),
        app.put_json_with_session(&bob_path, viewer.clone()),
    );
    let statuses = [first.status(), second.status()];
    assert!(
        statuses.contains(&StatusCode::CONFLICT),
        "片方は最後の管理者として弾かれる: {statuses:?}"
    );

    // Admin は 1 人残っている（0 人になると誰も直せなくなる）
    let members = json_body(app.get_with_session(&project_members_path).await).await;
    let admins = members
        .as_array()
        .expect("members")
        .iter()
        .filter(|m| m["role"] == "Admin")
        .count();
    assert_eq!(admins, 1, "Admin が 0 人にならない: {members}");

    app.cleanup_user(owner.id).await;
    app.cleanup_user(alice.id).await;
    app.cleanup_user(bob.id).await;
}

/// テナントからの除名も、プロジェクト側の Admin 判定と同じロックの内側で行う。
///
/// 「最後の Admin を残す」判定は `tenant_members` を見て在籍者だけを数える。
/// 除名がそのロックの外にいると、A の降格が B を在籍中の Admin として数えている
/// あいだに B を除名でき、両方成功して在籍 Admin が 0 人になる。
///
/// 実時間の競合は再現が安定しないので、テナント行を掴んだまま除名を投げて
/// **待たされること**を見る。除名が同じロックを取らなければ素通りして落ちる。
#[tokio::test]
async fn removing_a_tenant_member_waits_for_the_membership_lock() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;

    let tenant_members_path = format!("/v1/tenants/{}/members", tp.tenant_id);

    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    assert_eq!(
        app.post_json_with_session(
            &tenant_members_path,
            serde_json::json!({ "user_id": bob.id, "role": "Member" }),
        )
        .await
        .status(),
        StatusCode::CREATED
    );

    let bob_tenant_path = format!("{tenant_members_path}/{}", bob.id);

    // 判定側が取るのと同じロックを、テストが先に握る
    let txn = app.state.db.begin().await.expect("begin");
    txn.query_one_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT id FROM tenants WHERE id = $1 FOR UPDATE",
        [tp.tenant_id.into()],
    ))
    .await
    .expect("lock tenant row");

    let blocked = tokio::time::timeout(
        std::time::Duration::from_millis(700),
        app.delete_with_session(&bob_tenant_path),
    )
    .await;
    assert!(
        blocked.is_err(),
        "ロックを握っている間、除名は待たされる（同じロックを取らないと素通りする）"
    );

    txn.rollback().await.expect("rollback");

    // 対照: ロックが空いていれば除名は通る（過剰に塞いでいない）
    assert_eq!(
        app.delete_with_session(&bob_tenant_path).await.status(),
        StatusCode::NO_CONTENT,
        "ロックが空いていれば除名できる"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(bob.id).await;
}

/// 在籍している Admin が 0 人になっても、テナントオーナーがそのプロジェクトを直せる。
///
/// 直列化しても「A を降格 →（別の操作として）B を除名」の順は両方とも正当なので、
/// 在籍 Admin は 0 人になりうる。除名側で 409 にすれば揃うが、対象が単独 Admin の
/// プロジェクトを全部直すまでオフボーディングできなくなる。維持しているのは
/// 「在籍 Admin ≥ 1」ではなく「そのプロジェクトを管理できる人が常に居る」の方で、
/// ここではその保証が成り立つことを固定する。
#[tokio::test]
async fn an_owner_can_still_manage_a_project_whose_admins_all_left() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let alice = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;

    let tenant_members_path = format!("/v1/tenants/{}/members", tp.tenant_id);
    let project_members_path = format!(
        "/v1/tenants/{}/projects/{}/members",
        tp.tenant_id, tp.project_id
    );

    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    for user in [&alice, &bob] {
        assert_eq!(
            app.post_json_with_session(
                &tenant_members_path,
                serde_json::json!({ "user_id": user.id, "role": "Member" }),
            )
            .await
            .status(),
            StatusCode::CREATED
        );
        assert_eq!(
            app.post_json_with_session(
                &project_members_path,
                serde_json::json!({ "user_id": user.id, "role": "Admin" }),
            )
            .await
            .status(),
            StatusCode::CREATED
        );
    }

    // alice を降格（bob が在籍中の Admin として残るので通る）
    assert_eq!(
        app.put_json_with_session(
            &format!("{project_members_path}/{}", alice.id),
            serde_json::json!({ "role": "Viewer" }),
        )
        .await
        .status(),
        StatusCode::OK
    );

    // 続けて bob をテナントから除名する。プロジェクトのロールでは止めない
    assert_eq!(
        app.delete_with_session(&format!("{tenant_members_path}/{}", bob.id))
            .await
            .status(),
        StatusCode::NO_CONTENT,
        "単独 Admin でもテナントからは外せる（オフボーディングを止めない）"
    );

    // これで在籍している Admin は 0 人。オーナーはそれでも直せる
    let members = json_body(app.get_with_session(&project_members_path).await).await;
    let tenant_rows = json_body(app.get_with_session(&tenant_members_path).await).await;
    let residents: std::collections::HashSet<String> = tenant_rows
        .as_array()
        .expect("tenant members")
        .iter()
        .map(|m| m["user_id"].as_str().expect("user_id").to_owned())
        .collect();
    let active_admins = members
        .as_array()
        .expect("members")
        .iter()
        .filter(|m| m["role"] == "Admin")
        .filter(|m| residents.contains(m["user_id"].as_str().expect("user_id")))
        .count();
    assert_eq!(active_admins, 0, "前提: 在籍 Admin は 0 人: {members}");

    // オーナーは一覧を読め、残った alice を Admin へ戻せる（行き止まりではない）
    assert_eq!(
        app.put_json_with_session(
            &format!("{project_members_path}/{}", alice.id),
            serde_json::json!({ "role": "Admin" }),
        )
        .await
        .status(),
        StatusCode::OK,
        "オーナーは Admin を立て直せる"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(alice.id).await;
    app.cleanup_user(bob.id).await;
}

/// テナントメンバー全員を登録し、`roles` の順にプロジェクトメンバーとして足す。
/// オーナーでログインした状態で戻る。
async fn seed_project_members(
    app: &mut TestApp,
    tp: &TestTenantProject,
    owner: &TestUser,
    tenant_users: &[&TestUser],
    roles: &[(&TestUser, &str)],
) {
    let tenant_members_path = format!("/v1/tenants/{}/members", tp.tenant_id);
    let project_members_path = format!(
        "/v1/tenants/{}/projects/{}/members",
        tp.tenant_id, tp.project_id
    );
    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    for user in tenant_users {
        assert_eq!(
            app.post_json_with_session(
                &tenant_members_path,
                serde_json::json!({ "user_id": user.id, "role": "Member" }),
            )
            .await
            .status(),
            StatusCode::CREATED
        );
    }
    for (user, role) in roles {
        assert_eq!(
            app.post_json_with_session(
                &project_members_path,
                serde_json::json!({ "user_id": user.id, "role": role }),
            )
            .await
            .status(),
            StatusCode::CREATED
        );
    }
}

/// 共有プロジェクトの最後のメンバーである Admin は、自分を外せる（TASK-217）。
///
/// 外すと `project_members` が 0 件になり、継承（テナントメンバー全員に開く）へ戻る。
/// 管理はテナントオーナーが引き継ぐので「管理できる人が常に居る」は崩れない。
/// 修正前は `would_drop_last_admin` で 409 になり、継承へ戻す手段が無かった。
#[tokio::test]
async fn the_only_admin_of_a_shared_project_can_remove_themselves() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let alice = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    seed_project_members(&mut app, &tp, &owner, &[&alice, &bob], &[(&alice, "Admin")]).await;

    let project_path = format!("/v1/tenants/{}/projects/{}", tp.tenant_id, tp.project_id);
    let alice_path = format!("{project_path}/members/{}", alice.id);

    // 前提: alice だけが指定されているので、明示 ACE の無い bob は開けない
    app.reset_session_client();
    app.login_session(&bob.email, &bob.password).await;
    assert_ne!(
        app.get_with_session(&project_path).await.status(),
        StatusCode::OK,
        "前提: メンバー指定がある間は bob に閉じている"
    );

    app.reset_session_client();
    app.login_session(&alice.email, &alice.password).await;
    assert_eq!(
        app.delete_with_session(&alice_path).await.status(),
        StatusCode::NO_CONTENT,
        "共有プロジェクトの最後のメンバーは、Admin でも外せる"
    );

    // 継承に戻ったので、明示 ACE の無いテナントメンバーも開ける
    app.reset_session_client();
    app.login_session(&bob.email, &bob.password).await;
    assert_eq!(
        app.get_with_session(&project_path).await.status(),
        StatusCode::OK,
        "メンバー指定が無くなったプロジェクトはテナントメンバーに開放される"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(alice.id).await;
    app.cleanup_user(bob.id).await;
}

/// 対照: 他のメンバー（Viewer）が残るなら、最後の Admin は従来どおり外せない。
/// Admin の居ない明示 ACE だけの状態を作らない。
#[tokio::test]
async fn the_last_admin_cannot_be_removed_while_other_members_remain() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let alice = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    seed_project_members(
        &mut app,
        &tp,
        &owner,
        &[&alice, &bob],
        &[(&alice, "Admin"), (&bob, "Viewer")],
    )
    .await;

    app.reset_session_client();
    app.login_session(&alice.email, &alice.password).await;
    assert_eq!(
        app.delete_with_session(&format!(
            "/v1/tenants/{}/projects/{}/members/{}",
            tp.tenant_id, tp.project_id, alice.id
        ))
        .await
        .status(),
        StatusCode::CONFLICT,
        "Viewer が残るなら最後の Admin は外せない"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(alice.id).await;
    app.cleanup_user(bob.id).await;
}

/// 対照: 単独メンバーの Admin を降格しても行は残る（継承へ戻らない）ので、従来どおり 409。
#[tokio::test]
async fn the_only_admin_still_cannot_be_demoted() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let alice = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    seed_project_members(&mut app, &tp, &owner, &[&alice], &[(&alice, "Admin")]).await;

    app.reset_session_client();
    app.login_session(&alice.email, &alice.password).await;
    assert_eq!(
        app.put_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/members/{}",
                tp.tenant_id, tp.project_id, alice.id
            ),
            serde_json::json!({ "role": "Viewer" }),
        )
        .await
        .status(),
        StatusCode::CONFLICT,
        "単独 Admin の降格は従来どおり弾く"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(alice.id).await;
}

/// 対照: 個人プロジェクト（Inbox）は 0 件になっても継承へ戻らないので、本人の Admin 行は外せない。
#[tokio::test]
async fn the_owner_row_of_a_personal_project_cannot_be_removed() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let alice = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    seed_project_members(&mut app, &tp, &owner, &[&alice], &[]).await;

    app.reset_session_client();
    app.login_session(&alice.email, &alice.password).await;
    let inbox = json_body(
        app.get_with_session(&format!(
            "/v1/tenants/{}/users/me/personal-project",
            tp.tenant_id
        ))
        .await,
    )
    .await;
    assert_eq!(inbox["is_personal"], true, "前提: 個人プロジェクト");
    let inbox_id = inbox["id"].as_str().expect("inbox id");

    assert_eq!(
        app.delete_with_session(&format!(
            "/v1/tenants/{}/projects/{inbox_id}/members/{}",
            tp.tenant_id, alice.id
        ))
        .await
        .status(),
        StatusCode::CONFLICT,
        "個人プロジェクトの本人 Admin 行は外せない"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(alice.id).await;
}

/// 判定側が取るのと同じテナント行のロックを、テストが先に握る。
async fn lock_tenant(app: &TestApp, tenant_id: Uuid) -> DatabaseTransaction {
    let txn = app.state.db.begin().await.expect("begin");
    txn.query_one_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT id FROM tenants WHERE id = $1 FOR UPDATE",
        [tenant_id.into()],
    ))
    .await
    .expect("lock tenant row");
    txn
}

/// 握っているロックの中でメンバー行を消してコミットする（待っている要求がここで動き出す）。
async fn delete_member_row_and_commit(txn: DatabaseTransaction, project_id: Uuid, user_id: Uuid) {
    txn.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "DELETE FROM project_members WHERE project_id = $1 AND user_id = $2",
        [project_id.into(), user_id.into()],
    ))
    .await
    .expect("delete member row");
    txn.commit().await.expect("commit");
}

async fn member_count(app: &TestApp, project_id: Uuid, user_id: Uuid) -> i64 {
    let row = app
        .state
        .db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT COUNT(*) AS n FROM project_members WHERE project_id = $1 AND user_id = $2",
            [project_id.into(), user_id.into()],
        ))
        .await
        .expect("count")
        .expect("row");
    row.try_get("", "n").expect("n")
}

/// 追加もメンバー変更のロックを取り、認可はロックの後で見る。
///
/// 削除が「最後の 1 行」と数えた後に追加が通ると Viewer だけが残る。ロックを取るだけでは
/// 足りず、待っている間に追加する本人の Admin 行が消されたら（継承へ戻ったら）、
/// 権限を失った追加は 403 で終わらなければならない。
#[tokio::test]
async fn adding_a_member_waits_for_the_lock_and_rechecks_authorization() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let alice = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    seed_project_members(&mut app, &tp, &owner, &[&alice, &bob], &[(&alice, "Admin")]).await;

    app.reset_session_client();
    app.login_session(&alice.email, &alice.password).await;

    let txn = lock_tenant(&app, tp.tenant_id).await;
    let members_path = format!(
        "/v1/tenants/{}/projects/{}/members",
        tp.tenant_id, tp.project_id
    );
    let add = app.post_json_with_session(
        &members_path,
        serde_json::json!({ "user_id": bob.id, "role": "Viewer" }),
    );
    tokio::pin!(add);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(700), &mut add)
            .await
            .is_err(),
        "ロックを握っている間、追加は待たされる"
    );

    // 待っている間に alice の行を消す（alice 自身の削除で継承へ戻ったのと同じ状態）
    delete_member_row_and_commit(txn, tp.project_id, alice.id).await;
    assert_eq!(
        add.await.status(),
        StatusCode::FORBIDDEN,
        "権限を失った後の追加は通らない"
    );
    assert_eq!(
        member_count(&app, tp.project_id, bob.id).await,
        0,
        "bob の行はできない（Viewer だけが残らない）"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(alice.id).await;
    app.cleanup_user(bob.id).await;
}

/// 削除も認可をロックの後で見る。
///
/// Admin A・B のプロジェクトで、B が A の削除をロック待ちしている間に B の行が消されると、
/// 権限を失った B の削除が「最後の 1 行」の例外で通り、公開範囲が広がる。
#[tokio::test]
async fn removing_a_member_rechecks_authorization_after_the_lock() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let alice = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    seed_project_members(
        &mut app,
        &tp,
        &owner,
        &[&alice, &bob],
        &[(&alice, "Admin"), (&bob, "Admin")],
    )
    .await;

    app.reset_session_client();
    app.login_session(&bob.email, &bob.password).await;

    let txn = lock_tenant(&app, tp.tenant_id).await;
    let alice_path = format!(
        "/v1/tenants/{}/projects/{}/members/{}",
        tp.tenant_id, tp.project_id, alice.id
    );
    let remove = app.delete_with_session(&alice_path);
    tokio::pin!(remove);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(700), &mut remove)
            .await
            .is_err(),
        "ロックを握っている間、削除は待たされる"
    );

    // 待っている間に bob の行を消す（alice が bob を外したのと同じ状態）
    delete_member_row_and_commit(txn, tp.project_id, bob.id).await;
    assert_eq!(
        remove.await.status(),
        StatusCode::FORBIDDEN,
        "権限を失った後の削除は通らない"
    );
    assert_eq!(
        member_count(&app, tp.project_id, alice.id).await,
        1,
        "alice の行は残る（継承へ戻らない）"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(alice.id).await;
    app.cleanup_user(bob.id).await;
}

// テナント `Admin` によるプロジェクトメンバー管理（TASK-372）。
// `require_project_admin` が「共有プロジェクトに今も入れるテナント Admin」を通す。
// 修正前はテナント Admin でもプロジェクト Admin の ACE が無ければ 403。

/// オーナーでログインし、`user` をテナントに `role` で登録する。
async fn add_tenant_member_with_role(
    app: &mut TestApp,
    tp: &TestTenantProject,
    owner: &TestUser,
    user: &TestUser,
    role: &str,
) {
    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    let res = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/members", tp.tenant_id),
            serde_json::json!({ "user_id": user.id, "role": role }),
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "add tenant member {role}"
    );
}

fn members_path(tp: &TestTenantProject) -> String {
    format!(
        "/v1/tenants/{}/projects/{}/members",
        tp.tenant_id, tp.project_id
    )
}

/// 開いた共有プロジェクトのメンバーを、ACE の無いテナント `Admin` が管理できる。
/// 最初の ACE で継承が止まるので、自分を先に入れてから他人を足す（仕様書の手順）。
#[tokio::test]
async fn tenant_admin_manages_members_of_an_open_project() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let admin = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &admin, "Admin").await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &bob, "Member").await;
    let path = members_path(&tp);

    app.reset_session_client();
    app.login_session(&admin.email, &admin.password).await;
    assert_eq!(
        app.get_with_session(&path).await.status(),
        StatusCode::OK,
        "ACE の無いテナント Admin が一覧を読める"
    );
    assert_eq!(
        app.post_json_with_session(
            &path,
            serde_json::json!({ "user_id": admin.id, "role": "Admin" })
        )
        .await
        .status(),
        StatusCode::CREATED,
        "自分を先に入れる"
    );
    assert_eq!(
        app.post_json_with_session(
            &path,
            serde_json::json!({ "user_id": bob.id, "role": "Viewer" })
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        app.put_json_with_session(
            &format!("{path}/{}", bob.id),
            serde_json::json!({ "role": "Member" }),
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.delete_with_session(&format!("{path}/{}", bob.id))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );

    for u in [&owner, &admin, &bob] {
        app.cleanup_user(u.id).await;
    }
}

/// 閉じた共有プロジェクト: `Viewer` の ACE で入れるテナント `Admin` はメンバーを足せる。
/// ACE の無いテナント `Admin` は入れないので一覧も 403（見える範囲は変えない）。
#[tokio::test]
async fn tenant_admin_manages_a_closed_project_only_with_an_ace() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let inside = app.insert_user(false, false).await;
    let outside = app.insert_user(false, false).await;
    let alice = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &inside, "Admin").await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &outside, "Admin").await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &alice, "Member").await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &bob, "Member").await;
    let path = members_path(&tp);

    // オーナーが alice（Admin）と inside（Viewer）の ACE で閉じる
    for (user, role) in [(&alice, "Admin"), (&inside, "Viewer")] {
        assert_eq!(
            app.post_json_with_session(
                &path,
                serde_json::json!({ "user_id": user.id, "role": role })
            )
            .await
            .status(),
            StatusCode::CREATED
        );
    }

    app.reset_session_client();
    app.login_session(&outside.email, &outside.password).await;
    assert_eq!(
        app.get_with_session(&path).await.status(),
        StatusCode::FORBIDDEN,
        "入れないテナント Admin は一覧も読めない"
    );

    app.reset_session_client();
    app.login_session(&inside.email, &inside.password).await;
    assert_eq!(
        app.post_json_with_session(
            &path,
            serde_json::json!({ "user_id": bob.id, "role": "Viewer" })
        )
        .await
        .status(),
        StatusCode::CREATED,
        "Viewer ACE しか無いテナント Admin がメンバーを足せる"
    );

    for u in [&owner, &inside, &outside, &alice, &bob] {
        app.cleanup_user(u.id).await;
    }
}

/// 個人プロジェクト（Inbox）はテナントロールでは管理できない。`Viewer` の ACE で入れる
/// テナント `Admin` でも一覧・追加は 403。本人の自動 Admin 行は従来どおり通る。
#[tokio::test]
async fn tenant_admin_cannot_manage_personal_project_members() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let admin = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &admin, "Admin").await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &bob, "Member").await;

    app.reset_session_client();
    app.login_session(&bob.email, &bob.password).await;
    let personal = app
        .get_with_session(&format!(
            "/v1/tenants/{}/users/me/personal-project",
            tp.tenant_id
        ))
        .await;
    assert_eq!(personal.status(), StatusCode::OK);
    let personal_id = json_body(personal).await["id"]
        .as_str()
        .expect("personal id")
        .to_string();
    let path = format!(
        "/v1/tenants/{}/projects/{personal_id}/members",
        tp.tenant_id
    );
    assert_eq!(
        app.post_json_with_session(
            &path,
            serde_json::json!({ "user_id": admin.id, "role": "Viewer" })
        )
        .await
        .status(),
        StatusCode::CREATED,
        "本人（自動 Admin 行）は Inbox のメンバーを管理できる"
    );

    app.reset_session_client();
    app.login_session(&admin.email, &admin.password).await;
    assert_eq!(
        app.get_with_session(&path).await.status(),
        StatusCode::FORBIDDEN,
        "他人の Inbox の一覧は読めない"
    );
    assert_eq!(
        app.post_json_with_session(
            &path,
            serde_json::json!({ "user_id": owner.id, "role": "Viewer" })
        )
        .await
        .status(),
        StatusCode::FORBIDDEN,
        "他人の Inbox にメンバーを足せない"
    );

    for u in [&owner, &admin, &bob] {
        app.cleanup_user(u.id).await;
    }
}

/// テナント `Admin` の分岐もロック内で「今も入れるか」を見直す。
///
/// 可視性の確認（`ensure_tenant_access`）はロックの外で走る。開いたプロジェクトへの追加が
/// ロック待ちの間に、別の操作が他人の ACE を置いて閉じると、ACE の無いテナント Admin は
/// もう入れない。待っていた追加は 403 で終わらなければならない。
#[tokio::test]
async fn tenant_admin_rechecks_visibility_after_the_lock() {
    let mut app = TestApp::new().await;
    let owner = app.insert_user(false, false).await;
    let admin = app.insert_user(false, false).await;
    let alice = app.insert_user(false, false).await;
    let bob = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &admin, "Admin").await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &alice, "Member").await;
    add_tenant_member_with_role(&mut app, &tp, &owner, &bob, "Member").await;

    app.reset_session_client();
    app.login_session(&admin.email, &admin.password).await;

    let txn = lock_tenant(&app, tp.tenant_id).await;
    let path = members_path(&tp);
    let add = app.post_json_with_session(
        &path,
        serde_json::json!({ "user_id": bob.id, "role": "Viewer" }),
    );
    tokio::pin!(add);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(700), &mut add)
            .await
            .is_err(),
        "ロックを握っている間、追加は待たされる"
    );

    // 待っている間に alice の ACE を置いて閉じる
    txn.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO project_members (id, project_id, user_id, role) VALUES ($1, $2, $3, 'Admin')",
        [Uuid::new_v4().into(), tp.project_id.into(), alice.id.into()],
    ))
    .await
    .expect("insert ace");
    txn.commit().await.expect("commit");

    assert_eq!(
        add.await.status(),
        StatusCode::FORBIDDEN,
        "閉じられた後の追加は通らない"
    );
    assert_eq!(member_count(&app, tp.project_id, bob.id).await, 0);

    for u in [&owner, &admin, &alice, &bob] {
        app.cleanup_user(u.id).await;
    }
}
