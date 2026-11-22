// 受け入れ条件（docs/dev/features/review-findings/index.md から移した。テストの無い条件はページの「未実装と既知の制限」に残す）
// - 正常系: 一括起票、一覧、fixed、verified が通る
// - 指摘ゼロのラウンド、同一 PR への 2 ラウンド目（R2 として新しいラウンドになる）
// - ラウンドの採番: 同じ PR へ同時にラウンドを確定しても `round` が重複しない
// - リポジトリの同定: 連携を別リポジトリへ差し替えると、同じ PR 番号でも R1 から始まり、旧リポジトリのラウンドは一覧・集計に混ざらない
// - マージ可否: レビューが 1 件も無い PR は「可」にならない（対照: 指摘ゼロのラウンドが 1 件あれば「可」）。集計は最新ラウンドの `head_sha` を返す
// - 取り下げの代行: 作成者がテナントの利用者でなくなった指摘は、テナントオーナーが `rejected` にできる（対照: 作成者が在籍しているうちは、オーナーでも代行できない）
// - 代行の痕跡: 集計に「オーナー代行での棄却 n 件」が残り、遷移履歴に代行者が残る。同じ指摘を `rejected → open → rejected` と往復させても件数は 1 のまま
// - 拒否系: 他テナント 404 / fixed 宣言者本人による verified の拒否（対照: 別レビュワーなら成功）/ 読み取り API の PR 番号が 0 以下
// - 取り下げの主体: 他人が出した指摘は、空のラウンドを確定して「レビュー側」を名乗っても `rejected` にできない（対照: 出した本人は取り下げられる。同じ空ラウンドの作成者でも `verified` は行える）
// - 繰り延べ: deferred で通常タスクが同プロジェクトに起票されリンクされる。deferred → open で自動起票タスクがクローズされる
// - 繰り延べの重大度制約: High / Medium を deferred にしようとすると拒否され、マージ可否が「可」に変わらない（対照: Low / Nit は成功し、通常タスクが起票される）
// - 繰り延べの往復: deferred → open → deferred で同じ `deferred_task_id` が再オープンされる。対照: リンク先のタスクを削除してから再繰り延べするとリンクが差し替わり、削除済みタスクが復活しない
// - 繰り延べ先の担当者ごとの完了: 設定 on で未完了の担当者が残るタスクは、指摘を閉じても完了にならない（対照: 全員完了なら完了する）。完了から開き直すと担当者の完了は全員分消える
// - 戻り遷移: fixed → open の差し戻しはレビュー側のみ（レビュー側でない利用者は 403、対照: レビュワーは成功）。verified からの遷移はすべて拒否（終端）
// - 引き継ぎ（TASK-495）: 元 PR の未解決（open / fixed）の指摘だけが、引き継ぎ先の新しいラウンド（作成者・head は元のまま、`carried_by` は引き継いだ人）へ写り、元 PR は変わらない
// - 引き継ぎのレビュー扱い: 写しのラウンドはレビューに数えない（写しだけの PR は未レビュー。対照: レビュー済みの PR へ Low だけを引き継いでも最新の head は変わらず、マージ可のまま）
// - 引き継ぎの権限: 引き継いだ人は `rejected` / `verified` の権限を得ない（対照: 元のラウンドの作成者は行える。`available_actions` も同じ）
// - 引き継ぎの再実行: 2 回目は 409。元 PR に指摘が増えれば、その分だけ写る
// - 引き継ぎの拒否系: 同じ PR 400 / 元ラウンド無し 409 / 引き継ぐ指摘無し 409 / PR 番号 0 と形式違いの `from_repo` 400 / `write:review` の無い PAT 403 / テナント外 403（対照: 201）
// - 引き継ぎの元リポジトリ: 過去の連携先を `from_repo` で指せる（既定は現在の連携先）
// - 内容の直し（TASK-577）: その指摘を含むラウンドの作成者は、open の指摘の題・本文・位置を直せる。変わった欄だけが前後の値つきで履歴に残り、応答と一覧に `edited_at` が載る。今と同じ値だけの直しは何も書かない
// - 直しの主体と状態: 作成者でない利用者は 403、open でない指摘は 409（対照: open に戻せば直せる）。`write:review` の無い PAT は 403
// - 直しの検め: 欄が無い・line が 0・題が空・file が 1001 文字は 400 で、指摘は変わらない
// - 重大度は直せない: `severity` を送ると（ほかの欄と一緒でも）400 で、何も変わらない
// - 直しとマージ可否: 題・本文・位置を直しても、集計の判定（塞いでいる件数・マージ可否）は変わらない

use crate::common::TestApp;
use axum::http::StatusCode;
use entity::scopes::Scope;
use entity::{
    github_integrations, project_statuses, review_finding_edits, review_findings, reviews,
    task_assignees, tasks, tenant_members,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, prelude::Uuid,
};

// レビュー指摘管理（#623 の仕様）の統合テスト。
//
// 状態遷移の表そのものは service の単体テストが固定するので、ここでは
// 「API を通したときに規則どおり通る／弾かれる」ことと、繰り延べ先タスクの
// 作成・クローズという DB をまたぐ副作用を確かめる。

struct Fixture {
    app: TestApp,
    tenant_id: Uuid,
    project_id: Uuid,
    reviewer: crate::common::TestUser,
    developer: crate::common::TestUser,
}

impl Fixture {
    fn reviews_path(&self) -> String {
        format!(
            "/v1/tenants/{}/projects/{}/reviews",
            self.tenant_id, self.project_id
        )
    }

    fn findings_path(&self) -> String {
        format!(
            "/v1/tenants/{}/projects/{}/review-findings",
            self.tenant_id, self.project_id
        )
    }

    async fn login(&mut self, user: &crate::common::TestUser) {
        self.app.reset_session_client();
        self.app
            .login_session_no_content(&user.email, &user.password)
            .await;
    }
}

/// レビュワー（テナントオーナー）と修正者（テナントメンバー）がいるプロジェクト。
/// 繰り延べ先タスクの作成に必要な既定ステータスと完了ステータスも用意する。
async fn setup() -> Fixture {
    let mut app = TestApp::new().await;
    let reviewer = app.insert_user_default().await;
    let developer = app.insert_user_default().await;
    let tp = app.insert_tenant_project(reviewer.id).await;

    for (name, position, is_default, is_done) in
        [("Todo", 0, true, false), ("Done", 1, false, true)]
    {
        project_statuses::ActiveModel {
            id: Set(Uuid::new_v4()),
            project_id: Set(tp.project_id),
            name: Set(name.into()),
            color: Set("#888888".into()),
            position: Set(position),
            is_default: Set(is_default),
            is_done_state: Set(is_done),
            is_default_done: Set(is_done),
            created_at: Set(chrono::Utc::now().into()),
        }
        .insert(&app.state.db)
        .await
        .expect("insert status");
    }

    // 修正者をテナントメンバーにして、プロジェクトへ入れるようにする
    app.reset_session_client();
    app.login_session_no_content(&reviewer.email, &reviewer.password)
        .await;
    let added = app
        .post_json_with_session(
            &format!("/v1/tenants/{}/members", tp.tenant_id),
            serde_json::json!({ "user_id": developer.id, "role": "Member" }),
        )
        .await;
    assert_eq!(added.status(), StatusCode::CREATED);

    Fixture {
        app,
        tenant_id: tp.tenant_id,
        project_id: tp.project_id,
        reviewer,
        developer,
    }
}

/// プロジェクトを指定のリポジトリへ連携させ、その連携 ID を返す。
async fn link_repo(fx: &Fixture, owner: &str, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    github_integrations::ActiveModel {
        id: Set(id),
        project_id: Set(fx.project_id),
        // 他テストと衝突しない範囲で散らす
        installation_id: Set((Uuid::new_v4().as_u128() % 1_000_000) as i64 + 9_000_000),
        repo_owner: Set(owner.into()),
        repo_name: Set(name.into()),
        access_token_enc: Set("unused".into()),
        token_expires_at: Set(chrono::Utc::now().into()),
        created_by: Set(fx.reviewer.id),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(&fx.app.state.db)
    .await
    .expect("insert integration");
    id
}

async fn json(res: reqwest::Response) -> serde_json::Value {
    res.json::<serde_json::Value>().await.expect("json body")
}

/// 指摘 1 件のラウンドを起票し、(review_id, finding_id) を返す。
async fn submit_round(fx: &Fixture, pr: i32, severity: &str, title: &str) -> (String, String) {
    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": pr,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "summary": "総評",
                "findings": [{
                    "severity": severity,
                    "title": title,
                    "body": "再現条件と根拠",
                    "file": "apps/frontend/src/App.vue",
                    "line": 42,
                }],
            }),
        )
        .await;
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "ラウンドの起票は成功する"
    );
    let body = json(res).await;
    (
        body["id"].as_str().expect("review id").to_string(),
        body["findings"][0]["id"]
            .as_str()
            .expect("finding id")
            .to_string(),
    )
}

async fn edit(fx: &Fixture, finding_id: &str, body: serde_json::Value) -> reqwest::Response {
    fx.app
        .patch_json_with_session(
            &format!("{}/{finding_id}/content", fx.findings_path()),
            body,
        )
        .await
}

