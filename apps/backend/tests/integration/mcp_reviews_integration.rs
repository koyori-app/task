//! MCP のレビューのツール（`POST /mcp`）。規則は docs/dev/features/mcp.md。
//!
//! - 起票・一覧・集計・状態遷移・引き継ぎが、REST API と同じ結果を返す
//! - `submit_review` は `head_sha` の形、空白だけの `title` / `body`、起票の上限を API を呼ぶ前に確かめる
//! - scope（`read:review` / `write:review`）とプロジェクト束縛の拒否を、対照の成功と組で確かめる

use super::mcp_integration::{Fixture, ToolOutcome, api_pat, call, fixture};
use crate::common::TestApp;
use entity::github_integrations;
use entity::scopes::Scope;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::{Value, json};
use uuid::Uuid;

const HEAD: &str = "60cdd77a1b2c3d4e5f60718293a4b5c6d7e8f901";
const LIMITS: &str = "review does not satisfy the API's limits:";

fn finding(severity: &str, title: &str) -> Value {
    json!({ "severity": severity, "title": title, "body": "本文" })
}

async fn submit(app: &TestApp, token: &str, fx: &Fixture, pr: i32, findings: Value) -> ToolOutcome {
    call(
        app,
        token,
        "submit_review",
        json!({ "project": fx.project_key, "pr": pr, "head_sha": HEAD, "findings": findings }),
    )
    .await
}

async fn rounds(app: &TestApp, token: &str, fx: &Fixture, pr: i32) -> Vec<Value> {
    call(
        app,
        token,
        "list_review_rounds",
        json!({ "project": fx.project_key, "pr": pr }),
    )
    .await
    .json()
    .as_array()
    .expect("rounds")
    .clone()
}

async fn findings(app: &TestApp, token: &str, arguments: Value) -> Vec<Value> {
    call(app, token, "list_review_findings", arguments)
        .await
        .json()
        .as_array()
        .expect("findings")
        .clone()
}

/// プロジェクトを指定のリポジトリへ連携させる（reviews_integration.rs の `link_repo` と同じ形）。
async fn link_repo(app: &TestApp, fx: &Fixture, owner: &str, name: &str) {
    github_integrations::ActiveModel {
        id: Set(Uuid::new_v4()),
        project_id: Set(fx.tp.project_id),
        // 他テストと衝突しない範囲で散らす
        installation_id: Set((Uuid::new_v4().as_u128() % 1_000_000) as i64 + 9_000_000),
        repo_owner: Set(owner.into()),
        repo_name: Set(name.into()),
        access_token_enc: Set("unused".into()),
        token_expires_at: Set(chrono::Utc::now().into()),
        created_by: Set(fx.user.id),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&app.state.db)
    .await
    .expect("insert integration");
}

#[tokio::test]
async fn submit_review_then_read_back() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let created = submit(
        &app,
        &token,
        &fx,
        5,
        json!([
            { "severity": "high", "title": "塞ぐ指摘", "body": "本文", "file": "src/lib.rs", "line": 12 },
            finding("low", "軽い指摘"),
        ]),
    )
    .await
    .json();
    assert_eq!(created["round"], 1, "{created}");
    assert_eq!(created["findings"].as_array().expect("findings").len(), 2);

    let by = |extra: Value| {
        let mut arguments = json!({ "project": fx.project_key, "pr": 5 });
        for (key, value) in extra.as_object().expect("object") {
            arguments[key] = value.clone();
        }
        arguments
    };
    assert_eq!(findings(&app, &token, by(json!({}))).await.len(), 2);
    let high = findings(&app, &token, by(json!({ "severity": ["high"] }))).await;
    assert_eq!(high.len(), 1);
    assert_eq!(high[0]["title"], "塞ぐ指摘");
    assert_eq!(high[0]["file"], "src/lib.rs");
    assert_eq!(high[0]["line"], 12);
    assert_eq!(
        findings(&app, &token, by(json!({ "state": ["fixed"] })))
            .await
            .len(),
        0
    );
    // 複数の値はカンマ区切りで API へ渡る
    assert_eq!(
        findings(
            &app,
            &token,
            by(json!({ "state": ["open", "fixed"], "severity": ["low", "nit"] }))
        )
        .await
        .len(),
        1
    );

    let listed = rounds(&app, &token, &fx, 5).await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["finding_count"], 2);

    let summary = call(
        &app,
        &token,
        "get_review_summary",
        json!({ "project": fx.project_key, "pr": 5 }),
    )
    .await
    .json();
    assert_eq!(summary["rounds"], 1, "{summary}");
    assert_eq!(summary["blocking"], 1, "{summary}");
    assert_eq!(summary["latest_head_sha"], HEAD, "{summary}");
    assert_eq!(summary["gate"], "unlinked", "{summary}");
}

