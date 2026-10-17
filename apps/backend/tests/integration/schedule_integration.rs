use std::collections::HashSet;
use std::sync::atomic::{AtomicI32, Ordering};

use crate::common::{
    TestApp, TestTenantProject, TestUser, ensure_tenant_member_for_project, execute_sql,
    insert_extra_project,
};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use entity::{milestones, project_statuses, scopes::Scope, sprints, task_assignees, tasks};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::Value;
use uuid::Uuid;

static SEQ: AtomicI32 = AtomicI32::new(1);

const RANGE: &str = "from=2026-10-12&to=2026-10-25";

fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
}
fn at(day: u32) -> DateTime<Utc> {
    d(day).and_hms_opt(0, 0, 0).unwrap().and_utc()
}
fn url(tenant: Uuid, query: &str) -> String {
    format!("/v1/tenants/{tenant}/schedule?{query}")
}
async fn get(app: &TestApp, tenant: Uuid, query: &str) -> Value {
    let res = app.get_with_session(&url(tenant, query)).await;
    if res.status() != StatusCode::OK {
        panic!("schedule: {} {}", res.status(), res.text().await.unwrap());
    }
    res.json().await.unwrap()
}
async fn code(app: &TestApp, tenant: Uuid, query: &str) -> StatusCode {
    app.get_with_session(&url(tenant, query)).await.status()
}
fn ids(body: &Value) -> Vec<String> {
    body["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_owned())
        .collect()
}
fn set(ids: &[Uuid]) -> HashSet<String> {
    ids.iter().map(Uuid::to_string).collect()
}
fn got(body: &Value) -> HashSet<String> {
    ids(body).into_iter().collect()
}
async fn status(app: &TestApp, project: Uuid, done: bool) -> Uuid {
    let id = Uuid::new_v4();
    project_statuses::ActiveModel {
        id: Set(id),
        project_id: Set(project),
        name: Set(if done { "Done" } else { "Todo" }.into()),
        color: Set("#123456".into()),
        position: Set(if done { 1 } else { 0 }),
        is_default: Set(!done),
        is_done_state: Set(done),
        is_default_done: Set(done),
        created_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .unwrap();
    id
}
#[allow(clippy::too_many_arguments)]
async fn task(
    app: &TestApp,
    project: Uuid,
    status: Uuid,
    creator: Uuid,
    planned: Option<NaiveDate>,
    soft: Option<DateTime<Utc>>,
    hard: Option<DateTime<Utc>>,
) -> Uuid {
    let id = Uuid::new_v4();
    let now = Utc::now();
    tasks::ActiveModel {
        id: Set(id),
        project_id: Set(project),
        seq_id: Set(SEQ.fetch_add(1, Ordering::Relaxed)),
        title: Set("task".into()),
        description: Set(None),
        status_id: Set(status),
        priority: Set(tasks::TaskPriority::Medium),
        progress_pct: Set(0),
        parent_task_id: Set(None),
        milestone_id: Set(None),
        soft_deadline: Set(soft.map(Into::into)),
        hard_deadline: Set(hard.map(Into::into)),
        planned_start_on: Set(planned),
        estimated_minutes: Set(None),
        is_archived: Set(false),
        created_by: Set(creator),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        deleted_at: Set(None),
        sprint_id: Set(None),
        completed_at: Set(None),
    }
    .insert(&app.state.db)
    .await
    .unwrap();
    id
}
async fn assign(app: &TestApp, task: Uuid, user: Uuid, role: &str) {
    task_assignees::ActiveModel {
        id: Set(Uuid::new_v4()),
        task_id: Set(task),
        user_id: Set(user),
        role: Set(role.into()),
        assigned_at: Set(Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .unwrap();
}
async fn setup(app: &mut TestApp) -> (TestUser, TestTenantProject, Uuid, Uuid) {
    let user = app.insert_user_default().await;
    app.login_session_no_content(&user.email, &user.password)
        .await;
    let tp = app.insert_tenant_project(user.id).await;
    let todo = status(app, tp.project_id, false).await;
    let done = status(app, tp.project_id, true).await;
    (user, tp, todo, done)
}
/// members はそのページの担当者を重複なく集めたものになっている
fn assert_members_match_assignees(body: &Value) {
    let assignees: HashSet<&str> = body["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t["assignees"].as_array().unwrap())
        .map(|a| a["user_id"].as_str().unwrap())
        .collect();
    let members: Vec<&str> = body["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(members.len(), assignees.len(), "members に重複がある");
    assert_eq!(members.into_iter().collect::<HashSet<_>>(), assignees);
}

#[tokio::test]
async fn includes_tasks_whose_span_overlaps_the_range_inclusively() {
    let mut app = TestApp::new().await;
    let (user, tp, todo, _) = setup(&mut app).await;
    let p = tp.project_id;
    let u = user.id;
    // 端に一致するものは入り、1 日外れたら入らない
    let planned_to = task(&app, p, todo, u, Some(d(25)), None, None).await;
    let planned_after = task(&app, p, todo, u, Some(d(26)), None, None).await;
    let soft_from = task(&app, p, todo, u, None, Some(at(12)), None).await;
    let soft_before = task(&app, p, todo, u, None, Some(at(11)), None).await;
    let hard_from = task(&app, p, todo, u, None, None, Some(at(12))).await;
    let hard_before = task(&app, p, todo, u, None, None, Some(at(11))).await;
    let _undated = task(&app, p, todo, u, None, None, None).await;
    // 期間をまたぐもの（始点が from より前、終点が from ちょうど）
    let spanning = task(&app, p, todo, u, Some(d(1)), Some(at(12)), None).await;
    let start_at_to = task(&app, p, todo, u, Some(d(25)), Some(at(30)), None).await;
    let start_after = task(&app, p, todo, u, Some(d(26)), Some(at(30)), None).await;
    // 終点は Soft が Hard より優先する
    let soft_wins = task(&app, p, todo, u, None, Some(at(11)), Some(at(20))).await;
    let archived = task(&app, p, todo, u, Some(d(20)), None, None).await;
    let deleted = task(&app, p, todo, u, Some(d(20)), None, None).await;
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET is_archived = true WHERE id = $1",
        vec![archived.into()],
    )
    .await;
    execute_sql(
        &app.state.db,
        "UPDATE tasks SET deleted_at = now() WHERE id = $1",
        vec![deleted.into()],
    )
    .await;

    let body = get(&app, tp.tenant_id, RANGE).await;
    assert_eq!(
        got(&body),
        set(&[planned_to, soft_from, hard_from, spanning, start_at_to])
    );
    for excluded in [
        planned_after,
        soft_before,
        hard_before,
        start_after,
        soft_wins,
    ] {
        assert!(!got(&body).contains(&excluded.to_string()));
    }
    // 並びは始点の昇順
    assert_eq!(ids(&body)[0], spanning.to_string());
    let starts: Vec<_> = body["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            t["planned_start_on"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| "2026-10-12".into())
        })
        .collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted);
    assert_eq!(body["range"]["from"], "2026-10-12");
    assert_eq!(body["range"]["to"], "2026-10-25");
    assert_eq!(body["range"]["timezone"], "UTC");
    assert!(body["next_cursor"].is_null());
}

#[tokio::test]
async fn timezone_moves_the_deadline_date() {
    let mut app = TestApp::new().await;
    let (user, tp, todo, _) = setup(&mut app).await;
    let soft = "2026-10-20T15:30:00Z".parse::<DateTime<Utc>>().unwrap();
    let id = task(&app, tp.project_id, todo, user.id, None, Some(soft), None).await;
    let tokyo21 = get(
        &app,
        tp.tenant_id,
        "from=2026-10-21&to=2026-10-21&timezone=Asia%2FTokyo",
    )
    .await;
    assert_eq!(got(&tokyo21), set(&[id]));
    assert_eq!(tokyo21["range"]["timezone"], "Asia/Tokyo");
    let utc21 = get(&app, tp.tenant_id, "from=2026-10-21&to=2026-10-21").await;
    assert!(ids(&utc21).is_empty());
    let utc20 = get(&app, tp.tenant_id, "from=2026-10-20&to=2026-10-20").await;
    assert_eq!(got(&utc20), set(&[id]));
    let tokyo20 = get(
        &app,
        tp.tenant_id,
        "from=2026-10-20&to=2026-10-20&timezone=Asia%2FTokyo",
    )
    .await;
    assert!(ids(&tokyo20).is_empty());
}

#[tokio::test]
async fn task_starting_after_its_deadline_spans_deadline_to_start() {
    let mut app = TestApp::new().await;
    let (user, tp, todo, _) = setup(&mut app).await;
    // 開始予定日 10/20・締切 10/10 の逆転したタスクは 10/10〜10/20 として扱う
    let inverted = task(
        &app,
        tp.project_id,
        todo,
        user.id,
        Some(d(20)),
        Some(at(10)),
        None,
    )
    .await;
    for day in [10, 15, 20] {
        let body = get(
            &app,
            tp.tenant_id,
            &format!("from=2026-10-{day}&to=2026-10-{day}"),
        )
        .await;
        assert_eq!(got(&body), set(&[inverted]), "10/{day}");
    }
    for day in ["09", "21"] {
        let body = get(
            &app,
            tp.tenant_id,
            &format!("from=2026-10-{day}&to=2026-10-{day}"),
        )
        .await;
        assert!(ids(&body).is_empty(), "10/{day}");
    }
    // 並びも早い方（締切の 10/10）を始点として扱う
    let later = task(&app, tp.project_id, todo, user.id, Some(d(12)), None, None).await;
    let body = get(&app, tp.tenant_id, "from=2026-10-01&to=2026-10-31").await;
    assert_eq!(ids(&body), vec![inverted.to_string(), later.to_string()]);
}

#[tokio::test]
async fn rejects_invalid_ranges_and_parameters() {
    let mut app = TestApp::new().await;
    let (_, tp, _, _) = setup(&mut app).await;
    let t = tp.tenant_id;
    for query in [
        "from=2026-10-25&to=2026-10-12",
        // 2026-01-01 + 93 日
        "from=2026-01-01&to=2026-04-04",
        "from=2026-10-12&to=2026-10-25&timezone=Mars%2FOlympus",
        "from=2026-10-12&to=2026-10-25&limit=501",
        "from=2026-10-12&to=2026-10-25&limit=0",
        "from=2026-10-12&to=2026-10-25&cursor=!!!",
        "to=2026-10-25",
        "from=2026-10-12&to=2026-10-25&project_id=not-a-uuid",
    ] {
        assert_eq!(
            code(&app, t, query).await,
            StatusCode::BAD_REQUEST,
            "{query}"
        );
    }
    // 対照: 92 日ちょうど・上限ちょうどの limit は通る
    for query in [
        "from=2026-01-01&to=2026-04-03",
        "from=2026-10-12&to=2026-10-25&limit=500",
        "from=2026-10-12&to=2026-10-12",
    ] {
        assert_eq!(code(&app, t, query).await, StatusCode::OK, "{query}");
    }
    app.reset_session_client();
    assert_eq!(
        app.get(&url(t, RANGE)).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn filters_by_done_state_project_and_assignee() {
    let mut app = TestApp::new().await;
    let (owner, tp, todo, done) = setup(&mut app).await;
    let other_project = insert_extra_project(&app, tp.tenant_id).await;
    let other_todo = status(&app, other_project, false).await;
    let alice = app.insert_user_default().await;
    let bob = app.insert_user_default().await;
    let open = task(&app, tp.project_id, todo, owner.id, Some(d(15)), None, None).await;
    let finished = task(&app, tp.project_id, done, owner.id, Some(d(16)), None, None).await;
    let elsewhere = task(
        &app,
        other_project,
        other_todo,
        owner.id,
        Some(d(17)),
        None,
        None,
    )
    .await;
    assign(&app, open, alice.id, "primary").await;
    assign(&app, finished, bob.id, "assignee").await;
    assign(&app, elsewhere, bob.id, "primary").await;
    assign(&app, elsewhere, alice.id, "assignee").await;
    let t = tp.tenant_id;

    let all = get(&app, t, RANGE).await;
    assert_eq!(got(&all), set(&[open, finished, elsewhere]));
    assert_members_match_assignees(&all);
    assert_eq!(all["members"].as_array().unwrap().len(), 2);
    let finished_item = &all["tasks"][1];
    assert_eq!(finished_item["id"], finished.to_string());
    assert_eq!(finished_item["status"]["is_done_state"], true);
    assert_eq!(finished_item["assignees"][0]["role"], "assignee");
    assert_eq!(finished_item["assignees"][0]["user_id"], bob.id.to_string());
    assert_eq!(all["tasks"][0]["status"]["is_done_state"], false);

    let not_done = get(&app, t, &format!("{RANGE}&include_done=false")).await;
    assert_eq!(got(&not_done), set(&[open, elsewhere]));
    let with_done = get(&app, t, &format!("{RANGE}&include_done=true")).await;
    assert_eq!(got(&with_done), set(&[open, finished, elsewhere]));

    let only_other = get(&app, t, &format!("{RANGE}&project_id={other_project}")).await;
    assert_eq!(got(&only_other), set(&[elsewhere]));
    let both = get(
        &app,
        t,
        &format!(
            "{RANGE}&project_id={other_project}&project_id={}",
            tp.project_id
        ),
    )
    .await;
    assert_eq!(got(&both), set(&[open, finished, elsewhere]));
    // 見えない（別テナントの）プロジェクト ID は黙って落とす
    let foreign = app.insert_tenant_project(owner.id).await;
    let dropped = get(
        &app,
        t,
        &format!(
            "{RANGE}&project_id={}&project_id={other_project}",
            foreign.project_id
        ),
    )
    .await;
    assert_eq!(got(&dropped), set(&[elsewhere]));

    let bobs = get(&app, t, &format!("{RANGE}&user_id={}", bob.id)).await;
    assert_eq!(got(&bobs), set(&[finished, elsewhere]));
    let alices = get(&app, t, &format!("{RANGE}&user_id={}", alice.id)).await;
    assert_eq!(got(&alices), set(&[open, elsewhere]));
    let either = get(
        &app,
        t,
        &format!("{RANGE}&user_id={}&user_id={}", alice.id, bob.id),
    )
    .await;
    assert_eq!(got(&either), set(&[open, finished, elsewhere]));
    let nobody = get(&app, t, &format!("{RANGE}&user_id={}", owner.id)).await;
    assert!(ids(&nobody).is_empty());
    assert!(nobody["members"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn follows_project_visibility_guest_scope_and_tokens() {
    let mut app = TestApp::new().await;
    let (owner, tp, todo, _) = setup(&mut app).await;
    let t = tp.tenant_id;
    let viewer = app.insert_user_default().await;
    let insider = app.insert_user_default().await;
    let guest = app.insert_user_default().await;
    let outsider = app.insert_user_default().await;
    app.insert_tenant_project(outsider.id).await;
    execute_sql(
        &app.state.db,
        "INSERT INTO tenant_members (id,tenant_id,user_id,role) VALUES ($1,$2,$3,'Member')",
        vec![Uuid::new_v4().into(), t.into(), viewer.id.into()],
    )
    .await;
    // メンバー指定のあるプロジェクト（viewer は非メンバー）
    let restricted = insert_extra_project(&app, t).await;
    let restricted_todo = status(&app, restricted, false).await;
    ensure_tenant_member_for_project(&app.state.db, restricted, insider.id).await;
    for user in [insider.id, guest.id] {
        execute_sql(
            &app.state.db,
            "INSERT INTO project_members (id,project_id,user_id,role) VALUES ($1,$2,$3,'Member')",
            vec![Uuid::new_v4().into(), restricted.into(), user.into()],
        )
        .await;
    }
    // viewer 自身の個人プロジェクト
    let personal = insert_extra_project(&app, t).await;
    let personal_todo = status(&app, personal, false).await;
    execute_sql(
        &app.state.db,
        "UPDATE projects SET is_personal = true, personal_owner_id = $1 WHERE id = $2",
        vec![viewer.id.into(), personal.into()],
    )
    .await;
    let public_task = task(&app, tp.project_id, todo, owner.id, Some(d(15)), None, None).await;
    let restricted_task = task(
        &app,
        restricted,
        restricted_todo,
        owner.id,
        Some(d(15)),
        None,
        None,
    )
    .await;
    let personal_task = task(
        &app,
        personal,
        personal_todo,
        viewer.id,
        Some(d(15)),
        None,
        None,
    )
    .await;
    // 期間に重なるスプリント・マイルストーンも同じ可視範囲に従う
    for project in [tp.project_id, restricted] {
        sprints::ActiveModel {
            id: Set(Uuid::new_v4()),
            project_id: Set(project),
            name: Set("Sprint".into()),
            goal: Set(None),
            start_date: Set(d(10)),
            end_date: Set(d(20)),
            status: Set(sprints::SprintStatus::Active),
            created_by: Set(owner.id),
            created_at: Set(Utc::now().into()),
            updated_at: Set(Utc::now().into()),
        }
        .insert(&app.state.db)
        .await
        .unwrap();
    }

    // オーナーは全プロジェクト（個人プロジェクトは除く）
    let by_owner = get(&app, t, RANGE).await;
    assert_eq!(got(&by_owner), set(&[public_task, restricted_task]));
    assert_eq!(by_owner["sprints"].as_array().unwrap().len(), 2);

    // テナントメンバー: 公開プロジェクトだけ。個人プロジェクト（自分のものでも）は返らない
    app.login_session_no_content(&viewer.email, &viewer.password)
        .await;
    let by_viewer = get(&app, t, RANGE).await;
    assert_eq!(got(&by_viewer), set(&[public_task]));
    assert!(!got(&by_viewer).contains(&personal_task.to_string()));
    assert_eq!(by_viewer["sprints"].as_array().unwrap().len(), 1);
    assert_eq!(
        by_viewer["sprints"][0]["project_id"],
        tp.project_id.to_string()
    );
    // 見えないプロジェクトを指名しても返らない
    let named = get(&app, t, &format!("{RANGE}&project_id={restricted}")).await;
    assert!(ids(&named).is_empty());
    assert!(named["sprints"].as_array().unwrap().is_empty());

    // メンバー指定のあるプロジェクトのメンバーには見える（対照）
    app.login_session_no_content(&insider.email, &insider.password)
        .await;
    assert_eq!(
        got(&get(&app, t, RANGE).await),
        set(&[public_task, restricted_task])
    );

    // 客分: 明示メンバーのプロジェクトだけ
    app.login_session_no_content(&guest.email, &guest.password)
        .await;
    assert_eq!(got(&get(&app, t, RANGE).await), set(&[restricted_task]));

    // 別テナントのユーザーは 403
    app.login_session_no_content(&outsider.email, &outsider.password)
        .await;
    assert_eq!(code(&app, t, RANGE).await, StatusCode::FORBIDDEN);

    // PAT: read:task が要る。プロジェクト束縛付きは My Tasks と同じく 403
    let no_task = app
        .insert_pat(viewer.id, t, vec![Scope::ReadProject], None)
        .await;
    assert_eq!(
        app.get_with_bearer(&url(t, RANGE), &no_task).await.status(),
        StatusCode::FORBIDDEN
    );
    let read_task = app
        .insert_pat(viewer.id, t, vec![Scope::ReadTask], None)
        .await;
    let res = app.get_with_bearer(&url(t, RANGE), &read_task).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        got(&res.json::<Value>().await.unwrap()),
        set(&[public_task])
    );
    let bound = app
        .insert_pat(
            viewer.id,
            t,
            vec![Scope::ReadTask],
            Some(vec![tp.project_id]),
        )
        .await;
    assert_eq!(
        app.get_with_bearer(&url(t, RANGE), &bound).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn cursor_pages_without_gaps_and_repeats_sprints_and_milestones() {
    let mut app = TestApp::new().await;
    let (owner, tp, todo, _) = setup(&mut app).await;
    let t = tp.tenant_id;
    let p = tp.project_id;
    let alice = app.insert_user_default().await;
    // 2 件が同じ始点（id で順が決まる）
    let first = task(&app, p, todo, owner.id, Some(d(13)), None, None).await;
    let tie_a = task(&app, p, todo, owner.id, Some(d(14)), None, None).await;
    let tie_b = task(&app, p, todo, owner.id, None, Some(at(14)), None).await;
    assign(&app, first, alice.id, "primary").await;
    assign(&app, tie_a, owner.id, "primary").await;
    assign(&app, tie_b, owner.id, "primary").await;
    let sprint = |start: u32, end: u32| sprints::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(p),
        name: Set(format!("Sprint {start}-{end}")),
        goal: Set(None),
        start_date: Set(d(start)),
        end_date: Set(d(end)),
        status: Set(sprints::SprintStatus::Planning),
        created_by: Set(owner.id),
        created_at: Set(Utc::now().into()),
        updated_at: Set(Utc::now().into()),
    };
    let mut sprints_in = Vec::new();
    for (start, end, inside) in [
        (1, 12, true),
        (1, 11, false),
        (25, 31, true),
        (26, 31, false),
    ] {
        let model = sprint(start, end).insert(&app.state.db).await.unwrap();
        if inside {
            sprints_in.push(model.id);
        }
    }
    let mut milestones_in = Vec::new();
    for (day, inside) in [(12, true), (25, true), (11, false), (26, false)] {
        let model = milestones::ActiveModel {
            id: Set(Uuid::new_v4()),
            project_id: Set(p),
            name: Set(format!("M{day}")),
            description: Set(None),
            due_date: Set(d(day)),
            created_by: Set(owner.id),
            created_at: Set(Utc::now().into()),
            updated_at: Set(Utc::now().into()),
        }
        .insert(&app.state.db)
        .await
        .unwrap();
        if inside {
            milestones_in.push(model.id);
        }
    }
    let ids_of = |body: &Value, key: &str| -> HashSet<String> {
        body[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap().to_owned())
            .collect()
    };

    let page1 = get(&app, t, &format!("{RANGE}&limit=2")).await;
    assert_eq!(ids(&page1).len(), 2);
    assert_eq!(ids(&page1)[0], first.to_string());
    assert_members_match_assignees(&page1);
    let cursor = page1["next_cursor"]
        .as_str()
        .expect("next_cursor")
        .to_owned();
    let page2 = get(&app, t, &format!("{RANGE}&limit=2&cursor={cursor}")).await;
    assert_eq!(ids(&page2).len(), 1);
    assert!(page2["next_cursor"].is_null());
    assert_members_match_assignees(&page2);
    assert_eq!(page2["members"][0]["id"], owner.id.to_string());

    let mut all: Vec<_> = ids(&page1).into_iter().chain(ids(&page2)).collect();
    let mut ties = [tie_a.to_string(), tie_b.to_string()];
    ties.sort();
    assert_eq!(all[1..], ties[..]);
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 3);

    for page in [&page1, &page2] {
        assert_eq!(ids_of(page, "sprints"), set(&sprints_in));
        assert_eq!(ids_of(page, "milestones"), set(&milestones_in));
    }
    // ちょうど limit 件なら次ページは無い
    let exact = get(&app, t, &format!("{RANGE}&limit=3")).await;
    assert_eq!(ids(&exact).len(), 3);
    assert!(exact["next_cursor"].is_null());
}
