// POST /v1/tenants/{t}/projects/{p}/tasks/bulk-create の統合テスト（TASK-480）。
// 受け入れ条件:
// - 2 件作成 → 201、seq_id が連番、tasks が入力順、task_activities の task_created が 2 件、
//   Webhook 1 本で webhook_deliveries が 2 行
// - 他人を担当にした要素で notifications が作られる
// - プロジェクトメンバー → 201 / 非メンバー → 403
// - read:task だけの PAT → 403
// - 100 件 → 201、101 件 → 400、0 件 → 400
// - 2 件目の status_id が他プロジェクト → 400 で本文に tasks[1]、1 件目も作られない（全件ロールバック）
// - 2 件目で soft_deadline >= hard_deadline → 400
// - 同時実行: 一括作成中に単体作成を走らせて両方成功し seq_id が重複しない

use crate::common::{TestApp, TestTenantProject, TestUser};
use axum::http::StatusCode;
use entity::scopes::Scope;
use entity::{notifications, task_activities, tasks, webhook_deliveries};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::{Value, json};
use uuid::Uuid;

struct Fx {
    app: TestApp,
    tp: TestTenantProject,
    owner: TestUser,
    status_id: String,
    base: String,
}

async fn setup() -> Fx {
    let mut app = TestApp::new_with_webhook_loopback(true).await;
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
    Fx {
        app,
        tp,
        owner,
        status_id,
        base,
    }
}

fn items(status_id: &str, n: usize) -> Vec<Value> {
    (0..n)
        .map(|i| json!({"title": format!("bulk {i}"), "status_id": status_id}))
        .collect()
}