/// 指摘の直しの履歴を、欄の名の順に (欄, 前, 後, edit_id, 直した人) で返す。
async fn edits_of(
    fx: &Fixture,
    finding_id: &str,
) -> Vec<(String, Option<String>, Option<String>, Uuid, Uuid)> {
    let mut rows = review_finding_edits::Entity::find()
        .filter(
            review_finding_edits::Column::FindingId.eq(finding_id.parse::<Uuid>().expect("uuid")),
        )
        .all(&fx.app.state.db)
        .await
        .expect("edits")
        .into_iter()
        .map(|row| {
            (
                row.field,
                row.before_value,
                row.after_value,
                row.edit_id,
                row.actor_id,
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

async fn finding_row(fx: &Fixture, finding_id: &str) -> review_findings::Model {
    review_findings::Entity::find_by_id(finding_id.parse::<Uuid>().expect("uuid"))
        .one(&fx.app.state.db)
        .await
        .expect("finding")
        .expect("finding row")
}

async fn transition(fx: &Fixture, finding_id: &str, state: &str) -> reqwest::Response {
    fx.app
        .patch_json_with_session(
            &format!("{}/{finding_id}", fx.findings_path()),
            serde_json::json!({ "state": state }),
        )
        .await
}

/// 一括起票 → 一覧 → fixed → verified の正常系と、集計のマージ可否。
#[tokio::test]
async fn round_is_created_and_findings_run_through_the_happy_path() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    let (review_id, finding_id) = submit_round(&fx, 618, "high", "認可が抜けている").await;

    // 起票直後は High が 1 件未解決なのでマージ不可
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=618", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["rounds"], 1);
    assert_eq!(summary["blocking"], 1);
    assert_eq!(summary["mergeable"], false);

    // ラウンド一覧に R1 が出る
    let rounds = json(
        fx.app
            .get_with_session(&format!("{}?pr=618", fx.reviews_path()))
            .await,
    )
    .await;
    let rounds = rounds.as_array().expect("rounds");
    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0]["round"], 1);
    assert_eq!(rounds[0]["finding_count"], 1);
    assert_eq!(rounds[0]["reviewer"]["id"], fx.reviewer.id.to_string());

    // 修正者が fixed を宣言する
    fx.login(&fx.developer.clone()).await;
    let res = transition(&fx, &finding_id, "fixed").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = json(res).await;
    assert_eq!(body["state"], "fixed");
    assert_eq!(body["fixed_by"], fx.developer.id.to_string());
    // 起票と fixed の 2 行が履歴に残る
    let history = body["transitions"].as_array().expect("transitions");
    assert_eq!(history.len(), 2);
    assert!(history[0]["from_state"].is_null(), "起票は from が null");
    assert_eq!(history[1]["to_state"], "fixed");

    // fixed でもマージ判定は塞がったまま（確認が済んでいないため）
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=618", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["blocking"], 1, "fixed は未解決として数える");

    // レビュワーが確認する
    fx.login(&fx.reviewer.clone()).await;
    let res = transition(&fx, &finding_id, "verified").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(json(res).await["state"], "verified");

    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=618", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["blocking"], 0);
    assert_eq!(summary["mergeable"], true, "確認済みならマージできる");

    // ラウンド詳細からも読める
    let detail = json(
        fx.app
            .get_with_session(&format!("{}/{review_id}", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(detail["findings"][0]["state"], "verified");

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 修正側は fixing（修正中）を宣言・取り消しでき、そこから fixed へ進める。
/// fixing はマージ判定では未解決のまま数え、確認を飛ばして verified にはできない。
#[tokio::test]
async fn the_fixer_can_mark_a_finding_as_fixing() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding_id) = submit_round(&fx, 719, "high", "入力の検証が無い").await;

    // レビュー側でない修正者でも着手を宣言できる
    fx.login(&fx.developer.clone()).await;
    let res = transition(&fx, &finding_id, "fixing").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = json(res).await;
    assert_eq!(body["state"], "fixing");
    assert!(body["fixed_by"].is_null(), "着手は修正の宣言ではない");
    assert_eq!(
        body["available_actions"],
        serde_json::json!(["open", "fixed"])
    );

    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=719", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["blocking"], 1, "fixing は未解決として数える");

    // 着手の取り消しと再着手
    assert_eq!(
        transition(&fx, &finding_id, "open").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        transition(&fx, &finding_id, "fixing").await.status(),
        StatusCode::OK
    );

    // 確認を飛ばして verified にはできない（レビュー側でも）
    fx.login(&fx.reviewer.clone()).await;
    assert_eq!(
        transition(&fx, &finding_id, "verified").await.status(),
        StatusCode::CONFLICT
    );

    fx.login(&fx.developer.clone()).await;
    let res = transition(&fx, &finding_id, "fixed").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = json(res).await;
    assert_eq!(body["state"], "fixed");
    assert_eq!(body["fixed_by"], fx.developer.id.to_string());

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 自分で fixed を宣言した人は、自分でその指摘を verified にできない。
/// 別のレビュワーなら通る（過剰拒否でないことの対照）。
#[tokio::test]
async fn the_fixer_cannot_verify_their_own_fix() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding_id) = submit_round(&fx, 700, "medium", "境界値が 1 つずれている").await;

    // レビュワー自身が fixed を宣言してしまうと…
    let res = transition(&fx, &finding_id, "fixed").await;
    assert_eq!(res.status(), StatusCode::OK);

    // …同じ人は verified に進められない
    let res = transition(&fx, &finding_id, "verified").await;
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "自分の修正を自分で確認済みにはできない"
    );

    // 別のレビュワー（R2 を出した人）なら確認できる
    fx.login(&fx.developer.clone()).await;
    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 700,
                "head_sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "summary": "2 巡目",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(
        json(res).await["round"],
        2,
        "同一 PR の再レビューは R2 になる"
    );

    let res = transition(&fx, &finding_id, "verified").await;
    assert_eq!(res.status(), StatusCode::OK, "別のレビュワーなら確認できる");

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// レビューを一度も出していない利用者は、差し戻し・棄却を行えない。
#[tokio::test]
async fn reviewer_only_transitions_reject_the_developer() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding_id) = submit_round(&fx, 701, "high", "認可が抜けている").await;

    fx.login(&fx.developer.clone()).await;
    // 棄却は指摘を出した本人だけの判断
    let res = transition(&fx, &finding_id, "rejected").await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // fixed を宣言するのは修正側でよい
    assert_eq!(
        transition(&fx, &finding_id, "fixed").await.status(),
        StatusCode::OK
    );
    // 差し戻し（未対応判定）はレビュー側だけ
    let res = transition(&fx, &finding_id, "open").await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // レビュワーなら差し戻せる
    fx.login(&fx.reviewer.clone()).await;
    let res = transition(&fx, &finding_id, "open").await;
    assert_eq!(res.status(), StatusCode::OK, "レビュワーは差し戻せる");
    assert_eq!(json(res).await["state"], "open");

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 規則にない遷移は 409。verified は終端。
#[tokio::test]
async fn invalid_transitions_are_rejected() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding_id) = submit_round(&fx, 702, "low", "命名が揺れている").await;

    // 確認を飛ばして verified にはできない
    assert_eq!(
        transition(&fx, &finding_id, "verified").await.status(),
        StatusCode::CONFLICT
    );

    // open → fixed → verified まで進めてから
    assert_eq!(
        transition(&fx, &finding_id, "fixed").await.status(),
        StatusCode::OK
    );
    fx.login(&fx.developer.clone()).await;
    // developer は fixed を宣言していないので確認できる
    assert_eq!(
        transition(&fx, &finding_id, "verified").await.status(),
        StatusCode::FORBIDDEN,
        "レビューを出していない利用者は確認もできない"
    );

    fx.login(&fx.reviewer.clone()).await;
    // reviewer が fixed を宣言したので、この指摘は本人以外が確認するしかない。
    // 2 巡目を developer が出して確認する
    fx.login(&fx.developer.clone()).await;
    assert_eq!(
        fx.app
            .post_json_with_session(
                &fx.reviews_path(),
                serde_json::json!({
                    "pr_number": 702,
                    "head_sha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "findings": [],
                }),
            )
            .await
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        transition(&fx, &finding_id, "verified").await.status(),
        StatusCode::OK
    );

    // verified は終端。どこへも戻せない
    for state in ["open", "fixing", "fixed", "deferred", "rejected"] {
        assert_eq!(
            transition(&fx, &finding_id, state).await.status(),
            StatusCode::CONFLICT,
            "verified からは {state} へ遷移できない"
        );
    }

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 繰り延べで通常タスクが起票されリンクされ、取り消すとそのタスクが完了する。
#[tokio::test]
async fn deferring_creates_a_task_and_reverting_closes_it() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding_id) = submit_round(&fx, 703, "nit", "コメントの表記ゆれ").await;

    let res = transition(&fx, &finding_id, "deferred").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = json(res).await;
    assert_eq!(body["state"], "deferred");
    let task_id: Uuid = body["deferred_task_id"]
        .as_str()
        .expect("繰り延べ先タスクがリンクされる")
        .parse()
        .expect("uuid");

    let task = tasks::Entity::find_by_id(task_id)
        .one(&fx.app.state.db)
        .await
        .expect("query task")
        .expect("task row");
    assert_eq!(task.project_id, fx.project_id, "同じプロジェクトに起票する");
    assert!(
        task.title.contains("コメントの表記ゆれ"),
        "指摘のタイトルを引き継ぐ: {}",
        task.title
    );
    assert_eq!(task.priority, tasks::TaskPriority::Low);
    assert!(task.completed_at.is_none(), "起票直後は未完了");

    // 繰り延べた Low/Nit はマージを塞がない
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=703", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["mergeable"], true);

    // 「やはり今直す」で open へ戻すと、自動起票したタスクは畳まれる
    let res = transition(&fx, &finding_id, "open").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = json(res).await;
    assert_eq!(body["state"], "open");
    assert_eq!(
        body["deferred_task_id"],
        task_id.to_string(),
        "リンクは残す（次の繰り延べで開き直すため）: {body}"
    );

    let task = tasks::Entity::find_by_id(task_id)
        .one(&fx.app.state.db)
        .await
        .expect("query task")
        .expect("task row");
    assert!(
        task.completed_at.is_some(),
        "二重管理を作らないよう自動起票タスクは完了する"
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 指摘ゼロのラウンドを作っても、他人が出した指摘を取り下げる権利は得られない。
///
/// ラウンドは指摘ゼロでも確定できる。取り下げまで「より新しいラウンドの作成者」に
/// 認めると、修正する側が空のラウンドを 1 本作るだけで他人の High を `rejected` にでき、
/// マージ基準を 1 人で迂回できてしまう（仕様 §3）。
#[tokio::test]
async fn an_empty_round_does_not_grant_the_right_to_reject() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 710,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "summary": "総評",
                "findings": [
                    { "severity": "high", "title": "認可が抜けている", "body": "根拠" },
                    { "severity": "high", "title": "検証が抜けている", "body": "根拠" },
                ],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = json(res).await;
    let id_of = |title: &str| -> String {
        body["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .find(|f| f["title"] == title)
            .unwrap_or_else(|| panic!("{title} の指摘"))["id"]
            .as_str()
            .expect("finding id")
            .to_string()
    };
    let first = id_of("認可が抜けている");
    let second = id_of("検証が抜けている");

    // 修正する側が、指摘ゼロのラウンドを確定して「レビュー側」を名乗る
    fx.login(&fx.developer.clone()).await;
    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 710,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "summary": "指摘なし",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(json(res).await["round"], 2, "空のラウンドも確定はできる");

    // それでも他人の指摘は取り下げられない
    assert_eq!(
        transition(&fx, &first, "rejected").await.status(),
        StatusCode::FORBIDDEN,
        "空のラウンドでは取り下げの権利を得られない"
    );

    // マージ可否は「可」に変わらない
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=710", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["mergeable"], false);
    assert_eq!(summary["blocking"], 2, "High は 2 件とも未解決のまま");

    // 対照 1: 確認（verified）は後から出したラウンドの作成者にも許す
    // ——再レビューの「解消」判定そのものなので塞がない
    fx.login(&fx.reviewer.clone()).await;
    assert_eq!(
        transition(&fx, &first, "fixed").await.status(),
        StatusCode::OK
    );
    fx.login(&fx.developer.clone()).await;
    assert_eq!(
        transition(&fx, &first, "verified").await.status(),
        StatusCode::OK,
        "空のラウンドでも確認は行える"
    );

    // 対照 2: 指摘を出した本人は取り下げられる
    fx.login(&fx.reviewer.clone()).await;
    let res = transition(&fx, &second, "rejected").await;
    assert_eq!(res.status(), StatusCode::OK, "出した本人は取り下げられる");
    assert_eq!(json(res).await["state"], "rejected");

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 同じ PR へ同時にラウンドを確定しても `round` は重複しない。
///
/// 採番から挿入までをプロジェクト行のロックで直列化し、
/// `UNIQUE (project_id, pr_number, round)` を最後の防波堤に置いている（仕様 §3）。
/// 番号が重なると R1, R2, … と「どの head を見た判断か」の対応が崩れる。
#[tokio::test]
async fn concurrent_rounds_on_the_same_pr_get_distinct_numbers() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    let path = fx.reviews_path();
    let body = |i: i32| {
        serde_json::json!({
            "pr_number": 709,
            "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
            "summary": format!("同時に確定したレビュー {i}"),
            "findings": [],
        })
    };
    // 5 本を同時に投げる（依存を増やさないよう join! で並べる）
    let responses = tokio::join!(
        fx.app.post_json_with_session(&path, body(1)),
        fx.app.post_json_with_session(&path, body(2)),
        fx.app.post_json_with_session(&path, body(3)),
        fx.app.post_json_with_session(&path, body(4)),
        fx.app.post_json_with_session(&path, body(5)),
    );
    let responses = [
        responses.0,
        responses.1,
        responses.2,
        responses.3,
        responses.4,
    ];

    let mut rounds = Vec::new();
    for res in responses {
        assert_eq!(res.status(), StatusCode::CREATED, "同時でも起票は成功する");
        rounds.push(json(res).await["round"].as_i64().expect("round"));
    }
    rounds.sort_unstable();
    assert_eq!(
        rounds,
        vec![1, 2, 3, 4, 5],
        "round が重複しない: {rounds:?}"
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 連携先リポジトリを差し替えると、同じ PR 番号でも別の PR として扱う。
///
/// プロジェクトの連携先は解除・再連携で差し替えられる。`project_id + pr_number` だけを
/// キーにすると、旧リポジトリの PR #10 と新リポジトリの PR #10 が同じ PR として続く
/// （仕様 §3）。
#[tokio::test]
async fn rounds_are_scoped_to_the_linked_repository() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    let old_integration = link_repo(&fx, "acme", "old-repo").await;
    let (_, finding_id) = submit_round(&fx, 711, "high", "旧リポジトリの指摘").await;

    // 旧リポジトリのラウンドは見えている
    let rounds = json(
        fx.app
            .get_with_session(&format!("{}?pr=711", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(rounds.as_array().expect("rounds").len(), 1);

    // 連携を別リポジトリへ差し替える
    github_integrations::Entity::delete_by_id(old_integration)
        .exec(&fx.app.state.db)
        .await
        .expect("delete integration");
    link_repo(&fx, "acme", "new-repo").await;

    // 同じ PR 番号でも R1 から始まる（旧リポジトリの続きにしない）
    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 711,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "summary": "新リポジトリのレビュー",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(json(res).await["round"], 1, "別リポジトリの PR は R1 から");

    // 一覧・集計に旧リポジトリのラウンドと指摘が混ざらない
    let rounds = json(
        fx.app
            .get_with_session(&format!("{}?pr=711", fx.reviews_path()))
            .await,
    )
    .await;
    let rounds = rounds.as_array().expect("rounds");
    assert_eq!(rounds.len(), 1, "現在の連携先のラウンドだけ: {rounds:?}");
    assert_eq!(rounds[0]["summary"], "新リポジトリのレビュー");

    let findings = json(
        fx.app
            .get_with_session(&format!("{}?pr=711", fx.findings_path()))
            .await,
    )
    .await;
    assert!(
        findings.as_array().expect("findings").is_empty(),
        "旧リポジトリの指摘は出てこない: {findings}"
    );

    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=711", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["blocking"], 0, "旧リポジトリの High は数えない");
    assert_eq!(summary["mergeable"], true);

    // リポジトリを明示すれば、旧リポジトリのラウンドも読める（履歴が読めないと
    // 出した本人が整理もできない。仕様 §5）
    let old_rounds = json(
        fx.app
            .get_with_session(&format!("{}?pr=711&repo=acme/old-repo", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(old_rounds.as_array().expect("rounds").len(), 1);
    assert_eq!(old_rounds[0]["summary"], "総評");
    let old_summary = json(
        fx.app
            .get_with_session(&format!(
                "{}/summary?pr=711&repo=acme/old-repo",
                fx.reviews_path()
            ))
            .await,
    )
    .await;
    assert_eq!(old_summary["blocking"], 1, "旧リポジトリの High が見える");
    assert_eq!(old_summary["repository"], "acme/old-repo");

    // 形式が違えば 400（黙って現在の連携先へ落とさない）
    assert_eq!(
        fx.app
            .get_with_session(&format!("{}?pr=711&repo=acme", fx.reviews_path()))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );

    // 旧リポジトリの指摘は消えていない（連携解除で失われない）
    let finding = review_findings::Entity::find_by_id(finding_id.parse::<Uuid>().expect("uuid"))
        .one(&fx.app.state.db)
        .await
        .expect("query finding");
    assert!(finding.is_some(), "連携を外しても指摘は残る");

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// レビュー側の判定もリポジトリで絞る。
///
/// ラウンド番号はリポジトリごとに 1 から振り直されるので、絞らないと旧リポジトリで
/// R1 を出しただけの利用者が、新リポジトリの同じ PR 番号でもレビュー側と判定される。
#[tokio::test]
async fn the_reviewer_side_is_scoped_to_the_linked_repository() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    // reviewer は旧リポジトリでだけラウンドを出す
    let old_integration = link_repo(&fx, "acme", "old-repo").await;
    submit_round(&fx, 711, "high", "旧リポジトリの指摘").await;

    github_integrations::Entity::delete_by_id(old_integration)
        .exec(&fx.app.state.db)
        .await
        .expect("delete integration");
    link_repo(&fx, "acme", "new-repo").await;

    // 新リポジトリの PR #711 は developer がレビューする（同じく R1 から始まる）
    fx.login(&fx.developer.clone()).await;
    let (_, finding_id) = submit_round(&fx, 711, "high", "新リポジトリの指摘").await;
    assert_eq!(
        transition(&fx, &finding_id, "fixed").await.status(),
        StatusCode::OK
    );

    // 旧リポジトリの R1 しか持たない reviewer は、新リポジトリの
    // fixed → verified を行えない
    fx.login(&fx.reviewer.clone()).await;
    assert_eq!(
        transition(&fx, &finding_id, "verified").await.status(),
        StatusCode::FORBIDDEN,
        "旧リポジトリのラウンドでレビュー側を名乗れない"
    );

    // 現在の連携先でラウンドを出せば進められる（過剰に拒否していない）
    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 711,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "summary": "新リポジトリの再レビュー",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(
        transition(&fx, &finding_id, "verified").await.status(),
        StatusCode::OK
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 作成者がテナントから居なくなった指摘は、オーナーが取り下げを代行できる。
///
/// 取り下げは本来「出した本人だけ」。除名・退会で主体が消えると、誤った High を
/// 取り下げる手段が無くなり、直していないものを verified と記録するしかなくなる（仕様 §3）。
#[tokio::test]
async fn the_owner_can_reject_on_behalf_of_a_departed_reviewer() {
    let mut fx = setup().await;

    // 修正する側（developer）が指摘を出す。オーナーは reviewer
    fx.login(&fx.developer.clone()).await;
    let (_, finding_id) = submit_round(&fx, 713, "high", "誤った指摘").await;

    // 在籍しているうちは、オーナーでも代行できない（役割規則を素通しにしない）
    fx.login(&fx.reviewer.clone()).await;
    assert_eq!(
        transition(&fx, &finding_id, "rejected").await.status(),
        StatusCode::FORBIDDEN,
        "作成者が在籍していれば代行できない"
    );

    // 在籍しているうちは、ラウンド一覧も不在の印を立てない
    // （画面はこの印で代行ボタンを出すので、立てると押しても 403 のボタンになる）
    let rounds = json(
        fx.app
            .get_with_session(&format!("{}?pr=713", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(
        rounds[0]["reviewer_left_tenant"], false,
        "在籍中は不在の印が立たない: {rounds}"
    );

    // テナントから居なくなる
    tenant_members::Entity::delete_many()
        .filter(tenant_members::Column::TenantId.eq(fx.tenant_id))
        .filter(tenant_members::Column::UserId.eq(fx.developer.id))
        .exec(&fx.app.state.db)
        .await
        .expect("remove tenant member");

    // 不在になったラウンドには印が立つ（画面がオーナーへ代行ボタンを出せる）
    let rounds = json(
        fx.app
            .get_with_session(&format!("{}?pr=713", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(
        rounds[0]["reviewer_left_tenant"], true,
        "不在になったら印が立つ: {rounds}"
    );

    // オーナー自身のラウンドには印が立たない。オーナーは tenant_members 行を
    // 持たないが不在ではない（立てると、本人として取り下げられるのに画面が
    // 「代行」を出す）。一覧はラウンド降順なので [0] が今出したラウンド
    let _ = submit_round(&fx, 713, "low", "オーナーのラウンド").await;
    let rounds = json(
        fx.app
            .get_with_session(&format!("{}?pr=713", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(
        rounds[0]["reviewer_left_tenant"], false,
        "オーナー自身のラウンドは不在扱いにしない: {rounds}"
    );
    assert_eq!(
        rounds[1]["reviewer_left_tenant"], true,
        "不在の印はラウンド単位で残る: {rounds}"
    );

    let res = transition(&fx, &finding_id, "rejected").await;
    assert_eq!(res.status(), StatusCode::OK, "作成者が消えたら代行できる");
    let body = json(res).await;
    assert_eq!(body["state"], "rejected");

    // 代行の痕跡は集計に残る（除名 → 代行 → 再招待をしても消えない）
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=713", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(
        summary["owner_override_rejections"], 1,
        "オーナー代行での棄却が数えられる: {summary}"
    );

    // 往復させても件数は増えない。数えるのは指摘の件数であって遷移の回数ではない
    // （rejected → open は通るので、回数で数えると指摘 1 件が何件にも見える）
    assert_eq!(
        transition(&fx, &finding_id, "open").await.status(),
        StatusCode::OK,
        "代行で棄却した指摘は open へ戻せる"
    );
    assert_eq!(
        transition(&fx, &finding_id, "rejected").await.status(),
        StatusCode::OK,
        "戻したものをもう一度代行で棄却できる"
    );
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=713", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(
        summary["owner_override_rejections"], 1,
        "同じ指摘を往復させても 1 件のまま: {summary}"
    );

    // 誰が代行したかは履歴に残る
    let transitions = body["transitions"].as_array().expect("transitions");
    let last = transitions.last().expect("last transition");
    assert_eq!(last["to_state"], "rejected");
    assert_eq!(last["actor"]["id"], fx.reviewer.id.to_string());

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// レビューが 1 件も無い PR は「マージ可」にしない。
///
/// 件数だけで判定すると、未レビューの PR が 0 件として通る（仕様 §5）。
#[tokio::test]
async fn a_pull_request_without_any_round_is_not_mergeable() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=712", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["rounds"], 0);
    assert_eq!(summary["blocking"], 0);
    assert_eq!(
        summary["mergeable"], false,
        "レビューされていない PR は通さない: {summary}"
    );
    assert!(summary["latest_head_sha"].is_null());

    // 対照: 指摘ゼロのラウンドが 1 件あれば「可」になり、見た commit も返る
    let head = "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e";
    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 712,
                "head_sha": head,
                "summary": "指摘なし",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=712", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["rounds"], 1);
    assert_eq!(summary["mergeable"], true);
    assert_eq!(
        summary["latest_head_sha"], head,
        "レビューした commit を返す（鮮度は呼び出し側が見る）"
    );
    assert!(
        summary["repository"].is_null(),
        "連携が無ければ集計対象のリポジトリは空（呼び出し側はゲートとして通さない）: {summary}"
    );

    // 対照: 連携があれば、どのリポジトリを見た集計かを返す
    link_repo(&fx, "acme", "app").await;
    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 712,
                "head_sha": head,
                "summary": "連携後のレビュー",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=712", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["repository"], "acme/app");

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// PR 一覧の `blocking_fixed` は High / Medium かつ fixed だけを数え、`blocking` /
/// `unresolved` は従来どおり open / fixing / fixed を数える（TASK-302）。
#[tokio::test]
async fn pull_request_list_separates_fixed_blocking_findings() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    // 731: High / Medium がすべて fixed（Low の fixed は blocking_fixed に入らない）
    let (_, high_fixed) = submit_round(&fx, 731, "high", "fixed の High").await;
    let (_, medium_fixed) = submit_round(&fx, 731, "medium", "fixed の Medium").await;
    let (_, low_fixed) = submit_round(&fx, 731, "low", "fixed の Low").await;
    // 732: fixed と open が混ざる
    let (_, mixed_fixed) = submit_round(&fx, 732, "high", "fixed の High").await;
    submit_round(&fx, 732, "medium", "open の Medium").await;
    // 733: 着手中（fixing）だけ
    let (_, fixing) = submit_round(&fx, 733, "high", "fixing の High").await;

    fx.login(&fx.developer.clone()).await;
    for id in [&high_fixed, &medium_fixed, &low_fixed, &mixed_fixed] {
        assert_eq!(transition(&fx, id, "fixed").await.status(), StatusCode::OK);
    }
    assert_eq!(
        transition(&fx, &fixing, "fixing").await.status(),
        StatusCode::OK
    );

    let prs = json(
        fx.app
            .get_with_session(&format!("{}/pull-requests", fx.reviews_path()))
            .await,
    )
    .await;
    let row = |pr: i32| {
        prs.as_array()
            .expect("array")
            .iter()
            .find(|row| row["pr_number"] == pr)
            .unwrap_or_else(|| panic!("PR {pr} が一覧にある: {prs}"))
            .clone()
    };
    for (pr, unresolved, blocking, blocking_fixed) in
        [(731, 3, 2, 2), (732, 2, 2, 1), (733, 1, 1, 0)]
    {
        let row = row(pr);
        assert_eq!(row["unresolved"], unresolved, "PR {pr}: {row}");
        assert_eq!(row["blocking"], blocking, "PR {pr}: {row}");
        assert_eq!(row["blocking_fixed"], blocking_fixed, "PR {pr}: {row}");
    }

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// マージ前必須の重大度は繰り延べられない。
///
/// 繰り延べはマージ可否の集計から外れるので、High / Medium に許すと
/// 「1 回 deferred にすればマージ可」という迂回路ができる（仕様 §3）。
#[tokio::test]
async fn high_and_medium_findings_cannot_be_deferred() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 708,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "summary": "総評",
                "findings": [
                    { "severity": "high", "title": "認可が抜けている", "body": "根拠" },
                    { "severity": "medium", "title": "境界値が 1 つずれている", "body": "根拠" },
                    { "severity": "low", "title": "命名が揺れている", "body": "根拠" },
                ],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = json(res).await;
    let id_of = |severity: &str| -> String {
        body["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .find(|f| f["severity"] == severity)
            .unwrap_or_else(|| panic!("{severity} の指摘"))["id"]
            .as_str()
            .expect("finding id")
            .to_string()
    };

    for severity in ["high", "medium"] {
        let finding_id = id_of(severity);
        let res = transition(&fx, &finding_id, "deferred").await;
        assert_eq!(
            res.status(),
            StatusCode::CONFLICT,
            "{severity} は繰り延べられない"
        );

        // 理由が本文に出る（CLI から使うレビュワーが対処を選べるように）
        let message = json(res).await["message"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(
            message.contains(severity) && message.contains("繰り延べ"),
            "なぜ通らないのかが分かる: {message}"
        );

        // 状態も変わっていない（拒否したのにタスクだけ起票される、を防ぐ）
        let listed = json(
            fx.app
                .get_with_session(&format!("{}?pr=708", fx.findings_path()))
                .await,
        )
        .await;
        let finding = listed
            .as_array()
            .expect("findings")
            .iter()
            .find(|f| f["id"] == finding_id.as_str())
            .expect("対象の指摘")
            .clone();
        assert_eq!(finding["state"], "open");
        assert!(
            finding["deferred_task_id"].is_null(),
            "拒否した繰り延べでタスクを起票しない: {finding}"
        );
    }

    // マージ可否は「可」に変わらない
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=708", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["mergeable"], false);
    assert_eq!(summary["blocking"], 2, "High / Medium は未解決のまま");

    // 対照: Low は繰り延べられ、通常タスクが起票される
    let res = transition(&fx, &id_of("low"), "deferred").await;
    assert_eq!(res.status(), StatusCode::OK, "Low は繰り延べられる");
    let body = json(res).await;
    assert_eq!(body["state"], "deferred");
    assert!(
        body["deferred_task_id"].is_string(),
        "繰り延べ先タスクがリンクされる: {body}"
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 繰り延べを往復しても有効なタスクは 1 件。削除されていたら代替を起票する。
///
/// 「常に同じ物理タスク」ではなく「同時に存在する有効なタスクは 1 件」が不変条件
/// （仕様 §3）。タスクの削除はソフトデリートで外部キーが外れないため、生死は
/// リンク先の `deleted_at` で見る。
#[tokio::test]
async fn deferring_again_reuses_the_task_unless_it_was_deleted() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding_id) = submit_round(&fx, 714, "low", "命名が揺れている").await;

    let first: Uuid =
        json(transition(&fx, &finding_id, "deferred").await).await["deferred_task_id"]
            .as_str()
            .expect("繰り延べ先タスク")
            .parse()
            .expect("uuid");

    // 往復しても新しいタスクを作らず、同じタスクを開き直す
    assert_eq!(
        transition(&fx, &finding_id, "open").await.status(),
        StatusCode::OK
    );
    let again = json(transition(&fx, &finding_id, "deferred").await).await;
    assert_eq!(
        again["deferred_task_id"],
        first.to_string(),
        "同じタスクを使い回す: {again}"
    );
    let task = tasks::Entity::find_by_id(first)
        .one(&fx.app.state.db)
        .await
        .expect("query task")
        .expect("task row");
    assert!(task.completed_at.is_none(), "開き直したので未完了に戻る");

    // 人がタスクを消したら、次の繰り延べは代替を 1 件起票してリンクを差し替える
    assert_eq!(
        transition(&fx, &finding_id, "open").await.status(),
        StatusCode::OK
    );
    let mut active: tasks::ActiveModel = tasks::Entity::find_by_id(first)
        .one(&fx.app.state.db)
        .await
        .expect("query task")
        .expect("task row")
        .into();
    active.deleted_at = Set(Some(chrono::Utc::now().into()));
    active.update(&fx.app.state.db).await.expect("delete task");

    let replaced = json(transition(&fx, &finding_id, "deferred").await).await;
    let replacement = replaced["deferred_task_id"]
        .as_str()
        .expect("代替タスク")
        .to_string();
    assert_ne!(
        replacement,
        first.to_string(),
        "削除済みタスクを復活させず、代替を起票する: {replaced}"
    );
    let deleted = tasks::Entity::find_by_id(first)
        .one(&fx.app.state.db)
        .await
        .expect("query task")
        .expect("task row");
    assert!(
        deleted.deleted_at.is_some(),
        "消したタスクは消えたまま（黙って復活しない）"
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 繰り延べ先タスクを「担当者全員の完了が必要」にして、担当者を入れる。
/// `completed` の担当者は完了済みで入れる。
async fn require_all_assignees(fx: &Fixture, task_id: Uuid, assignees: &[(Uuid, bool)]) {
    let db = &fx.app.state.db;
    let mut active: tasks::ActiveModel = tasks::Entity::find_by_id(task_id)
        .one(db)
        .await
        .expect("query task")
        .expect("task row")
        .into();
    active.require_all_assignees_done = Set(true);
    active.update(db).await.expect("set rule");
    for (user_id, completed) in assignees {
        task_assignees::ActiveModel {
            id: Set(Uuid::new_v4()),
            task_id: Set(task_id),
            user_id: Set(*user_id),
            role: Set("assignee".into()),
            assigned_at: Set(chrono::Utc::now().into()),
            completed_at: Set(completed.then(|| chrono::Utc::now().into())),
        }
        .insert(db)
        .await
        .expect("insert assignee");
    }
}

async fn task_row(fx: &Fixture, task_id: Uuid) -> tasks::Model {
    tasks::Entity::find_by_id(task_id)
        .one(&fx.app.state.db)
        .await
        .expect("query task")
        .expect("task row")
}

async fn completed_assignees(fx: &Fixture, task_id: Uuid) -> usize {
    task_assignees::Entity::find()
        .filter(task_assignees::Column::TaskId.eq(task_id))
        .filter(task_assignees::Column::CompletedAt.is_not_null())
        .all(&fx.app.state.db)
        .await
        .expect("query assignees")
        .len()
}

/// 繰り延べ先タスクが「担当者全員の完了が必要」で未完了の担当者が残るとき、
/// 指摘を閉じても完了にしない（PR マージの自動完了と同じ扱い）。
/// 対照: 全員が完了していれば完了する。
#[tokio::test]
async fn closing_a_deferred_task_respects_pending_assignees() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (reviewer_id, developer_id) = (fx.reviewer.id, fx.developer.id);

    let (_, pending_finding) = submit_round(&fx, 720, "low", "未完了の担当者が残る").await;
    let pending_task: Uuid =
        json(transition(&fx, &pending_finding, "deferred").await).await["deferred_task_id"]
            .as_str()
            .expect("繰り延べ先タスク")
            .parse()
            .expect("uuid");
    require_all_assignees(
        &fx,
        pending_task,
        &[(reviewer_id, true), (developer_id, false)],
    )
    .await;
    let before = task_row(&fx, pending_task).await;

    let res = transition(&fx, &pending_finding, "open").await;
    assert_eq!(res.status(), StatusCode::OK, "指摘の遷移そのものは通る");
    let after = task_row(&fx, pending_task).await;
    assert_eq!(after.status_id, before.status_id, "完了へ移さない");
    assert!(after.completed_at.is_none(), "未完了のまま残る");
    assert_eq!(completed_assignees(&fx, pending_task).await, 1);

    // 対照: 全員完了なら、設定 on でも完了にする
    let (_, done_finding) = submit_round(&fx, 721, "low", "全員完了").await;
    let done_task: Uuid =
        json(transition(&fx, &done_finding, "deferred").await).await["deferred_task_id"]
            .as_str()
            .expect("繰り延べ先タスク")
            .parse()
            .expect("uuid");
    require_all_assignees(&fx, done_task, &[(reviewer_id, true), (developer_id, true)]).await;
    let res = transition(&fx, &done_finding, "open").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        task_row(&fx, done_task).await.completed_at.is_some(),
        "全員完了なら完了する"
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 完了済みの繰り延べ先タスクを開き直すと、担当者の完了は全員分消える。
/// 対照: 未完了のまま開き直すときは、担当者の完了を消さない。
#[tokio::test]
async fn reopening_a_deferred_task_clears_assignee_completions() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (reviewer_id, developer_id) = (fx.reviewer.id, fx.developer.id);

    let (_, finding_id) = submit_round(&fx, 722, "low", "開き直し").await;
    let task_id: Uuid =
        json(transition(&fx, &finding_id, "deferred").await).await["deferred_task_id"]
            .as_str()
            .expect("繰り延べ先タスク")
            .parse()
            .expect("uuid");
    require_all_assignees(&fx, task_id, &[(reviewer_id, true), (developer_id, true)]).await;

    // 指摘を閉じて（全員完了なので完了する）、もう一度繰り延べる
    assert_eq!(
        transition(&fx, &finding_id, "open").await.status(),
        StatusCode::OK
    );
    assert!(task_row(&fx, task_id).await.completed_at.is_some());
    assert_eq!(completed_assignees(&fx, task_id).await, 2);

    let again = json(transition(&fx, &finding_id, "deferred").await).await;
    assert_eq!(again["deferred_task_id"], task_id.to_string());
    let reopened = task_row(&fx, task_id).await;
    assert!(reopened.completed_at.is_none(), "未完了に戻る");
    assert_eq!(
        completed_assignees(&fx, task_id).await,
        0,
        "担当者の完了は全員分消える"
    );

    // 対照: 完了していないタスクの繰り延べ（open → deferred の往復で畳まれない場合）では消さない。
    // 担当者が 1 人完了済みのまま、完了前のタスクを開き直す
    let (_, other) = submit_round(&fx, 723, "low", "未完了で開き直し").await;
    let other_task: Uuid =
        json(transition(&fx, &other, "deferred").await).await["deferred_task_id"]
            .as_str()
            .expect("繰り延べ先タスク")
            .parse()
            .expect("uuid");
    require_all_assignees(
        &fx,
        other_task,
        &[(reviewer_id, true), (developer_id, false)],
    )
    .await;
    // 閉じても完了しない（未完了の担当者が残る）ので、そのまま開き直す
    assert_eq!(
        transition(&fx, &other, "open").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        transition(&fx, &other, "deferred").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        completed_assignees(&fx, other_task).await,
        1,
        "未完了のタスクを開き直しても、担当者の完了は消さない"
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 指摘ゼロのラウンドも正当（「指摘なし」の記録）。
#[tokio::test]
async fn a_round_without_findings_is_valid() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 704,
                "head_sha": "cccccccccccccccccccccccccccccccccccccccc",
                "summary": "具体的な不具合は見つからなかった",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = json(res).await;
    assert_eq!(body["round"], 1);
    assert_eq!(body["finding_count"], 0);
    assert_eq!(body["summary"], "具体的な不具合は見つからなかった");

    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=704", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["rounds"], 1);
    assert_eq!(summary["mergeable"], true);

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 指摘一覧は状態・重大度で絞り込め、綴り違いは黙って無視せず 400 にする。
#[tokio::test]
async fn findings_can_be_filtered_and_unknown_filters_are_rejected() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 705,
                "head_sha": "dddddddddddddddddddddddddddddddddddddddd",
                "findings": [
                    { "severity": "high", "title": "認可漏れ", "body": "本文" },
                    { "severity": "low", "title": "命名", "body": "本文" },
                    { "severity": "nit", "title": "表記", "body": "本文" },
                ],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let all = json(
        fx.app
            .get_with_session(&format!("{}?pr=705", fx.findings_path()))
            .await,
    )
    .await;
    assert_eq!(all.as_array().expect("findings").len(), 3);

    let filtered = json(
        fx.app
            .get_with_session(&format!("{}?pr=705&severity=low,nit", fx.findings_path()))
            .await,
    )
    .await;
    let filtered = filtered.as_array().expect("findings");
    assert_eq!(filtered.len(), 2, "重大度で絞り込める");
    assert!(
        filtered
            .iter()
            .all(|f| f["severity"] == "low" || f["severity"] == "nit")
    );

    let open_only = json(
        fx.app
            .get_with_session(&format!("{}?pr=705&state=open", fx.findings_path()))
            .await,
    )
    .await;
    assert_eq!(open_only.as_array().expect("findings").len(), 3);
    let verified_only = json(
        fx.app
            .get_with_session(&format!("{}?pr=705&state=verified", fx.findings_path()))
            .await,
    )
    .await;
    assert!(verified_only.as_array().expect("findings").is_empty());

    // 綴り違いを黙って無視すると絞り込みが効いていないことに気づけない。
    // 400 にはどのパラメーターのどの値が未知なのかを入れる（CLI から使う
    // レビュワーが「bad request」だけでは直しようがない）
    let res = fx
        .app
        .get_with_session(&format!("{}?pr=705&severity=critical", fx.findings_path()))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let message = json(res).await["message"]
        .as_str()
        .expect("message")
        .to_string();
    assert!(
        message.contains("severity") && message.contains("critical"),
        "パラメーター名と受け取った値が届く: {message}"
    );

    let res = fx
        .app
        .get_with_session(&format!("{}?pr=705&state=merged", fx.findings_path()))
        .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let message = json(res).await["message"]
        .as_str()
        .expect("message")
        .to_string();
    assert!(
        message.contains("state") && message.contains("merged"),
        "state 側も同じように届く: {message}"
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 他プロジェクト・他テナントの指摘は触れず、存在も漏らさない。
#[tokio::test]
async fn findings_are_scoped_to_their_project() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (review_id, finding_id) = submit_round(&fx, 706, "high", "認可漏れ").await;

    // 同じテナントの別プロジェクト経由では見えない
    let other = fx.app.insert_tenant_project(fx.reviewer.id).await;
    let other_reviews = format!(
        "/v1/tenants/{}/projects/{}/reviews",
        other.tenant_id, other.project_id
    );
    assert_eq!(
        fx.app
            .get_with_session(&format!("{other_reviews}/{review_id}"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fx.app
            .patch_json_with_session(
                &format!(
                    "/v1/tenants/{}/projects/{}/review-findings/{finding_id}",
                    other.tenant_id, other.project_id
                ),
                serde_json::json!({ "state": "fixed" }),
            )
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    // 対照: 本来のプロジェクト経由なら通る
    assert_eq!(
        fx.app
            .get_with_session(&format!("{}/{review_id}", fx.reviews_path()))
            .await
            .status(),
        StatusCode::OK
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 未ログインは 401。テナントに入れない利用者は 403。
#[tokio::test]
async fn access_requires_a_session_and_tenant_membership() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    submit_round(&fx, 707, "high", "認可漏れ").await;

    fx.app.reset_session_client();
    assert_eq!(
        fx.app
            .get_with_session(&format!("{}?pr=707", fx.reviews_path()))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let outsider = fx.app.insert_user_default().await;
    fx.login(&outsider.clone()).await;
    assert_eq!(
        fx.app
            .get_with_session(&format!("{}?pr=707", fx.reviews_path()))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
    fx.app.cleanup_user(outsider.id).await;
}

/// 読み取りも PR 番号が 0 以下なら 400。
///
/// 起票だけ弾いて読み取りを通すと、`?pr=0` は空一覧・`mergeable: false` という
/// 正常応答になる。「そんな PR は無い」と「未レビューなので通せない」が同じ形に
/// 見えるので、ゲートとして使う側は綴り違いに気づけない（仕様 §10）。
#[tokio::test]
async fn read_apis_reject_a_non_positive_pr_number() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    let reviews = fx.reviews_path();
    let findings = fx.findings_path();
    for path in [
        format!("{reviews}?pr=0"),
        format!("{reviews}?pr=-1"),
        format!("{reviews}/summary?pr=0"),
        format!("{findings}?pr=0"),
    ] {
        assert_eq!(
            fx.app.get_with_session(&path).await.status(),
            StatusCode::BAD_REQUEST,
            "PR 番号が 0 以下なら弾く: {path}"
        );
    }

    // 対照: 1 以上なら、そのラウンドが無くても正常に読める（過剰拒否でない）
    for path in [
        format!("{reviews}?pr=1"),
        format!("{reviews}/summary?pr=1"),
        format!("{findings}?pr=1"),
    ] {
        assert_eq!(
            fx.app.get_with_session(&path).await.status(),
            StatusCode::OK,
            "1 以上は通す: {path}"
        );
    }

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 起票は head_sha が 40 桁の小文字 16 進でなければ 400。
///
/// マージ可否のゲートは `latest_head_sha` を厳密一致で比べる。短縮 SHA を受け取ると、
/// そのラウンドは指摘を全部解消しても通らなくなり、しかも「同じ commit に見えるのに
/// 再レビューを要求される」形で出るので、原因が投入時の書き方にあることを辿れない。
#[tokio::test]
async fn a_round_requires_a_full_commit_sha() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;

    for sha in [
        "60cdd77",                                   // 短縮（git log --oneline の形）
        "60CDD7795F94FA4E4148CE996C2EFB4C363E3F5E",  // 大文字
        "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e0", // 41 桁
        "zzcdd7795f94fa4e4148ce996c2efb4c363e3f5e",  // 16 進でない
    ] {
        let res = fx
            .app
            .post_json_with_session(
                &fx.reviews_path(),
                serde_json::json!({
                    "pr_number": 731,
                    "head_sha": sha,
                    "findings": [],
                }),
            )
            .await;
        assert_eq!(
            res.status(),
            StatusCode::BAD_REQUEST,
            "head_sha を弾く: {sha}"
        );
    }

    // 対照: 40 桁の小文字 16 進なら通る（過剰拒否でない）
    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 731,
                "head_sha": "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 指摘一覧の `available_actions` を id で引く。
async fn actions_in_list(fx: &Fixture, pr: i32, finding_id: &str) -> serde_json::Value {
    let list = json(
        fx.app
            .get_with_session(&format!("{}?pr={pr}", fx.findings_path()))
            .await,
    )
    .await;
    list.as_array()
        .expect("findings")
        .iter()
        .find(|f| f["id"] == finding_id)
        .unwrap_or_else(|| panic!("finding {finding_id} in {list}"))["available_actions"]
        .clone()
}

/// ラウンド詳細の `available_actions`（1 件目の指摘）。
async fn actions_in_detail(fx: &Fixture, review_id: &str) -> serde_json::Value {
    let detail = json(
        fx.app
            .get_with_session(&format!("{}/{review_id}", fx.reviews_path()))
            .await,
    )
    .await;
    detail["findings"][0]["available_actions"].clone()
}

/// `available_actions` は要求者ごとに、仕様 §3 の役割規則どおりの遷移先だけを返す。
/// 一覧・ラウンド詳細・遷移の応答で同じ値になる。
#[tokio::test]
async fn available_actions_follow_the_role_rules() {
    let mut fx = setup().await;
    let actions = |values: &[&str]| serde_json::json!(values);

    // レビュワーが Low と High を出す
    fx.login(&fx.reviewer.clone()).await;
    let (low_review, low) = submit_round(&fx, 801, "low", "Low の指摘").await;
    let (_, high) = submit_round(&fx, 802, "high", "High の指摘").await;

    // 指摘の作者: 修正・繰り延べ・取り下げ。High には繰り延べが出ない
    assert_eq!(
        actions_in_list(&fx, 801, &low).await,
        actions(&["fixing", "fixed", "deferred", "rejected"])
    );
    assert_eq!(
        actions_in_detail(&fx, &low_review).await,
        actions(&["fixing", "fixed", "deferred", "rejected"]),
        "ラウンド詳細も同じ値"
    );
    assert_eq!(
        actions_in_list(&fx, 802, &high).await,
        actions(&["fixing", "fixed", "rejected"])
    );

    // 作者でない修正者には取り下げが出ない
    fx.login(&fx.developer.clone()).await;
    assert_eq!(
        actions_in_list(&fx, 801, &low).await,
        actions(&["fixing", "fixed", "deferred"])
    );
    assert_eq!(
        actions_in_list(&fx, 802, &high).await,
        actions(&["fixing", "fixed"])
    );

    // 修正者が fixed を宣言する。修正者はレビュー側でないので確認も差し戻しも出ない
    let res = transition(&fx, &low, "fixed").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        json(res).await["available_actions"],
        actions(&[]),
        "遷移の応答も要求者の値"
    );
    // レビュー側には確認と差し戻し
    fx.login(&fx.reviewer.clone()).await;
    assert_eq!(
        actions_in_list(&fx, 801, &low).await,
        actions(&["open", "verified"])
    );

    // fixed を宣言した本人には、レビュー側でも verified が出ない
    assert_eq!(
        transition(&fx, &high, "fixed").await.status(),
        StatusCode::OK
    );
    assert_eq!(actions_in_list(&fx, 802, &high).await, actions(&["open"]));

    // verified は終端で空
    assert_eq!(
        transition(&fx, &low, "verified").await.status(),
        StatusCode::OK
    );
    assert_eq!(actions_in_list(&fx, 801, &low).await, actions(&[]));

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

#[tokio::test]
async fn available_actions_require_write_review_scope() {
    let fx = setup().await;
    let (review_id, finding_id) = submit_round(&fx, 804, "low", "読み取り専用の指摘").await;
    let read_only = fx
        .app
        .insert_pat(fx.reviewer.id, fx.tenant_id, vec![Scope::ReadReview], None)
        .await;
    let list = json(
        fx.app
            .get_with_bearer(&format!("{}?pr=804", fx.findings_path()), &read_only)
            .await,
    )
    .await;
    assert_eq!(list[0]["available_actions"], serde_json::json!([]));
    let detail = json(
        fx.app
            .get_with_bearer(&format!("{}/{review_id}", fx.reviews_path()), &read_only)
            .await,
    )
    .await;
    assert_eq!(
        detail["findings"][0]["available_actions"],
        serde_json::json!([])
    );

    let writer = fx
        .app
        .insert_pat(
            fx.reviewer.id,
            fx.tenant_id,
            vec![Scope::ReadReview, Scope::WriteReview],
            None,
        )
        .await;
    let list = json(
        fx.app
            .get_with_bearer(&format!("{}?pr=804", fx.findings_path()), &writer)
            .await,
    )
    .await;
    assert_eq!(list[0]["id"], finding_id);
    assert_eq!(
        list[0]["available_actions"],
        serde_json::json!(["fixing", "fixed", "deferred", "rejected"])
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 作者がテナントから居なくなったときだけ、オーナーに取り下げが出る。
#[tokio::test]
async fn available_actions_offer_the_owner_rejection_for_a_departed_reviewer() {
    let mut fx = setup().await;
    fx.login(&fx.developer.clone()).await;
    let (_, finding) = submit_round(&fx, 803, "low", "作者が去る指摘").await;

    fx.login(&fx.reviewer.clone()).await;
    assert_eq!(
        actions_in_list(&fx, 803, &finding).await,
        serde_json::json!(["fixing", "fixed", "deferred"]),
        "作者が在籍していればオーナーでも出ない"
    );

    tenant_members::Entity::delete_many()
        .filter(tenant_members::Column::TenantId.eq(fx.tenant_id))
        .filter(tenant_members::Column::UserId.eq(fx.developer.id))
        .exec(&fx.app.state.db)
        .await
        .expect("remove tenant member");

    assert_eq!(
        actions_in_list(&fx, 803, &finding).await,
        serde_json::json!(["fixing", "fixed", "deferred", "rejected"])
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

async fn gate(fx: &Fixture, pr: i32) -> serde_json::Value {
    json(
        fx.app
            .get_with_session(&format!("{}/summary?pr={pr}", fx.reviews_path()))
            .await,
    )
    .await["gate"]
        .clone()
}

/// 最新ラウンドの `pr_head_sha`（要約ジョブが GitHub から読んだ現在の head）を書き換える。
async fn set_cached_head(fx: &Fixture, review_id: &str, sha: &str) {
    let mut active: reviews::ActiveModel =
        reviews::Entity::find_by_id(review_id.parse::<Uuid>().expect("uuid"))
            .one(&fx.app.state.db)
            .await
            .expect("select")
            .expect("review")
            .into();
    active.pr_head_sha = Set(Some(sha.into()));
    active.pr_head_checked_at = Set(Some(chrono::Utc::now().into()));
    active.update(&fx.app.state.db).await.expect("update head");
}

/// `gate` の 6 値と、条件が重なったときの優先順位（仕様 §5）。
#[tokio::test]
async fn summary_gate_takes_the_first_matching_condition() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let reviewed = "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e";
    let pushed = "0000000000000000000000000000000000000001";

    // 連携なし: 未解決の High があっても unlinked が先
    let (unlinked_review, _) = submit_round(&fx, 804, "high", "連携前").await;
    set_cached_head(&fx, &unlinked_review, pushed).await;
    assert_eq!(gate(&fx, 804).await, "unlinked");

    link_repo(&fx, "acme", "gate").await;
    assert_eq!(gate(&fx, 805).await, "unreviewed");

    // 未解決の High があれば、レビュー後にコミットが積まれていても blocked が先
    let (blocked_review, _) = submit_round(&fx, 805, "high", "未解決").await;
    assert_eq!(gate(&fx, 805).await, "blocked");
    set_cached_head(&fx, &blocked_review, pushed).await;
    assert_eq!(gate(&fx, 805).await, "blocked");

    // 未解決が無い PR: 鮮度で決まる
    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": 806,
                "head_sha": reviewed,
                "summary": "指摘なし",
                "findings": [],
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let clean_review = json(res).await["id"].as_str().expect("id").to_string();
    assert_eq!(gate(&fx, 806).await, "stale_unknown");
    set_cached_head(&fx, &clean_review, pushed).await;
    assert_eq!(gate(&fx, 806).await, "outdated");
    set_cached_head(&fx, &clean_review, reviewed).await;
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=806", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["gate"], "ready");
    assert!(
        !summary["pr_head_checked_at"].is_null(),
        "確認時刻を併記する"
    );
    assert_eq!(summary["mergeable"], true, "既存フィールドは残す");

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

// ── 引き継ぎ（TASK-495） ─────────────────────────────────────────────────

const OLD_HEAD: &str = "60cdd7795f94fa4e4148ce996c2efb4c363e3f5e";
const NEW_HEAD: &str = "1111111111111111111111111111111111111111";

/// 指定の head で、`(severity, title)` の指摘を持つラウンドを起票し、(review_id, 指摘の id の並び) を返す。
async fn submit_round_with(
    fx: &Fixture,
    pr: i32,
    head_sha: &str,
    findings: &[(&str, &str)],
) -> (String, Vec<String>) {
    let findings: Vec<serde_json::Value> = findings
        .iter()
        .map(|(severity, title)| {
            serde_json::json!({
                "severity": severity,
                "title": title,
                "body": "再現条件と根拠",
                "file": "apps/frontend/src/App.vue",
                "line": 42,
            })
        })
        .collect();
    let res = fx
        .app
        .post_json_with_session(
            &fx.reviews_path(),
            serde_json::json!({
                "pr_number": pr,
                "head_sha": head_sha,
                "summary": "総評",
                "findings": findings,
            }),
        )
        .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = json(res).await;
    (
        body["id"].as_str().expect("review id").to_string(),
        body["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .map(|f| f["id"].as_str().expect("finding id").to_string())
            .collect(),
    )
}

async fn carry_over(
    fx: &Fixture,
    from_pr: i32,
    to_pr: i32,
    from_repo: Option<&str>,
) -> reqwest::Response {
    fx.app
        .post_json_with_session(
            &format!("{}/carry-over", fx.reviews_path()),
            serde_json::json!({
                "from_pr_number": from_pr,
                "from_repo": from_repo,
                "to_pr_number": to_pr,
            }),
        )
        .await
}

async fn findings_of(fx: &Fixture, pr: i32) -> Vec<serde_json::Value> {
    json(
        fx.app
            .get_with_session(&format!("{}?pr={pr}", fx.findings_path()))
            .await,
    )
    .await
    .as_array()
    .expect("findings")
    .clone()
}

fn finding_titled<'a>(findings: &'a [serde_json::Value], title: &str) -> &'a serde_json::Value {
    findings
        .iter()
        .find(|f| f["title"] == title)
        .unwrap_or_else(|| panic!("{title} の指摘が無い: {findings:?}"))
}

/// 修正者でも引き継ぎ者でもない利用者（テナントメンバー）を足す。
async fn add_member(fx: &mut Fixture) -> crate::common::TestUser {
    let user = fx.app.insert_user_default().await;
    fx.login(&fx.reviewer.clone()).await;
    let added = fx
        .app
        .post_json_with_session(
            &format!("/v1/tenants/{}/members", fx.tenant_id),
            serde_json::json!({ "user_id": user.id, "role": "Member" }),
        )
        .await;
    assert_eq!(added.status(), StatusCode::CREATED);
    user
}

/// 未解決の指摘だけが、作成者と head を保ったまま新しいラウンドへ写る。
#[tokio::test]
async fn carry_over_copies_unresolved_findings_into_new_rounds() {
    let mut fx = setup().await;
    link_repo(&fx, "acme", "app").await;
    fx.login(&fx.reviewer.clone()).await;

    // 元 PR: R1 に 4 件（open / fixed / verified / rejected）、R2 は指摘なし
    let (review_1, ids) = submit_round_with(
        &fx,
        790,
        OLD_HEAD,
        &[
            ("high", "open のまま"),
            ("medium", "fixed のまま"),
            ("low", "verified 済み"),
            ("nit", "rejected 済み"),
        ],
    )
    .await;
    submit_round_with(&fx, 790, NEW_HEAD, &[]).await;
    fx.login(&fx.developer.clone()).await;
    assert_eq!(
        transition(&fx, &ids[1], "fixed").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        transition(&fx, &ids[2], "fixed").await.status(),
        StatusCode::OK
    );
    fx.login(&fx.reviewer.clone()).await;
    assert_eq!(
        transition(&fx, &ids[2], "verified").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        transition(&fx, &ids[3], "rejected").await.status(),
        StatusCode::OK
    );
    // 要求者ごとに変わる `available_actions` は比べない
    let source_state = |findings: Vec<serde_json::Value>| -> Vec<(String, String, usize)> {
        let mut out: Vec<_> = findings
            .iter()
            .map(|f| {
                (
                    f["id"].as_str().expect("id").to_string(),
                    f["state"].as_str().expect("state").to_string(),
                    f["transitions"].as_array().expect("transitions").len(),
                )
            })
            .collect();
        out.sort();
        out
    };
    let source_before = source_state(findings_of(&fx, 790).await);

    // 修正者が引き継ぐ
    fx.login(&fx.developer.clone()).await;
    let res = carry_over(&fx, 790, 1119, None).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = json(res).await;
    let rounds = body["rounds"].as_array().expect("rounds");
    assert_eq!(
        rounds.len(),
        1,
        "未解決を持つ元ラウンド 1 本につき写し 1 本"
    );
    let copy = &rounds[0];
    assert_eq!(copy["pr_number"], 1119);
    assert_eq!(copy["round"], 1);
    assert_eq!(
        copy["head_sha"], OLD_HEAD,
        "元のラウンドが見た commit のまま"
    );
    assert_eq!(
        copy["reviewer"]["id"],
        fx.reviewer.id.to_string(),
        "作成者は元のまま"
    );
    assert_eq!(copy["carried_by"]["id"], fx.developer.id.to_string());
    assert_eq!(copy["carried_from"]["review_id"], review_1);
    assert_eq!(copy["carried_from"]["pr_number"], 790);
    assert_eq!(copy["carried_from"]["round"], 1);
    assert_eq!(copy["finding_count"], 2);

    let copied = copy["findings"].as_array().expect("findings");
    assert_eq!(copied.len(), 2, "open と fixed だけ: {copied:?}");
    let open = finding_titled(copied, "open のまま");
    assert_eq!(open["state"], "open");
    assert_eq!(open["severity"], "high");
    assert_eq!(open["file"], "apps/frontend/src/App.vue");
    assert_eq!(open["line"], 42);
    assert_eq!(open["carried_from_finding_id"], ids[0]);
    let fixed = finding_titled(copied, "fixed のまま");
    assert_eq!(fixed["state"], "fixed", "状態は元のまま");
    assert_eq!(
        fixed["fixed_by"],
        fx.developer.id.to_string(),
        "fixed_by も保つ"
    );
    assert_eq!(fixed["carried_from_finding_id"], ids[1]);
    let history = open["transitions"].as_array().expect("transitions");
    assert_eq!(history.len(), 1);
    assert!(history[0]["from_state"].is_null());
    assert_eq!(history[0]["actor"]["id"], fx.developer.id.to_string());
    assert!(
        history[0]["note"]
            .as_str()
            .expect("note")
            .contains("PR #790 R1"),
        "出どころが履歴に残る: {history:?}"
    );

    // 引き継ぎ先の一覧にも出る。元 PR は件数も状態も変わらない
    assert_eq!(findings_of(&fx, 1119).await.len(), 2);
    assert_eq!(source_state(findings_of(&fx, 790).await), source_before);
    assert!(
        findings_of(&fx, 790)
            .await
            .iter()
            .all(|f| f["carried_from_finding_id"].is_null())
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 写しのラウンドは指摘の件数には入るが、レビューには数えない。
#[tokio::test]
async fn carried_rounds_do_not_count_as_a_review() {
    let mut fx = setup().await;
    link_repo(&fx, "acme", "app").await;
    fx.login(&fx.reviewer.clone()).await;
    submit_round_with(&fx, 790, OLD_HEAD, &[("high", "未解決")]).await;

    // 写しだけの PR は未レビュー（塞いでいる件数には入る）
    assert_eq!(
        carry_over(&fx, 790, 1119, None).await.status(),
        StatusCode::CREATED
    );
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=1119", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["gate"], "unreviewed");
    assert_eq!(summary["mergeable"], false);
    assert_eq!(summary["blocking"], 1);
    assert_eq!(summary["rounds"], 0);
    assert!(
        summary["latest_head_sha"].is_null(),
        "写しの head を最新にしない"
    );

    // この PR へ自分でラウンドを出せば、その head が最新になる
    submit_round_with(&fx, 1119, NEW_HEAD, &[]).await;
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=1119", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["gate"], "blocked");
    assert_eq!(summary["latest_head_sha"], NEW_HEAD);
    assert_eq!(summary["blocking"], 1, "写しの指摘は塞いだまま");

    // 対照: レビュー済みの PR へ Low だけを引き継いでも、最新の head は動かず、マージ可のまま
    submit_round_with(&fx, 791, OLD_HEAD, &[("low", "軽い指摘")]).await;
    submit_round_with(&fx, 900, NEW_HEAD, &[]).await;
    assert_eq!(
        carry_over(&fx, 791, 900, None).await.status(),
        StatusCode::CREATED
    );
    let summary = json(
        fx.app
            .get_with_session(&format!("{}/summary?pr=900", fx.reviews_path()))
            .await,
    )
    .await;
    assert_eq!(summary["latest_head_sha"], NEW_HEAD);
    assert_eq!(summary["blocking"], 0);
    assert_eq!(summary["mergeable"], true);

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 引き継いだ人は、写しの指摘の `rejected` / `verified` の権限を得ない。
#[tokio::test]
async fn carrying_over_does_not_move_the_right_to_reject() {
    let mut fx = setup().await;
    link_repo(&fx, "acme", "app").await;
    let carrier = add_member(&mut fx).await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, ids) = submit_round_with(
        &fx,
        790,
        OLD_HEAD,
        &[("high", "棄却したい指摘"), ("medium", "確認したい修正")],
    )
    .await;
    fx.login(&fx.developer.clone()).await;
    assert_eq!(
        transition(&fx, &ids[1], "fixed").await.status(),
        StatusCode::OK
    );

    // 引き継ぐのは、修正者でも作成者でもない第三者
    fx.login(&carrier).await;
    assert_eq!(
        carry_over(&fx, 790, 1119, None).await.status(),
        StatusCode::CREATED
    );
    let copies = findings_of(&fx, 1119).await;
    let high = finding_titled(&copies, "棄却したい指摘");
    let medium = finding_titled(&copies, "確認したい修正");
    let high_id = high["id"].as_str().expect("id").to_string();
    let medium_id = medium["id"].as_str().expect("id").to_string();
    assert!(
        !high["available_actions"]
            .as_array()
            .expect("actions")
            .contains(&"rejected".into()),
        "引き継いだ人に rejected は出ない: {high}"
    );
    assert!(
        !medium["available_actions"]
            .as_array()
            .expect("actions")
            .contains(&"verified".into()),
        "引き継いだ人に verified は出ない: {medium}"
    );
    assert_eq!(
        transition(&fx, &high_id, "rejected").await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        transition(&fx, &medium_id, "verified").await.status(),
        StatusCode::FORBIDDEN
    );

    // 対照: 元のラウンドの作成者は、引き継ぎ先でも行える
    fx.login(&fx.reviewer.clone()).await;
    let copies = findings_of(&fx, 1119).await;
    let actions = |title: &str| -> Vec<serde_json::Value> {
        finding_titled(&copies, title)["available_actions"]
            .as_array()
            .expect("actions")
            .clone()
    };
    assert!(actions("棄却したい指摘").contains(&"rejected".into()));
    assert!(actions("確認したい修正").contains(&"verified".into()));
    assert_eq!(
        transition(&fx, &high_id, "rejected").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        transition(&fx, &medium_id, "verified").await.status(),
        StatusCode::OK
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
    fx.app.cleanup_user(carrier.id).await;
}

/// 再実行しても増えず、元 PR に指摘が増えた分だけ写る。
#[tokio::test]
async fn carrying_over_twice_does_not_duplicate() {
    let mut fx = setup().await;
    link_repo(&fx, "acme", "app").await;
    fx.login(&fx.reviewer.clone()).await;
    submit_round_with(&fx, 790, OLD_HEAD, &[("high", "最初の指摘")]).await;

    assert_eq!(
        carry_over(&fx, 790, 1119, None).await.status(),
        StatusCode::CREATED
    );
    let again = carry_over(&fx, 790, 1119, None).await;
    assert_eq!(again.status(), StatusCode::CONFLICT);
    assert!(
        json(again).await["message"]
            .as_str()
            .expect("message")
            .contains("引き継げる指摘がありません")
    );
    assert_eq!(findings_of(&fx, 1119).await.len(), 1, "増えない");

    // 写しが解消されても、元の指摘は引き継ぎ済みとして数える
    let copy = findings_of(&fx, 1119).await[0]["id"]
        .as_str()
        .expect("id")
        .to_string();
    assert_eq!(
        transition(&fx, &copy, "rejected").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        carry_over(&fx, 790, 1119, None).await.status(),
        StatusCode::CONFLICT
    );

    // 元 PR に新しい指摘が増えたら、その 1 件だけが新しいラウンドへ写る
    submit_round_with(&fx, 790, NEW_HEAD, &[("medium", "後から増えた指摘")]).await;
    let third = carry_over(&fx, 790, 1119, None).await;
    assert_eq!(third.status(), StatusCode::CREATED);
    let body = json(third).await;
    let rounds = body["rounds"].as_array().expect("rounds");
    assert_eq!(rounds.len(), 1);
    assert_eq!(rounds[0]["round"], 2, "引き継ぎ先の採番は続く");
    assert_eq!(rounds[0]["head_sha"], NEW_HEAD);
    assert_eq!(rounds[0]["findings"].as_array().expect("findings").len(), 1);
    assert_eq!(findings_of(&fx, 1119).await.len(), 2);

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

#[tokio::test]
async fn carry_over_rejects_bad_requests() {
    let mut fx = setup().await;
    link_repo(&fx, "acme", "app").await;
    fx.login(&fx.reviewer.clone()).await;
    submit_round_with(&fx, 790, OLD_HEAD, &[("high", "未解決")]).await;
    submit_round_with(&fx, 792, OLD_HEAD, &[("nit", "棄却済み")]).await;
    let nit = findings_of(&fx, 792).await[0]["id"]
        .as_str()
        .expect("id")
        .to_string();
    assert_eq!(
        transition(&fx, &nit, "rejected").await.status(),
        StatusCode::OK
    );

    // 同じ PR（既定の連携先も、同じリポジトリの明示も）
    assert_eq!(
        carry_over(&fx, 790, 790, None).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        carry_over(&fx, 790, 790, Some("acme/app")).await.status(),
        StatusCode::BAD_REQUEST
    );
    // PR 番号は 1 以上、from_repo は owner/name
    assert_eq!(
        carry_over(&fx, 0, 1119, None).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        carry_over(&fx, 790, 0, None).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        carry_over(&fx, 790, 1119, Some("x")).await.status(),
        StatusCode::BAD_REQUEST
    );
    // 元ラウンドが無い（番号違い）と、引き継げる指摘が無い（解決済みだけ）は 409 で理由を返す
    let missing = carry_over(&fx, 999, 1119, None).await;
    assert_eq!(missing.status(), StatusCode::CONFLICT);
    assert!(
        json(missing).await["message"]
            .as_str()
            .expect("message")
            .contains("レビューラウンドがありません")
    );
    let resolved = carry_over(&fx, 792, 1119, None).await;
    assert_eq!(resolved.status(), StatusCode::CONFLICT);
    assert!(
        json(resolved).await["message"]
            .as_str()
            .expect("message")
            .contains("引き継げる指摘がありません")
    );
    assert!(
        findings_of(&fx, 1119).await.is_empty(),
        "拒否した引き継ぎは何も作らない"
    );

    // write:review の無い PAT
    let read_only = fx
        .app
        .insert_pat(fx.reviewer.id, fx.tenant_id, vec![Scope::ReadReview], None)
        .await;
    let res = fx
        .app
        .post_json_with_bearer(
            &format!("{}/carry-over", fx.reviews_path()),
            serde_json::json!({ "from_pr_number": 790, "to_pr_number": 1119 }),
            &read_only,
        )
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // テナント外の利用者
    let outsider = fx.app.insert_user_default().await;
    fx.login(&outsider.clone()).await;
    assert_eq!(
        carry_over(&fx, 790, 1119, None).await.status(),
        StatusCode::FORBIDDEN
    );

    // 対照: 正しい入力は通る
    fx.login(&fx.developer.clone()).await;
    assert_eq!(
        carry_over(&fx, 790, 1119, None).await.status(),
        StatusCode::CREATED
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
    fx.app.cleanup_user(outsider.id).await;
}

/// 過去の連携先のラウンドも、`from_repo` で指せば引き継げる。
#[tokio::test]
async fn carry_over_reads_the_source_from_a_past_repository() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let old_integration = link_repo(&fx, "acme", "old-repo").await;
    submit_round_with(&fx, 711, OLD_HEAD, &[("high", "旧リポジトリの指摘")]).await;

    github_integrations::Entity::delete_by_id(old_integration)
        .exec(&fx.app.state.db)
        .await
        .expect("delete integration");
    link_repo(&fx, "acme", "new-repo").await;

    // 既定（現在の連携先）には元ラウンドが無い
    assert_eq!(
        carry_over(&fx, 711, 712, None).await.status(),
        StatusCode::CONFLICT
    );

    // 旧リポジトリを指せば、同じ PR 番号へも引き継げる
    let res = carry_over(&fx, 711, 711, Some("acme/old-repo")).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = json(res).await;
    let copy = &body["rounds"][0];
    assert_eq!(copy["carried_from"]["repository"], "acme/old-repo");
    assert_eq!(copy["carried_from"]["pr_number"], 711);
    let history = &copy["findings"][0]["transitions"][0];
    assert!(
        history["note"]
            .as_str()
            .expect("note")
            .contains("acme/old-repo の PR #711 R1"),
        "別リポジトリの出どころが履歴に残る: {history}"
    );
    assert_eq!(findings_of(&fx, 711).await.len(), 1, "現在の連携先に写る");

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 写しの鎖と分岐は大元ごとに一件で、元 PR への帰還でも増えない。
#[tokio::test]
async fn carry_over_roots_do_not_duplicate_across_paths() {
    let mut fx = setup().await;
    link_repo(&fx, "acme", "app").await;
    fx.login(&fx.reviewer.clone()).await;
    submit_round_with(&fx, 790, OLD_HEAD, &[("high", "root")]).await;
    for (from, to, expected) in [
        (790, 791, StatusCode::CREATED),
        (791, 792, StatusCode::CREATED),
        (792, 790, StatusCode::CONFLICT),
        (790, 792, StatusCode::CONFLICT),
        (790, 793, StatusCode::CREATED),
        (791, 793, StatusCode::CONFLICT),
    ] {
        assert_eq!(
            carry_over(&fx, from, to, None).await.status(),
            expected,
            "{from} -> {to}"
        );
    }
    let listed = json(
        fx.app
            .get_with_session(&format!("{}/pull-requests", fx.reviews_path()))
            .await,
    )
    .await;
    let prs = listed.as_array().expect("PR list");
    for pr in prs {
        if pr["pr_number"] != 790 {
            assert_eq!(pr["rounds"], 0);
            assert!(pr["last_reviewed_at"].is_null());
        }
    }
    for pr in [790, 791, 792, 793] {
        assert_eq!(findings_of(&fx, pr).await.len(), 1);
    }
    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 遷移がプロジェクト FK を待つ間も、引き継ぎは先に元の指摘を待つ。
/// 時間ではなく pg_blocking_pids で順序を確認する。繰り延べのタスク作成も含む。
#[tokio::test]
async fn carry_over_waits_for_source_transition_before_numbering() {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement, TransactionTrait};
    use std::time::Duration;
    for state in ["fixed", "deferred"] {
        let mut fx = setup().await;
        link_repo(&fx, "acme", "app").await;
        fx.login(&fx.reviewer.clone()).await;
        let (_, finding) = submit_round(&fx, 790, "low", "concurrent").await;
        fx.login(&fx.developer.clone()).await;
        let db = &fx.app.state.db;
        let holder = db.begin().await.unwrap();
        let pid: i32 = holder
            .query_one_raw(Statement::from_string(
                DatabaseBackend::Postgres,
                "SELECT pg_backend_pid() AS pid",
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get("", "pid")
            .unwrap();
        holder
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT id FROM projects WHERE id = $1 FOR UPDATE",
                [fx.project_id.into()],
            ))
            .await
            .unwrap();
        let control = async {
            let transition_pid = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let row = db.query_one_raw(Statement::from_sql_and_values(DatabaseBackend::Postgres,
                        "SELECT pid FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)) LIMIT 1", [pid.into()])).await.unwrap();
                    if let Some(row) = row { break row.try_get::<i32>("", "pid").unwrap(); }
                    tokio::task::yield_now().await;
                }
            }).await.expect("transition holds finding and waits for project");
            let observe = async {
                let waited = tokio::time::timeout(Duration::from_secs(10), async {
                    loop {
                        let row = db.query_one_raw(Statement::from_sql_and_values(DatabaseBackend::Postgres,
                            "SELECT pid FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)) AND query LIKE '%review_findings%' LIMIT 1", [transition_pid.into()])).await.unwrap();
                        if row.is_some() { break; }
                        tokio::task::yield_now().await;
                    }
                }).await;
                holder.rollback().await.unwrap();
                waited
            };
            let (response, waited) = tokio::join!(carry_over(&fx, 790, 791, None), observe);
            waited.expect("carry-over must wait on the source finding before the project");
            assert_eq!(
                response.status(),
                if state == "deferred" {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::CREATED
                }
            );
        };
        let (changed, ()) = tokio::join!(transition(&fx, &finding, state), control);
        assert_eq!(changed.status(), StatusCode::OK);
        if state == "fixed" {
            assert_eq!(findings_of(&fx, 791).await[0]["state"], "fixed");
        }
        fx.app.cleanup_user(fx.reviewer.id).await;
        fx.app.cleanup_user(fx.developer.id).await;
    }
}

/// PR 一覧のレビュー時刻と番号は、後から写しを追加しても変わらない。
#[tokio::test]
async fn carried_rounds_do_not_change_pull_request_review_metadata() {
    let mut fx = setup().await;
    link_repo(&fx, "acme", "app").await;
    fx.login(&fx.reviewer.clone()).await;
    submit_round_with(&fx, 790, OLD_HEAD, &[("low", "source")]).await;
    submit_round_with(&fx, 791, NEW_HEAD, &[]).await;
    let before = json(
        fx.app
            .get_with_session(&format!("{}/pull-requests", fx.reviews_path()))
            .await,
    )
    .await;
    let original = before
        .as_array()
        .unwrap()
        .iter()
        .find(|pr| pr["pr_number"] == 791)
        .unwrap();
    for to in [791, 792] {
        assert_eq!(
            carry_over(&fx, 790, to, None).await.status(),
            StatusCode::CREATED
        );
    }
    let after = json(
        fx.app
            .get_with_session(&format!("{}/pull-requests", fx.reviews_path()))
            .await,
    )
    .await;
    for pr in after.as_array().unwrap() {
        match pr["pr_number"].as_i64().unwrap() {
            791 => {
                assert_eq!(pr["rounds"], original["rounds"]);
                assert_eq!(pr["last_reviewed_at"], original["last_reviewed_at"]);
            }
            792 => {
                assert_eq!(pr["rounds"], 0);
                assert!(pr["last_reviewed_at"].is_null());
            }
            _ => {}
        }
    }
    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 採番のロックは通知・タスクのプロジェクト FK 検査と競合しない。
#[tokio::test]
async fn round_numbering_does_not_block_project_foreign_keys() {
    use sea_orm::{ConnectionTrait, DatabaseBackend, Statement, TransactionTrait};
    let fx = setup().await;
    let db = &fx.app.state.db;
    let numbering = db.begin().await.unwrap();
    let repo = service::reviews::RepoRef {
        owner: String::new(),
        name: String::new(),
        integration_id: None,
    };
    assert_eq!(
        service::reviews::next_round(&numbering, fx.project_id, &repo, 790)
            .await
            .unwrap(),
        1
    );
    let foreign_key = db.begin().await.unwrap();
    let compatible = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        foreign_key.execute_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT id FROM projects WHERE id = $1 FOR KEY SHARE",
            [fx.project_id.into()],
        )),
    )
    .await;
    numbering.rollback().await.unwrap();
    foreign_key.rollback().await.unwrap();
    compatible
        .expect("numbering must allow FK key-share locks")
        .unwrap();
    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// ラウンドの作成者は open の指摘の内容を直せ、変わった欄だけが前後の値つきで残る（TASK-577）。
#[tokio::test]
async fn the_round_author_can_edit_an_open_finding_and_the_edit_is_recorded() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding) = submit_round(&fx, 901, "medium", "元の題").await;
    assert!(
        finding_row(&fx, &finding).await.edited_at.is_none(),
        "直す前は edited_at が空"
    );

    let res = edit(
        &fx,
        &finding,
        serde_json::json!({
            "title": "直した題",
            "body": "直した本文",
            "file": null,
            "line": 7,
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = json(res).await;
    assert_eq!(body["severity"], "medium", "重大度は変わらない");
    assert_eq!(body["title"], "直した題");
    assert_eq!(body["body"], "直した本文");
    assert!(
        body["file"].is_null(),
        "null で位置のファイルを消す: {body}"
    );
    assert_eq!(body["line"], 7);
    assert!(body["edited_at"].is_string(), "直した時刻を返す: {body}");
    assert_eq!(body["state"], "open", "直しで状態は変わらない");
    assert_eq!(
        body["transitions"].as_array().map(Vec::len),
        Some(1),
        "直しは状態の遷移の履歴に混ぜない: {body}"
    );

    let edits = edits_of(&fx, &finding).await;
    let edit_id = edits[0].3;
    assert_eq!(
        edits,
        vec![
            (
                "body".into(),
                Some("再現条件と根拠".into()),
                Some("直した本文".into()),
                edit_id,
                fx.reviewer.id
            ),
            (
                "file".into(),
                Some("apps/frontend/src/App.vue".into()),
                None,
                edit_id,
                fx.reviewer.id
            ),
            (
                "line".into(),
                Some("42".into()),
                Some("7".into()),
                edit_id,
                fx.reviewer.id
            ),
            (
                "title".into(),
                Some("元の題".into()),
                Some("直した題".into()),
                edit_id,
                fx.reviewer.id
            ),
        ],
        "変わった欄ごとに 1 行、同じ直しは同じ edit_id で残る"
    );

    // 一覧にも直した時刻が載る
    let list = json(
        fx.app
            .get_with_session(&format!("{}?pr=901", fx.findings_path()))
            .await,
    )
    .await;
    assert_eq!(list[0]["edited_at"], body["edited_at"]);

    // 今と同じ値だけを渡した直しは何も書かない（履歴も edited_at も変わらない）
    let res = edit(&fx, &finding, serde_json::json!({ "title": "直した題" })).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(json(res).await["edited_at"], body["edited_at"]);
    assert_eq!(edits_of(&fx, &finding).await.len(), 4);

    // 次の直しは別の edit_id で足される
    let res = edit(&fx, &finding, serde_json::json!({ "line": null })).await;
    assert_eq!(res.status(), StatusCode::OK);
    let edits = edits_of(&fx, &finding).await;
    assert_eq!(edits.len(), 5);
    let line_edits: Vec<_> = edits.iter().filter(|row| row.0 == "line").collect();
    assert_eq!(line_edits.len(), 2);
    assert!(
        line_edits
            .iter()
            .any(|row| row.1.as_deref() == Some("7") && row.2.is_none() && row.3 != edit_id)
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 直せるのは、その指摘を含むラウンドの作成者だけ（TASK-577）。
#[tokio::test]
async fn only_the_round_author_can_edit_a_finding() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding) = submit_round(&fx, 902, "high", "レビュワーの指摘").await;

    fx.login(&fx.developer.clone()).await;
    let res = edit(&fx, &finding, serde_json::json!({ "title": "他人が直す" })).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let row = finding_row(&fx, &finding).await;
    assert_eq!(row.title, "レビュワーの指摘", "断ったら指摘は変わらない");
    assert!(row.edited_at.is_none());
    assert!(edits_of(&fx, &finding).await.is_empty());

    // 対照: 作成者なら直せる
    fx.login(&fx.reviewer.clone()).await;
    assert_eq!(
        edit(
            &fx,
            &finding,
            serde_json::json!({ "title": "作成者が直す" })
        )
        .await
        .status(),
        StatusCode::OK
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 直せるのは open の間だけ。理由を本文に入れた 409 を返す（TASK-577）。
#[tokio::test]
async fn a_finding_that_is_not_open_cannot_be_edited() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding) = submit_round(&fx, 903, "medium", "修正済みにする指摘").await;

    fx.login(&fx.developer.clone()).await;
    assert_eq!(
        transition(&fx, &finding, "fixed").await.status(),
        StatusCode::OK
    );

    fx.login(&fx.reviewer.clone()).await;
    let res = edit(&fx, &finding, serde_json::json!({ "title": "後から直す" })).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let message = json(res).await["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        message.contains("fixed") && message.contains("open の間だけ"),
        "理由を返す: {message}"
    );
    let row = finding_row(&fx, &finding).await;
    assert_eq!(row.title, "修正済みにする指摘");
    assert!(edits_of(&fx, &finding).await.is_empty());

    // 対照: 差し戻して open にすれば直せる
    assert_eq!(
        transition(&fx, &finding, "open").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        edit(&fx, &finding, serde_json::json!({ "title": "後から直す" }))
            .await
            .status(),
        StatusCode::OK
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 直しの欄は起票と同じ上限で検め、通らなければ何も変えない（TASK-577）。
#[tokio::test]
async fn edits_are_validated_before_anything_changes() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding) = submit_round(&fx, 904, "low", "検めの指摘").await;

    for body in [
        serde_json::json!({}),
        serde_json::json!({ "line": 0 }),
        serde_json::json!({ "title": "" }),
        serde_json::json!({ "file": "a".repeat(1001) }),
    ] {
        let res = edit(&fx, &finding, body.clone()).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{body} は入力の誤り");
    }
    let row = finding_row(&fx, &finding).await;
    assert_eq!(row.title, "検めの指摘");
    assert_eq!(row.line, Some(42));
    assert!(row.edited_at.is_none());
    assert!(edits_of(&fx, &finding).await.is_empty());

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 直しは `write:review` の書き込みとして扱う（TASK-577）。
#[tokio::test]
async fn editing_a_finding_requires_the_write_review_scope() {
    let fx = setup().await;
    let (_, finding) = submit_round(&fx, 905, "low", "鍵の指摘").await;
    let path = format!("{}/{finding}/content", fx.findings_path());
    let body = serde_json::json!({ "title": "鍵で直す" });

    let read_only = fx
        .app
        .insert_pat(fx.reviewer.id, fx.tenant_id, vec![Scope::ReadReview], None)
        .await;
    let res = fx
        .app
        .patch_json_with_bearer(&path, body.clone(), &read_only)
        .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(finding_row(&fx, &finding).await.title, "鍵の指摘");

    // 対照: write:review を持つ鍵なら直せる
    let writer = fx
        .app
        .insert_pat(
            fx.reviewer.id,
            fx.tenant_id,
            vec![Scope::ReadReview, Scope::WriteReview],
            None,
        )
        .await;
    let res = fx.app.patch_json_with_bearer(&path, body, &writer).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(finding_row(&fx, &finding).await.title, "鍵で直す");

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

async fn summary_of(fx: &Fixture, pr: i32) -> serde_json::Value {
    json(
        fx.app
            .get_with_session(&format!("{}/summary?pr={pr}", fx.reviews_path()))
            .await,
    )
    .await
}

/// 重大度は直せない。送れば 400 で断り、何も変えない（TASK-577）。
#[tokio::test]
async fn the_severity_of_a_finding_cannot_be_edited() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding) = submit_round(&fx, 907, "high", "重さは直せない指摘").await;

    for body in [
        serde_json::json!({ "severity": "low" }),
        serde_json::json!({ "severity": null }),
        serde_json::json!({ "title": "題も一緒に直す", "severity": "nit" }),
    ] {
        let res = edit(&fx, &finding, body.clone()).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{body}");
        let text = res.text().await.unwrap_or_default();
        assert!(text.contains("severity は直せません"), "理由を返す: {text}");
    }
    let row = finding_row(&fx, &finding).await;
    assert_eq!(row.severity.as_str(), "high");
    assert_eq!(row.title, "重さは直せない指摘", "一緒に送った欄も変えない");
    assert!(row.edited_at.is_none());
    assert!(edits_of(&fx, &finding).await.is_empty());

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}

/// 内容を直しても、マージ可否の集計の判定は変わらない（TASK-577）。
#[tokio::test]
async fn editing_a_finding_does_not_change_the_merge_verdict() {
    let mut fx = setup().await;
    fx.login(&fx.reviewer.clone()).await;
    let (_, finding) = submit_round(&fx, 906, "high", "直しても塞ぐ指摘").await;

    let before = summary_of(&fx, 906).await;
    assert_eq!(before["blocking"], 1);
    assert_eq!(before["mergeable"], false, "{before}");

    assert_eq!(
        edit(
            &fx,
            &finding,
            serde_json::json!({
                "title": "題を直した",
                "body": "本文を直した",
                "file": null,
                "line": null,
            }),
        )
        .await
        .status(),
        StatusCode::OK
    );
    let after = summary_of(&fx, 906).await;
    assert_eq!(after["blocking"], before["blocking"], "{after}");
    assert_eq!(after["mergeable"], before["mergeable"], "{after}");
    assert_eq!(
        after["counts"], before["counts"],
        "重大度 × 状態の件数も変わらない"
    );

    fx.app.cleanup_user(fx.reviewer.id).await;
    fx.app.cleanup_user(fx.developer.id).await;
}