#[tokio::test]
async fn submit_review_accepts_the_limit_and_rejects_one_more() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let many = |count: usize| -> Value {
        (0..count)
            .map(|index| finding("nit", &format!("指摘 {index}")))
            .collect()
    };

    let created = submit(&app, &token, &fx, 5, many(200)).await.json();
    assert_eq!(created["findings"].as_array().expect("findings").len(), 200);

    // 上限の値が文面に残り、状態コードで始まらない（API を呼んでいない）
    let outcome = submit(&app, &token, &fx, 5, many(201)).await;
    assert_eq!(
        outcome.error(),
        format!("{LIMITS}\n  findings: length max: 200")
    );
    assert_eq!(rounds(&app, &token, &fx, 5).await.len(), 1);
}

#[tokio::test]
async fn submit_review_rejects_malformed_input_without_creating() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let outcome = call(
        &app,
        &token,
        "submit_review",
        json!({ "project": fx.project_key, "pr": 5, "head_sha": "60cdd77" }),
    )
    .await;
    assert_eq!(
        outcome.error(),
        "`head_sha` must be the full 40-character commit SHA (got: 60cdd77)"
    );

    let outcome = submit(
        &app,
        &token,
        &fx,
        5,
        json!([
            finding("high", "1 件目"),
            { "severity": "low", "title": "2 件目", "body": "本文", "line": 0 },
        ]),
    )
    .await;
    assert_eq!(
        outcome.error(),
        format!("{LIMITS}\n  findings[1].line: range min: 1")
    );

    let outcome = submit(&app, &token, &fx, 5, json!([finding("high", "   ")])).await;
    assert_eq!(outcome.error(), "findings[0].title is required");
    let outcome = submit(
        &app,
        &token,
        &fx,
        5,
        json!([{ "severity": "high", "title": "題", "body": "\n" }]),
    )
    .await;
    assert_eq!(outcome.error(), "findings[0].body is required");

    let outcome = call(
        &app,
        &token,
        "submit_review",
        json!({ "project": fx.project_key, "pr": 0, "head_sha": HEAD }),
    )
    .await;
    assert_eq!(outcome.error(), format!("{LIMITS}\n  pr: range min: 1"));

    assert!(rounds(&app, &token, &fx, 5).await.is_empty());

    // 対照: 正しい入力は通る。`findings` と `summary` は省ける
    let created = call(
        &app,
        &token,
        "submit_review",
        json!({ "project": fx.project_key, "pr": 5, "head_sha": HEAD }),
    )
    .await
    .json();
    assert_eq!(created["round"], 1, "{created}");
    assert_eq!(created["summary"], "", "{created}");
    assert_eq!(created["findings"], json!([]), "{created}");
}

#[tokio::test]
async fn unknown_severity_is_rejected_by_the_schema() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let outcome = submit(&app, &token, &fx, 5, json!([finding("critical", "題")])).await;
    assert!(
        outcome
            .error()
            .starts_with("failed to deserialize parameters"),
        "{}",
        outcome.text
    );
    assert!(rounds(&app, &token, &fx, 5).await.is_empty());

    // 対照: 綴りが正しければ通る
    let created = submit(&app, &token, &fx, 5, json!([finding("nit", "題")]))
        .await
        .json();
    assert_eq!(created["findings"][0]["severity"], "nit", "{created}");
}

