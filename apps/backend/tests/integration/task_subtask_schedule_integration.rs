// サブタスク作成時に、親の開始予定日・締切を未指定項目の初期値にする（TASK-506）。
// 受け入れ条件:
// - 親だけ指定して作成 → 開始予定日・Soft 締切・Hard 締切が親と同じ
// - 項目を指定した場合はその値が優先される
// - 片方の締切だけ指定し、親のもう片方と Soft < Hard にならないときは親の側を入れない
// - 作成後に親が変わっても子は変わらず、子側で変更・解除できる
// - 親なしの作成・PUT での親付けでは写さない
// - 一括作成も要素ごとに同じ規則

use crate::common::{TestApp, TestTenantProject, TestUser};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

struct Fx {
    app: TestApp,
    #[allow(dead_code)]
    tp: TestTenantProject,
    #[allow(dead_code)]
    owner: TestUser,
    status_id: String,
    base: String,
    parent_id: String,
}

const P_SOFT: &str = "2026-12-10T00:00:00Z";
const P_HARD: &str = "2026-12-20T00:00:00Z";
const P_START: &str = "2026-12-01";

fn ts(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str().map(|s| s.parse().unwrap())
}

fn at(s: &str) -> Option<DateTime<Utc>> {
    Some(s.parse().unwrap())
}

