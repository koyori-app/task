use crate::common::TestApp;
use axum::http::StatusCode;
use entity::scopes::Scope;
use entity::tenant_members;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

/// revoke-all（DELETE /v1/personal_tokens/revoke-all）の認可の釘。
///
/// テナント配下の**他人の鍵まで**失効させる、この系で最も影響の広い口である。
/// 発行（POST /v1/personal_tokens）は主と Admin に開いたが、revoke-all は
/// 主に限ったまま——発行側で新たに通るようになった役（Admin）が、
/// ここでは通らぬことを対で固定する。
///
///   主:    204（テナント配下の鍵が実際に失効する）
///   Admin: 403（鍵は生きたまま——権限は広がっていない）
async fn insert_admin_member(db: &DatabaseConnection, tenant_id: Uuid, user_id: Uuid) {
    tenant_members::ActiveModel {
        id: Set(Uuid::new_v4()),
        tenant_id: Set(tenant_id),
        user_id: Set(user_id),
        role: Set(tenant_members::TenantRole::Admin),
        department: Set(None),
        position: Set(None),
        team: Set(None),
    }
    .insert(db)
    .await
    .expect("insert admin member");
}

async fn live_token_count(db: &DatabaseConnection, tenant_id: Uuid) -> u64 {
    use sea_orm::PaginatorTrait;
    entity::personal_tokens::Entity::find()
        .filter(
            entity::personal_tokens::Column::Id
                .in_subquery(backend::utils::auth::token_ids_bound_to(tenant_id)),
        )
        .filter(entity::personal_tokens::Column::Revoked.eq(false))
        .count(db)
        .await
        .expect("count tokens")
}

#[tokio::test]
async fn revoke_all_allows_owner_and_rejects_admin() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let admin = app.insert_user(false, false).await;
    let tp = app.insert_tenant_project(owner.id).await;
    let tenant_id = tp.tenant_id;
    insert_admin_member(&app.state.db, tenant_id, admin.id).await;

    // 失効の対象になる鍵を二人分置く（revoke-all は他人の鍵にも及ぶ口である）
    app.insert_pat(owner.id, tenant_id, vec![Scope::ReadTask], None)
        .await;
    app.insert_pat(admin.id, tenant_id, vec![Scope::ReadTask], None)
        .await;
    assert_eq!(live_token_count(&app.state.db, tenant_id).await, 2);

    let body = serde_json::json!({ "confirm_tenant_id": tenant_id });

    // Admin は 403——発行はできるが、失効の全面口は開かぬ。鍵は生きたまま
    app.reset_session_client();
    app.login_session(&admin.email, &admin.password).await;
    let res = app
        .delete_json_with_session("/v1/personal_tokens/revoke-all", body.clone())
        .await;
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "Admin には revoke-all を許さぬ（発行側と対）"
    );
    assert_eq!(
        live_token_count(&app.state.db, tenant_id).await,
        2,
        "403 の要求で鍵が消えてはならぬ"
    );

    // 主は 204。テナント配下の鍵（他人の分も含む）が失効する
    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    let res = app
        .delete_json_with_session("/v1/personal_tokens/revoke-all", body)
        .await;
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "主は revoke-all できる"
    );
    assert_eq!(
        live_token_count(&app.state.db, tenant_id).await,
        0,
        "主の 204 でテナント配下の鍵が全て失効する"
    );

    app.cleanup_user(owner.id).await;
    app.cleanup_user(admin.id).await;
}

/// revoke-all は、そのテナントの束縛だけを外す。ほかのテナントにも結び付いたトークンは
/// そちらで使い続けられ、束縛が無くなったトークンだけが使えなくなる。
#[tokio::test]
async fn revoke_all_unbinds_only_that_tenant() {
    let mut app = TestApp::new().await;

    let owner = app.insert_user(false, false).await;
    let stranger = app.insert_user(false, false).await;
    let a = app.insert_tenant_project(owner.id).await.tenant_id;
    let b = app.insert_tenant_project(owner.id).await.tenant_id;
    let both = app
        .insert_pat_in_tenants(owner.id, &[a, b], vec![Scope::Api], None)
        .await;
    let only_a = app.insert_pat(owner.id, a, vec![Scope::Api], None).await;
    let projects = |t: Uuid| format!("/v1/tenants/{t}/projects");
    let body = serde_json::json!({ "confirm_tenant_id": a });

    // A のオーナーでない利用者は 403。束縛は変わらない
    app.reset_session_client();
    app.login_session(&stranger.email, &stranger.password).await;
    let res = app
        .delete_json_with_session("/v1/personal_tokens/revoke-all", body.clone())
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(live_token_count(&app.state.db, a).await, 2);
    assert_eq!(
        app.get_with_bearer(&projects(a), &only_a).await.status(),
        StatusCode::OK
    );

    app.reset_session_client();
    app.login_session(&owner.email, &owner.password).await;
    let res = app
        .delete_json_with_session("/v1/personal_tokens/revoke-all", body)
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    assert_eq!(
        app.get_with_bearer(&projects(b), &both).await.status(),
        StatusCode::OK,
        "もう 1 つのテナントでは使い続けられる"
    );
    assert_eq!(
        app.get_with_bearer(&projects(a), &both).await.status(),
        StatusCode::FORBIDDEN,
        "外したテナントでは使えない"
    );
    assert_eq!(
        app.get_with_bearer(&projects(a), &only_a).await.status(),
        StatusCode::UNAUTHORIZED,
        "束縛が無くなったトークンは取り消される"
    );
    assert_eq!(live_token_count(&app.state.db, a).await, 0);
    assert_eq!(live_token_count(&app.state.db, b).await, 1);

    app.cleanup_user(owner.id).await;
    app.cleanup_user(stranger.id).await;
}