#[tokio::test]
async fn update_review_finding_moves_the_state() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let created = submit(
        &app,
        &token,
        &fx,
        5,
        json!([finding("high", "直す"), finding("high", "繰り延べたい")]),
    )
    .await
    .json();
    let id = |index: usize| {
        created["findings"][index]["id"]
            .as_str()
            .expect("id")
            .to_string()
    };
    let update = |finding_id: String, state: &'static str| {
        let arguments = json!({
            "project": fx.project_key,
            "finding_id": finding_id,
            "state": state,
            "note": "直した",
        });
        call(&app, &token, "update_review_finding", arguments)
    };

    let fixed = update(id(0), "fixed").await.json();
    assert_eq!(fixed["state"], "fixed", "{fixed}");
    let transitions = fixed["transitions"].as_array().expect("transitions");
    assert_eq!(transitions.len(), 2, "{fixed}");
    assert!(
        transitions
            .iter()
            .any(|transition| transition["to_state"] == "fixed" && transition["note"] == "直した"),
        "{fixed}"
    );

    assert_eq!(
        update(id(1), "deferred").await.error(),
        "409 high の指摘は繰り延べられません（繰り延べは low / nit のみ。マージ前に解消するか、指摘自体を取り下げてください）"
    );
    assert_eq!(
        update("abc".into(), "fixed").await.error(),
        "finding_id must be a UUID"
    );
    assert_eq!(
        update(Uuid::new_v4().to_string(), "fixed").await.error(),
        "404 not-found"
    );
}

#[tokio::test]
async fn carry_over_review_copies_unresolved_findings() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    link_repo(&app, &fx, "acme", "app").await;
    let source = submit(&app, &token, &fx, 1, json!([finding("high", "未解決")]))
        .await
        .json();
    let carry = |from_pr: i32, to_pr: i32| {
        let arguments = json!({ "project": fx.project_key, "from_pr": from_pr, "to_pr": to_pr });
        call(&app, &token, "carry_over_review", arguments)
    };

    let carried = carry(1, 2).await.json();
    assert_eq!(
        carried["rounds"].as_array().expect("rounds").len(),
        1,
        "{carried}"
    );
    let copies = findings(&app, &token, json!({ "project": fx.project_key, "pr": 2 })).await;
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0]["title"], "未解決");
    assert_eq!(
        copies[0]["carried_from_finding_id"],
        source["findings"][0]["id"]
    );

    assert_eq!(
        carry(2, 2).await.error(),
        "400 引き継ぎ元と引き継ぎ先が同じ PR です"
    );
    assert_eq!(
        carry(8, 2).await.error(),
        "409 PR #8 にレビューラウンドがありません（PR 番号と from_repo を確かめてください）"
    );
}

#[tokio::test]
async fn blank_repo_is_read_as_omitted() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    link_repo(&app, &fx, "acme", "app").await;
    submit(&app, &token, &fx, 5, json!([finding("high", "題")]))
        .await
        .json();

    // 空文字を「連携前のラウンド」と読むと、どちらも 0 件になる
    for repo in [json!(""), json!("acme/app"), Value::Null] {
        let arguments = json!({ "project": fx.project_key, "pr": 5, "repo": repo });
        assert_eq!(
            findings(&app, &token, arguments.clone()).await.len(),
            1,
            "{repo}"
        );
        let summary = call(&app, &token, "get_review_summary", arguments.clone())
            .await
            .json();
        assert_eq!(summary["rounds"], 1, "{repo}: {summary}");
        let listed = call(&app, &token, "list_review_rounds", arguments)
            .await
            .json();
        assert_eq!(listed.as_array().expect("rounds").len(), 1, "{repo}");
    }

    let outcome = call(
        &app,
        &token,
        "list_review_findings",
        json!({ "project": fx.project_key, "pr": 5, "repo": "acme" }),
    )
    .await;
    assert_eq!(
        outcome.error(),
        "400 repo は owner/name の形で指定してください（受け取った値: acme）"
    );

    // `from_repo` の空文字も省略と読む（現在の連携先から引き継ぐ）
    let carried = call(
        &app,
        &token,
        "carry_over_review",
        json!({ "project": fx.project_key, "from_pr": 5, "to_pr": 6, "from_repo": "" }),
    )
    .await
    .json();
    assert_eq!(
        carried["rounds"].as_array().expect("rounds").len(),
        1,
        "{carried}"
    );
}