async fn setup() -> Fx {
    let mut app = TestApp::new().await;
    let owner = app.insert_user_default().await;
    app.login_session_no_content(&owner.email, &owner.password)
        .await;
    let tp = app.insert_tenant_project(owner.id).await;
    let status_id = crate::common::create_status(&app, &tp, "Todo", false)
        .await
        .to_string();
    let base = format!(
        "/v1/tenants/{}/projects/{}/tasks",
        tp.tenant_id, tp.project_id
    );
    let res = app
        .post_json_with_session(
            &base,
            json!({
                "title": "parent",
                "status_id": status_id,
                "soft_deadline": P_SOFT,
                "hard_deadline": P_HARD,
                "planned_start_on": P_START,
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let parent: Value = res.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap().to_string();
    Fx {
        app,
        tp,
        owner,
        status_id,
        base,
        parent_id,
    }
}

async fn create(fx: &Fx, extra: Value) -> Value {
    let mut body = json!({
        "title": "child",
        "status_id": fx.status_id,
        "parent_task_id": fx.parent_id,
    });
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    let res = fx.app.post_json_with_session(&fx.base, body).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    res.json().await.unwrap()
}

async fn fetch(fx: &Fx, id: &str) -> Value {
    let res = fx
        .app
        .get_with_session(&format!("{}/{}", fx.base, id))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    res.json().await.unwrap()
}

#[tokio::test]
async fn subtask_takes_unset_schedule_from_parent() {
    let fx = setup().await;
    let child = create(&fx, json!({})).await;
    assert_eq!(ts(&child["soft_deadline"]), at(P_SOFT));
    assert_eq!(ts(&child["hard_deadline"]), at(P_HARD));
    assert_eq!(child["planned_start_on"], P_START);
    // 取得し直しても同じ（保存されている）
    let got = fetch(&fx, child["id"].as_str().unwrap()).await;
    assert_eq!(ts(&got["soft_deadline"]), at(P_SOFT));
    assert_eq!(ts(&got["hard_deadline"]), at(P_HARD));
    assert_eq!(got["planned_start_on"], P_START);
}

#[tokio::test]
async fn explicit_schedule_wins_over_parent() {
    let fx = setup().await;
    let child = create(
        &fx,
        json!({
            "soft_deadline": "2026-12-12T00:00:00Z",
            "hard_deadline": "2026-12-18T00:00:00Z",
            "planned_start_on": "2026-12-03",
        }),
    )
    .await;
    assert_eq!(ts(&child["soft_deadline"]), at("2026-12-12T00:00:00Z"));
    assert_eq!(ts(&child["hard_deadline"]), at("2026-12-18T00:00:00Z"));
    assert_eq!(child["planned_start_on"], "2026-12-03");
}

#[tokio::test]
async fn conflicting_parent_deadline_is_not_inherited() {
    let fx = setup().await;

    // (a) Soft だけ指定し、親の Hard（12/20）より後 → Hard は入れない
    let a = create(&fx, json!({"soft_deadline": "2026-12-25T00:00:00Z"})).await;
    assert_eq!(ts(&a["soft_deadline"]), at("2026-12-25T00:00:00Z"));
    assert!(a["hard_deadline"].is_null());
    assert_eq!(a["planned_start_on"], P_START);

    // (b) Hard だけ指定し、親の Soft（12/10）より前 → Soft は入れない
    let b = create(&fx, json!({"hard_deadline": "2026-12-05T00:00:00Z"})).await;
    assert_eq!(ts(&b["hard_deadline"]), at("2026-12-05T00:00:00Z"));
    assert!(b["soft_deadline"].is_null());

    // (c) Soft だけ指定し、親の Hard と矛盾しない → Hard は親の値
    let c = create(&fx, json!({"soft_deadline": "2026-12-15T00:00:00Z"})).await;
    assert_eq!(ts(&c["soft_deadline"]), at("2026-12-15T00:00:00Z"));
    assert_eq!(ts(&c["hard_deadline"]), at(P_HARD));
}

#[tokio::test]
async fn later_parent_changes_do_not_reach_subtask() {
    let fx = setup().await;
    let child = create(&fx, json!({})).await;
    let child_id = child["id"].as_str().unwrap();

    let res = fx
        .app
        .put_json_with_session(
            &format!("{}/{}", fx.base, fx.parent_id),
            json!({
                "soft_deadline": "2026-12-11T00:00:00Z",
                "clear_planned_start_on": true,
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    let got = fetch(&fx, child_id).await;
    assert_eq!(ts(&got["soft_deadline"]), at(P_SOFT));
    assert_eq!(ts(&got["hard_deadline"]), at(P_HARD));
    assert_eq!(got["planned_start_on"], P_START);

    // 子の側で解除できる
    let res = fx
        .app
        .put_json_with_session(
            &format!("{}/{}", fx.base, child_id),
            json!({
                "clear_soft_deadline": true,
                "clear_hard_deadline": true,
                "clear_planned_start_on": true,
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let got = fetch(&fx, child_id).await;
    assert!(got["soft_deadline"].is_null());
    assert!(got["hard_deadline"].is_null());
    assert!(got["planned_start_on"].is_null());
}

#[tokio::test]
async fn no_parent_or_reparent_does_not_inherit() {
    let fx = setup().await;
    let res = fx
        .app
        .post_json_with_session(
            &fx.base,
            json!({"title": "orphan", "status_id": fx.status_id}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let orphan: Value = res.json().await.unwrap();
    assert!(orphan["soft_deadline"].is_null());
    assert!(orphan["hard_deadline"].is_null());
    assert!(orphan["planned_start_on"].is_null());

    let res = fx
        .app
        .put_json_with_session(
            &format!("{}/{}", fx.base, orphan["id"].as_str().unwrap()),
            json!({"parent_task_id": fx.parent_id}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::OK);
    let got = fetch(&fx, orphan["id"].as_str().unwrap()).await;
    assert_eq!(got["parent_task_id"], fx.parent_id.as_str());
    assert!(got["soft_deadline"].is_null());
    assert!(got["hard_deadline"].is_null());
    assert!(got["planned_start_on"].is_null());
}

#[tokio::test]
async fn bulk_create_inherits_per_item() {
    let fx = setup().await;
    let res = fx
        .app
        .post_json_with_session(
            &format!("{}/bulk-create", fx.base),
            json!({"tasks": [
                {"title": "b0", "status_id": fx.status_id, "parent_task_id": fx.parent_id},
                {"title": "b1", "status_id": fx.status_id, "parent_task_id": fx.parent_id,
                 "planned_start_on": "2026-12-04"},
            ]}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: Value = res.json().await.unwrap();
    let t = body["tasks"].as_array().unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(ts(&t[0]["soft_deadline"]), at(P_SOFT));
    assert_eq!(ts(&t[0]["hard_deadline"]), at(P_HARD));
    assert_eq!(t[0]["planned_start_on"], P_START);
    assert_eq!(ts(&t[1]["soft_deadline"]), at(P_SOFT));
    assert_eq!(ts(&t[1]["hard_deadline"]), at(P_HARD));
    assert_eq!(t[1]["planned_start_on"], "2026-12-04");
}