async fn count_tasks(fx: &Fx) -> u64 {
    tasks::Entity::find()
        .filter(tasks::Column::ProjectId.eq(fx.tp.project_id))
        .count(&fx.app.state.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn creates_in_order_with_activities_and_webhooks() {
    let fx = setup().await;
    let hook = fx
        .app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/webhooks",
                fx.tp.tenant_id, fx.tp.project_id
            ),
            json!({"url": "https://203.0.113.10/api/webhooks/1/x", "secret": "0123456789abcdef-secret",
                   "events": ["task.created"], "format": "json"}),
        )
        .await;
    assert_eq!(hook.status(), StatusCode::CREATED);
    let hook: Value = hook.json().await.unwrap();
    let hook_id: Uuid = hook["id"].as_str().unwrap().parse().unwrap();

    let res = fx
        .app
        .post_json_with_session(
            &format!("{}/bulk-create", fx.base),
            json!({"tasks": items(&fx.status_id, 2)}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: Value = res.json().await.unwrap();
    let ts = body["tasks"].as_array().unwrap();
    assert_eq!(ts.len(), 2);
    assert_eq!(ts[0]["title"], "bulk 0");
    assert_eq!(ts[1]["title"], "bulk 1");
    assert_eq!(
        ts[1]["seq_id"].as_i64().unwrap(),
        ts[0]["seq_id"].as_i64().unwrap() + 1
    );

    let ids: Vec<Uuid> = ts
        .iter()
        .map(|t| t["id"].as_str().unwrap().parse().unwrap())
        .collect();
    let acts = task_activities::Entity::find()
        .filter(task_activities::Column::TaskId.is_in(ids))
        .filter(task_activities::Column::EventType.eq("task_created"))
        .count(&fx.app.state.db)
        .await
        .unwrap();
    assert_eq!(acts, 2);
    let deliveries = webhook_deliveries::Entity::find()
        .filter(webhook_deliveries::Column::WebhookId.eq(hook_id))
        .filter(webhook_deliveries::Column::Event.eq("task.created"))
        .count(&fx.app.state.db)
        .await
        .unwrap();
    assert_eq!(deliveries, 2);
}

#[tokio::test]
async fn assigning_someone_else_creates_notifications() {
    let fx = setup().await;
    let other = fx.app.insert_user_default().await;
    crate::common::ensure_tenant_member_for_project(&fx.app.state.db, fx.tp.project_id, other.id)
        .await;
    let added = fx
        .app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/members",
                fx.tp.tenant_id, fx.tp.project_id
            ),
            json!({"user_id": other.id, "role": "Member"}),
        )
        .await;
    assert_eq!(added.status(), StatusCode::CREATED);
    let res = fx
        .app
        .post_json_with_session(
            &format!("{}/bulk-create", fx.base),
            json!({"tasks": [
                {"title": "a", "status_id": fx.status_id, "assignees": [{"user_id": other.id, "role": "assignee"}]},
                {"title": "b", "status_id": fx.status_id}
            ]}),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let n = notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(other.id))
        .count(&fx.app.state.db)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn member_allowed_outsider_forbidden_and_scope_enforced() {
    let mut fx = setup().await;
    let member = fx.app.insert_user_default().await;
    crate::common::ensure_tenant_member_for_project(&fx.app.state.db, fx.tp.project_id, member.id)
        .await;
    let added = fx
        .app
        .post_json_with_session(
            &format!(
                "/v1/tenants/{}/projects/{}/members",
                fx.tp.tenant_id, fx.tp.project_id
            ),
            json!({"user_id": member.id, "role": "Member"}),
        )
        .await;
    assert_eq!(added.status(), StatusCode::CREATED);
    let outsider = fx.app.insert_user_default().await;
    let url = format!("{}/bulk-create", fx.base);
    let body = json!({"tasks": items(&fx.status_id, 1)});

    fx.app.reset_session_client();
    fx.app
        .login_session_no_content(&member.email, &member.password)
        .await;
    assert_eq!(
        fx.app
            .post_json_with_session(&url, body.clone())
            .await
            .status(),
        StatusCode::CREATED
    );

    fx.app.reset_session_client();
    fx.app
        .login_session_no_content(&outsider.email, &outsider.password)
        .await;
    let before = count_tasks(&fx).await;
    assert_eq!(
        fx.app
            .post_json_with_session(&url, body.clone())
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(count_tasks(&fx).await, before);

    let read_only = fx
        .app
        .insert_pat(fx.owner.id, fx.tp.tenant_id, vec![Scope::ReadTask], None)
        .await;
    assert_eq!(
        fx.app
            .post_json_with_bearer(&url, body.clone(), &read_only)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let write = fx
        .app
        .insert_pat(fx.owner.id, fx.tp.tenant_id, vec![Scope::WriteTask], None)
        .await;
    assert_eq!(
        fx.app
            .post_json_with_bearer(&url, body, &write)
            .await
            .status(),
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn size_limits() {
    let fx = setup().await;
    let url = format!("{}/bulk-create", fx.base);
    let started = std::time::Instant::now();
    let r = fx
        .app
        .post_json_with_session(&url, json!({"tasks": items(&fx.status_id, 100)}))
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["tasks"].as_array().unwrap().len(), 100);
    eprintln!("bulk-create 100 items took {:?}", started.elapsed());
    for n in [101, 0] {
        let before = count_tasks(&fx).await;
        let r = fx
            .app
            .post_json_with_session(&url, json!({"tasks": items(&fx.status_id, n)}))
            .await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "n={n}");
        assert_eq!(count_tasks(&fx).await, before);
    }
}

#[tokio::test]
async fn failure_rolls_back_everything_and_names_the_index() {
    let fx = setup().await;
    let url = format!("{}/bulk-create", fx.base);
    let other_project = crate::common::insert_extra_project(&fx.app, fx.tp.tenant_id).await;
    let other_tp = TestTenantProject {
        tenant_id: fx.tp.tenant_id,
        project_id: other_project,
    };
    let foreign_status = crate::common::create_status(&fx.app, &other_tp, "Todo", false).await;

    let before = count_tasks(&fx).await;
    let r = fx
        .app
        .post_json_with_session(
            &url,
            json!({"tasks": [
                {"title": "ok", "status_id": fx.status_id},
                {"title": "bad", "status_id": foreign_status}
            ]}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let text = r.text().await.unwrap();
    assert!(text.contains("tasks[1]"), "body: {text}");
    assert_eq!(count_tasks(&fx).await, before, "1 件目も作られない");

    let r = fx
        .app
        .post_json_with_session(
            &url,
            json!({"tasks": [
                {"title": "ok", "status_id": fx.status_id},
                {"title": "bad", "status_id": fx.status_id,
                 "soft_deadline": "2026-12-02T00:00:00Z", "hard_deadline": "2026-12-01T00:00:00Z"}
            ]}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let text = r.text().await.unwrap();
    assert!(text.contains("tasks[1]"), "body: {text}");
    assert_eq!(count_tasks(&fx).await, before);
}

/// 一括作成（カウンター → sprint）と単体作成を sprint 付きで同時に流してもデッドロックせず、
/// seq_id が重複しない。
#[tokio::test]
async fn concurrent_with_single_create_has_unique_seq_ids() {
    let fx = setup().await;
    let path = format!(
        "/v1/tenants/{}/projects/{}",
        fx.tp.tenant_id, fx.tp.project_id
    );
    let r = fx
        .app
        .post_json_with_session(
            &format!("{path}/sprints"),
            json!({"name":"planning","goal":"test","start_date":"2026-09-01","end_date":"2026-09-14"}),
        )
        .await;
    assert_eq!(r.status(), StatusCode::CREATED);
    let sprint: Value = r.json().await.unwrap();
    let sprint_id = sprint["id"].as_str().unwrap().to_string();
    let r = fx
        .app
        .post_json_with_session(&format!("{path}/sprints/{sprint_id}/start"), json!({}))
        .await;
    assert_eq!(r.status(), StatusCode::OK);

    // カウンター行が無い初回の採番は INSERT 同士が競合する（既存の挙動。単体作成も同じ）ので、
    // 先に 1 件作って行を用意してから同時実行を試す。
    let seed = fx
        .app
        .post_json_with_session(
            &fx.base,
            json!({"title": "seed", "status_id": fx.status_id}),
        )
        .await;
    assert_eq!(seed.status(), StatusCode::CREATED);

    let mut handles = Vec::new();
    for round in 0..4 {
        let client = fx.app.session_client();
        let base = format!("{}{}", fx.app.base_url(), fx.base);
        let status_id = fx.status_id.clone();
        let sid = sprint_id.clone();
        handles.push(tokio::spawn(async move {
            let tasks: Vec<Value> = (0..5)
                .map(|i| json!({"title": format!("b{round}-{i}"), "status_id": status_id, "sprint_id": sid}))
                .collect();
            let r = client
                .post(format!("{base}/bulk-create"))
                .json(&json!({ "tasks": tasks }))
                .send()
                .await
                .unwrap();
            (r.status(), r.json::<Value>().await.unwrap())
        }));
        let client = fx.app.session_client();
        let base = format!("{}{}", fx.app.base_url(), fx.base);
        let status_id = fx.status_id.clone();
        let sid = sprint_id.clone();
        handles.push(tokio::spawn(async move {
            let r = client
                .post(&base)
                .json(&json!({"title": format!("s{round}"), "status_id": status_id, "sprint_id": sid}))
                .send()
                .await
                .unwrap();
            (r.status(), json!({"tasks": [r.json::<Value>().await.unwrap()]}))
        }));
    }
    let mut seqs = Vec::new();
    for h in handles {
        let (status, body) = tokio::time::timeout(std::time::Duration::from_secs(30), h)
            .await
            .expect("deadlock?")
            .unwrap();
        assert_eq!(status, StatusCode::CREATED, "{body}");
        for t in body["tasks"].as_array().unwrap() {
            seqs.push(t["seq_id"].as_i64().unwrap());
        }
    }
    let total = seqs.len();
    assert_eq!(total, 24);
    seqs.sort();
    seqs.dedup();
    assert_eq!(seqs.len(), total, "seq_id が重複した");
}

/// sprint を [B, A]（UUID は A < B）の順で含む一括作成と、A を完了して未完了を B へ移す
/// 処理を同時に流しても、どちらも 5xx（デッドロック）にならない。
#[tokio::test]
async fn bulk_create_does_not_deadlock_with_sprint_complete() {
    let fx = setup().await;
    let path = format!(
        "/v1/tenants/{}/projects/{}",
        fx.tp.tenant_id, fx.tp.project_id
    );
    // カウンター行を用意しておく（初回採番の競合は既存の挙動）。
    let seed = fx
        .app
        .post_json_with_session(
            &fx.base,
            json!({"title": "seed", "status_id": fx.status_id}),
        )
        .await;
    assert_eq!(seed.status(), StatusCode::CREATED);

    for round in 0..8 {
        let mut ids = Vec::new();
        for n in 0..2 {
            let r = fx
                .app
                .post_json_with_session(
                    &format!("{path}/sprints"),
                    json!({"name": format!("r{round}-{n}"), "start_date": "2026-09-01", "end_date": "2026-09-14"}),
                )
                .await;
            assert_eq!(r.status(), StatusCode::CREATED);
            let v: Value = r.json().await.unwrap();
            ids.push(v["id"].as_str().unwrap().to_string());
        }
        ids.sort();
        let (a, b) = (ids[0].clone(), ids[1].clone());
        let r = fx
            .app
            .post_json_with_session(&format!("{path}/sprints/{a}/start"), json!({}))
            .await;
        assert_eq!(r.status(), StatusCode::OK);

        let bulk_client = fx.app.session_client();
        let bulk_url = format!("{}{}/bulk-create", fx.app.base_url(), fx.base);
        let tasks: Vec<Value> = [&b, &a]
            .iter()
            .map(|sid| json!({"title": "x", "status_id": fx.status_id, "sprint_id": sid}))
            .collect();
        let complete_client = fx.app.session_client();
        let complete_url = format!("{}{}/sprints/{a}/complete", fx.app.base_url(), path);
        let bulk = tokio::spawn(async move {
            bulk_client
                .post(bulk_url)
                .json(&json!({ "tasks": tasks }))
                .send()
                .await
                .unwrap()
                .status()
        });
        let complete = tokio::spawn(async move {
            complete_client
                .post(complete_url)
                .json(&json!({"move_incomplete_to_sprint_id": b}))
                .send()
                .await
                .unwrap()
                .status()
        });
        for h in [bulk, complete] {
            let status = tokio::time::timeout(std::time::Duration::from_secs(30), h)
                .await
                .expect("deadlock?")
                .unwrap();
            assert!(!status.is_server_error(), "round {round}: {status}");
        }
    }
}