#[tokio::test]
async fn review_tools_follow_the_review_scopes() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let pat = |scopes: Vec<Scope>| app.insert_pat(fx.user.id, fx.tp.tenant_id, scopes, None);
    let reader = pat(vec![Scope::ReadProject, Scope::ReadReview]).await;
    let writer = pat(vec![Scope::ReadProject, Scope::WriteReview]).await;
    let tasks_only = pat(vec![Scope::ReadProject, Scope::ReadTask]).await;
    let list = json!({ "project": fx.project_key, "pr": 5 });

    assert_eq!(
        submit(&app, &reader, &fx, 5, json!([])).await.error(),
        "403 forbidden"
    );
    for tool in [
        "list_review_findings",
        "list_review_rounds",
        "get_review_summary",
    ] {
        let outcome = call(&app, &tasks_only, tool, list.clone()).await;
        assert_eq!(outcome.error(), "403 forbidden", "{tool}");
        // 対照: `read:review` があれば通る
        call(&app, &reader, tool, list.clone()).await.json();
    }
    assert_eq!(
        call(
            &app,
            &reader,
            "carry_over_review",
            json!({ "project": fx.project_key, "from_pr": 5, "to_pr": 6 }),
        )
        .await
        .error(),
        "403 forbidden"
    );

    // 対照: `write:review` は起票も、読み取りも、状態遷移も通る
    let created = submit(&app, &writer, &fx, 5, json!([finding("low", "題")]))
        .await
        .json();
    assert_eq!(findings(&app, &writer, list.clone()).await.len(), 1);
    let finding_id = created["findings"][0]["id"].clone();
    let update = json!({ "project": fx.project_key, "finding_id": finding_id, "state": "fixed" });
    assert_eq!(
        call(&app, &reader, "update_review_finding", update.clone())
            .await
            .error(),
        "403 forbidden"
    );
    let fixed = call(&app, &writer, "update_review_finding", update)
        .await
        .json();
    assert_eq!(fixed["state"], "fixed", "{fixed}");
}

#[tokio::test]
async fn review_tools_respect_allowed_project_ids() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let other_project = crate::common::insert_extra_project(&app, fx.tp.tenant_id).await;
    let scoped = app
        .insert_pat(
            fx.user.id,
            fx.tp.tenant_id,
            vec![Scope::Api],
            Some(vec![fx.tp.project_id]),
        )
        .await;

    let outcome = call(
        &app,
        &scoped,
        "list_review_findings",
        json!({ "project": other_project.to_string(), "pr": 5 }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");
    let outcome = call(
        &app,
        &scoped,
        "submit_review",
        json!({ "project": other_project.to_string(), "pr": 5, "head_sha": HEAD }),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");

    // 対照: 許可したプロジェクトは通る
    assert!(
        findings(
            &app,
            &scoped,
            json!({ "project": fx.tp.project_id.to_string(), "pr": 5 }),
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn finding_of_another_project_is_not_reachable_through_this_project() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let other_project = crate::common::insert_extra_project(&app, fx.tp.tenant_id).await;
    let created = call(
        &app,
        &token,
        "submit_review",
        json!({
            "project": other_project.to_string(),
            "pr": 5,
            "head_sha": HEAD,
            "findings": [finding("low", "別のプロジェクトの指摘")],
        }),
    )
    .await
    .json();
    let finding_id = created["findings"][0]["id"].clone();
    let update = |project: String| {
        let arguments = json!({ "project": project, "finding_id": finding_id, "state": "fixed" });
        call(&app, &token, "update_review_finding", arguments)
    };

    // 指摘の ID だけでは動かせない。パスのプロジェクトの指摘に限る
    assert_eq!(
        update(fx.tp.project_id.to_string()).await.error(),
        "404 not-found"
    );
    let untouched = findings(
        &app,
        &token,
        json!({ "project": other_project.to_string(), "pr": 5 }),
    )
    .await;
    assert_eq!(untouched[0]["state"], "open");

    // 対照: 指摘のあるプロジェクトを指せば通る
    let fixed = update(other_project.to_string()).await.json();
    assert_eq!(fixed["state"], "fixed", "{fixed}");
}

#[tokio::test]
async fn review_tools_refuse_a_tenant_outside_the_token_binding() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    // 持ち主は同じ。束縛していないテナントは、所属していても PAT では触れない
    let other_tenant = crate::common::insert_tenant(&app.state.db, fx.user.id).await;
    let other_project = crate::common::insert_extra_project(&app, other_tenant).await;
    let outside = |extra: Value| {
        let mut arguments = json!({
            "project": other_project.to_string(),
            "tenant": other_tenant.to_string(),
        });
        for (key, value) in extra.as_object().expect("object") {
            arguments[key] = value.clone();
        }
        arguments
    };

    let outcome = call(
        &app,
        &token,
        "list_review_findings",
        outside(json!({ "pr": 5 })),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");
    let outcome = call(
        &app,
        &token,
        "submit_review",
        outside(json!({ "pr": 5, "head_sha": HEAD })),
    )
    .await;
    assert_eq!(outcome.error(), "403 forbidden");

    // 対照: 束縛したテナントは `tenant` を渡しても通る
    let listed = findings(
        &app,
        &token,
        json!({
            "project": fx.tp.project_id.to_string(),
            "tenant": fx.tp.tenant_id.to_string(),
            "pr": 5,
        }),
    )
    .await;
    assert!(listed.is_empty());
}

#[tokio::test]
async fn update_review_finding_names_the_limit_of_the_note() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;
    let created = submit(&app, &token, &fx, 5, json!([finding("low", "題")]))
        .await
        .json();
    let finding_id = created["findings"][0]["id"].clone();
    let update = |state: &'static str, length: usize| {
        let arguments = json!({
            "project": fx.project_key,
            "finding_id": finding_id,
            "state": state,
            "note": "あ".repeat(length),
        });
        call(&app, &token, "update_review_finding", arguments)
    };

    // 上限の値が文面に残り、状態コードで始まらない（API を呼んでいない）
    assert_eq!(
        update("fixed", 2001).await.error(),
        "finding update does not satisfy the API's limits:\n  note: length max: 2000"
    );
    let untouched = findings(&app, &token, json!({ "project": fx.project_key, "pr": 5 })).await;
    assert_eq!(untouched[0]["state"], "open");

    // 対照: 上限ちょうどは通る
    let fixed = update("fixed", 2000).await.json();
    assert_eq!(fixed["state"], "fixed", "{fixed}");
}

#[tokio::test]
async fn submit_review_reads_head_sha_without_the_surrounding_whitespace() {
    let mut app = TestApp::new().await;
    let fx = fixture(&mut app).await;
    let token = api_pat(&app, &fx).await;

    let created = call(
        &app,
        &token,
        "submit_review",
        json!({ "project": fx.project_key, "pr": 5, "head_sha": format!(" {HEAD}\n") }),
    )
    .await
    .json();
    assert_eq!(created["head_sha"], HEAD, "{created}");

    // 空白を落としても 40 桁でなければ弾く
    let outcome = call(
        &app,
        &token,
        "submit_review",
        json!({ "project": fx.project_key, "pr": 5, "head_sha": " 60cdd77 " }),
    )
    .await;
    assert_eq!(
        outcome.error(),
        "`head_sha` must be the full 40-character commit SHA (got: 60cdd77)"
    );
}
