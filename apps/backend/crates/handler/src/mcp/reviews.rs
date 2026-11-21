//! レビュー指摘の MCP のツール。形は `tools` と同じで、REST API を内向きに呼び直す。
//!
//! 引数の `///` はツールの説明としてクライアントへ渡るので英語で書く
//! （CLI のヘルプ `crates/cli/src/cli.rs` の `ReviewCommand` の文面に合わせる）。

use axum::http::request::Parts;
use entity::review_findings::{FindingSeverity, FindingState};
use payload::reviews::{
    CarryOverReviewRequest, CreateFindingInput, CreateReviewRequest, UpdateFindingStateRequest,
};
use rmcp::{
    ErrorData,
    handler::server::{common::Extension, wrapper::Parameters},
    model::CallToolResult,
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use validator::{Validate, ValidationError, ValidationErrors, ValidationErrorsKind};

use super::inner::{InnerApi, ToolFailure};
use super::resolve;
use super::tools::{TaskMcp, blank_as_none, finish, parse_uuid, to_body};

/// 指摘の重大度。`entity` の enum は `JsonSchema` を持たないので写しを置く
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[cfg_attr(test, derive(serde::Serialize))]
#[serde(rename_all = "snake_case")]
enum SeverityArg {
    High,
    Medium,
    Low,
    Nit,
}

impl From<SeverityArg> for FindingSeverity {
    fn from(severity: SeverityArg) -> Self {
        match severity {
            SeverityArg::High => Self::High,
            SeverityArg::Medium => Self::Medium,
            SeverityArg::Low => Self::Low,
            SeverityArg::Nit => Self::Nit,
        }
    }
}

impl SeverityArg {
    fn as_str(self) -> &'static str {
        FindingSeverity::from(self).as_str()
    }
}

/// 指摘の状態。`SeverityArg` と同じく `entity` の写し
#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[cfg_attr(test, derive(serde::Serialize))]
#[serde(rename_all = "snake_case")]
enum StateArg {
    Open,
    Fixing,
    Fixed,
    Verified,
    Deferred,
    Rejected,
}

impl From<StateArg> for FindingState {
    fn from(state: StateArg) -> Self {
        match state {
            StateArg::Open => Self::Open,
            StateArg::Fixing => Self::Fixing,
            StateArg::Fixed => Self::Fixed,
            StateArg::Verified => Self::Verified,
            StateArg::Deferred => Self::Deferred,
            StateArg::Rejected => Self::Rejected,
        }
    }
}

impl StateArg {
    fn as_str(self) -> &'static str {
        FindingState::from(self).as_str()
    }
}

#[derive(Deserialize, JsonSchema)]
struct PrArgs {
    /// Project key or UUID
    project: String,
    /// Pull request number
    pr: i32,
    /// Repository to read as owner/name (default: the current integration)
    #[serde(default, deserialize_with = "blank_as_none")]
    repo: Option<String>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct ListReviewFindingsArgs {
    /// Project key or UUID
    project: String,
    /// Pull request number
    pr: i32,
    /// Repository to read as owner/name (default: the current integration)
    #[serde(default, deserialize_with = "blank_as_none")]
    repo: Option<String>,
    /// Filter by state (open, fixing, fixed, verified, deferred, rejected)
    state: Option<Vec<StateArg>>,
    /// Filter by severity (high, medium, low, nit)
    severity: Option<Vec<SeverityArg>>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct FindingArg {
    /// high and medium must be resolved before the merge; low and nit can be deferred
    severity: SeverityArg,
    /// Finding title (1-200 characters)
    title: String,
    /// Finding body (Markdown)
    body: String,
    /// Path of the file the finding is about
    #[serde(default, deserialize_with = "blank_as_none")]
    file: Option<String>,
    /// Line number in the file (1 or more)
    line: Option<i32>,
}

#[derive(Deserialize, JsonSchema)]
struct SubmitReviewArgs {
    /// Project key or UUID
    project: String,
    /// Pull request number
    pr: i32,
    /// Full 40-character SHA of the reviewed commit
    head_sha: String,
    /// Overall comment of the round (Markdown)
    summary: Option<String>,
    /// Findings of the round. Omit or pass an empty list to record a round without findings
    findings: Option<Vec<FindingArg>>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct UpdateReviewFindingArgs {
    /// Project key or UUID
    project: String,
    /// Finding UUID
    finding_id: String,
    /// New state (open, fixing, fixed, verified, deferred, rejected)
    state: StateArg,
    /// Why the state changed (kept in the history)
    note: Option<String>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct CarryOverReviewArgs {
    /// Project key or UUID
    project: String,
    /// Pull request number to carry findings from
    from_pr: i32,
    /// Pull request number to carry findings to (a pull request of the current integration)
    to_pr: i32,
    /// Repository of the source pull request as owner/name (default: the current integration)
    #[serde(default, deserialize_with = "blank_as_none")]
    from_repo: Option<String>,
    /// Tenant name or UUID. Only needed when the token is bound to more than one tenant
    #[serde(default, deserialize_with = "blank_as_none")]
    tenant: Option<String>,
}

/// （テナント ID、プロジェクト ID）を引く。
async fn project_path(
    api: &InnerApi,
    tenant: Option<&str>,
    project: &str,
) -> Result<(String, String), ToolFailure> {
    let tenant_id = resolve::tenant_id(api, tenant).await?;
    let project_id = resolve::project_id(api, &tenant_id, project).await?;
    Ok((tenant_id, project_id))
}

/// 配列を API のカンマ区切りにする。空の配列はクエリに付けない
fn csv<T>(values: Option<Vec<T>>, spell: fn(T) -> &'static str) -> Option<String> {
    let values = values?;
    if values.is_empty() {
        return None;
    }
    Some(values.into_iter().map(spell).collect::<Vec<_>>().join(","))
}

/// 起票を API へ送る前に確かめる。文面は CLI の `review submit`
/// （`crates/cli/src/commands/reviews.rs` の `parse_submit_payload`）に合わせる。
///
/// API に任せると、上限の違反は入力の全体を含むエラーになって上限の値が切れ、
/// 短縮 SHA は理由の無い `regex` になり、空白だけの題は通ってしまう。
/// 上限と範囲は DTO の `Validate` を走らせるだけで、写しは持たない
fn check_submit(request: &CreateReviewRequest) -> Result<(), ToolFailure> {
    if !common::validation::COMMIT_SHA_REGEX.is_match(&request.head_sha) {
        return Err(ToolFailure::new(format!(
            "`head_sha` must be the full 40-character commit SHA (got: {})",
            request.head_sha
        )));
    }
    for (index, finding) in request.findings.iter().enumerate() {
        for (key, value) in [("title", &finding.title), ("body", &finding.body)] {
            if value.trim().is_empty() {
                return Err(ToolFailure::new(format!(
                    "findings[{index}].{key} is required"
                )));
            }
        }
    }
    let Err(errors) = request.validate() else {
        return Ok(());
    };
    let mut lines = Vec::new();
    violations(&errors, "", &mut lines);
    // `ValidationErrors` の順は不定なので、決まった順にする
    lines.sort();
    Err(ToolFailure::new(format!(
        "review does not satisfy the API's limits:\n{}",
        lines.join("\n")
    )))
}

/// `findings[0].line` の形になるよう、入れ子とリストを辿ってパスを組み立てる。
fn violations(errors: &ValidationErrors, prefix: &str, out: &mut Vec<String>) {
    for (field, kind) in errors.errors() {
        // DTO の `pr_number` は、ツールの引数では `pr`
        let field: &str = if prefix.is_empty() && field == "pr_number" {
            "pr"
        } else {
            field
        };
        match kind {
            ValidationErrorsKind::Field(found) => {
                for violation in found {
                    out.push(format!("  {prefix}{field}: {}", describe(violation)));
                }
            }
            ValidationErrorsKind::Struct(nested) => {
                violations(nested, &format!("{prefix}{field}."), out);
            }
            ValidationErrorsKind::List(entries) => {
                for (index, nested) in entries {
                    violations(nested, &format!("{prefix}{field}[{index}]."), out);
                }
            }
        }
    }
}

fn describe(violation: &ValidationError) -> String {
    if let Some(message) = &violation.message {
        return message.to_string();
    }
    let mut described = violation.code.to_string();
    for key in ["min", "max", "equal"] {
        if let Some(value) = violation.params.get(key) {
            described.push_str(&format!(" {key}: {value}"));
        }
    }
    described
}

impl TaskMcp {
    async fn run_list_review_findings(
        &self,
        parts: &Parts,
        args: ListReviewFindingsArgs,
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let (tenant_id, project_id) =
            project_path(&api, args.tenant.as_deref(), &args.project).await?;
        // 省略した項目はクエリに付けない（API の既定に任せる）
        let mut query: Vec<(&str, String)> = vec![("pr", args.pr.to_string())];
        if let Some(repo) = args.repo {
            query.push(("repo", repo));
        }
        if let Some(state) = csv(args.state, StateArg::as_str) {
            query.push(("state", state));
        }
        if let Some(severity) = csv(args.severity, SeverityArg::as_str) {
            query.push(("severity", severity));
        }
        api.get(
            &[
                "v1",
                "tenants",
                &tenant_id,
                "projects",
                &project_id,
                "review-findings",
            ],
            &query,
        )
        .await
    }

    /// PR のレビューの読み取り（`tail` が空ならラウンドの一覧）。
    async fn run_review_get(
        &self,
        parts: &Parts,
        args: PrArgs,
        tail: &[&str],
    ) -> Result<Value, ToolFailure> {
        let api = self.inner(parts)?;
        let (tenant_id, project_id) =
            project_path(&api, args.tenant.as_deref(), &args.project).await?;
        let mut query: Vec<(&str, String)> = vec![("pr", args.pr.to_string())];
        if let Some(repo) = args.repo {
            query.push(("repo", repo));
        }
        let mut segments = vec![
            "v1",
            "tenants",
            &tenant_id,
            "projects",
            &project_id,
            "reviews",
        ];
        segments.extend_from_slice(tail);
        api.get(&segments, &query).await
    }

    async fn run_submit_review(
        &self,
        parts: &Parts,
        args: SubmitReviewArgs,
    ) -> Result<Value, ToolFailure> {
        let request = CreateReviewRequest {
            pr_number: args.pr,
            head_sha: args.head_sha,
            summary: args.summary.unwrap_or_default(),
            findings: args
                .findings
                .unwrap_or_default()
                .into_iter()
                .map(|finding| CreateFindingInput {
                    severity: finding.severity.into(),
                    title: finding.title,
                    body: finding.body,
                    file: finding.file,
                    line: finding.line,
                })
                .collect(),
        };
        // 引数の誤りは、何も呼ぶ前に返す
        check_submit(&request)?;
        let body = to_body(&request)?;

        let api = self.inner(parts)?;
        let (tenant_id, project_id) =
            project_path(&api, args.tenant.as_deref(), &args.project).await?;
        api.post(
            &[
                "v1",
                "tenants",
                &tenant_id,
                "projects",
                &project_id,
                "reviews",
            ],
            &body,
        )
        .await
    }

    async fn run_update_review_finding(
        &self,
        parts: &Parts,
        args: UpdateReviewFindingArgs,
    ) -> Result<Value, ToolFailure> {
        let finding_id = parse_uuid("finding_id", &args.finding_id)?.to_string();
        let body = to_body(&UpdateFindingStateRequest {
            state: args.state.into(),
            note: args.note,
        })?;

        let api = self.inner(parts)?;
        let (tenant_id, project_id) =
            project_path(&api, args.tenant.as_deref(), &args.project).await?;
        api.patch(
            &[
                "v1",
                "tenants",
                &tenant_id,
                "projects",
                &project_id,
                "review-findings",
                &finding_id,
            ],
            &body,
        )
        .await
    }

    async fn run_carry_over_review(
        &self,
        parts: &Parts,
        args: CarryOverReviewArgs,
    ) -> Result<Value, ToolFailure> {
        let body = to_body(&CarryOverReviewRequest {
            from_pr_number: args.from_pr,
            from_repo: args.from_repo,
            to_pr_number: args.to_pr,
        })?;

        let api = self.inner(parts)?;
        let (tenant_id, project_id) =
            project_path(&api, args.tenant.as_deref(), &args.project).await?;
        api.post(
            &[
                "v1",
                "tenants",
                &tenant_id,
                "projects",
                &project_id,
                "reviews",
                "carry-over",
            ],
            &body,
        )
        .await
    }
}

#[tool_router(router = review_router, vis = "pub(super)")]
impl TaskMcp {
    #[tool(
        description = "List the review findings of a pull request. Needs the read:review scope.",
        annotations(read_only_hint = true)
    )]
    async fn list_review_findings(
        &self,
        Parameters(args): Parameters<ListReviewFindingsArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_list_review_findings(&parts, args).await)
    }

    #[tool(
        description = "List the review rounds of a pull request, newest first. Needs the read:review scope.",
        annotations(read_only_hint = true)
    )]
    async fn list_review_rounds(
        &self,
        Parameters(args): Parameters<PrArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_review_get(&parts, args, &[]).await)
    }

    #[tool(
        description = "Show the merge verdict of a pull request: counts by severity and state, blocking findings, the reviewed commit (latest_head_sha) and gate. Compare latest_head_sha with the head of the pull request yourself; a review of an older commit does not cover it. Needs the read:review scope.",
        annotations(read_only_hint = true)
    )]
    async fn get_review_summary(
        &self,
        Parameters(args): Parameters<PrArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_review_get(&parts, args, &["summary"]).await)
    }

    #[tool(
        description = "Submit one review round for a pull request. The findings are created together and cannot be added later. Needs the write:review scope."
    )]
    async fn submit_review(
        &self,
        Parameters(args): Parameters<SubmitReviewArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_submit_review(&parts, args).await)
    }

    #[tool(
        description = "Move a review finding to a new state. available_actions of a finding lists the states you can move it to. Needs the write:review scope."
    )]
    async fn update_review_finding(
        &self,
        Parameters(args): Parameters<UpdateReviewFindingArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_update_review_finding(&parts, args).await)
    }

    #[tool(
        description = "Carry the unresolved findings of another pull request over to this one (e.g. after re-opening a merged pull request). Needs the write:review scope."
    )]
    async fn carry_over_review(
        &self,
        Parameters(args): Parameters<CarryOverReviewArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        finish(self.run_carry_over_review(&parts, args).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::Iterable;
    use serde_json::json;

    fn spelled<T: serde::Serialize>(values: &[T]) -> Vec<String> {
        values
            .iter()
            .map(|value| {
                serde_json::to_value(value)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    /// 写しの綴りと値の数が `entity` とずれたら落ちる
    #[test]
    fn severity_and_state_match_the_entity() {
        let severities = [
            SeverityArg::High,
            SeverityArg::Medium,
            SeverityArg::Low,
            SeverityArg::Nit,
        ];
        let entity: Vec<&str> = FindingSeverity::iter()
            .map(FindingSeverity::as_str)
            .collect();
        assert_eq!(spelled(&severities), entity);
        assert_eq!(severities.map(SeverityArg::as_str).to_vec(), entity);

        let states = [
            StateArg::Open,
            StateArg::Fixing,
            StateArg::Fixed,
            StateArg::Verified,
            StateArg::Deferred,
            StateArg::Rejected,
        ];
        let entity: Vec<&str> = FindingState::iter().map(FindingState::as_str).collect();
        assert_eq!(spelled(&states), entity);
        assert_eq!(states.map(StateArg::as_str).to_vec(), entity);
    }

    #[test]
    fn blank_repo_and_file_are_read_as_omitted() {
        let args: PrArgs =
            serde_json::from_value(json!({ "project": "K", "pr": 1, "repo": "", "tenant": "" }))
                .unwrap();
        assert_eq!(args.repo, None);
        assert_eq!(args.tenant, None);

        let args: ListReviewFindingsArgs =
            serde_json::from_value(json!({ "project": "K", "pr": 1, "repo": " " })).unwrap();
        assert_eq!(args.repo, None);

        let args: CarryOverReviewArgs = serde_json::from_value(
            json!({ "project": "K", "from_pr": 1, "to_pr": 2, "from_repo": "" }),
        )
        .unwrap();
        assert_eq!(args.from_repo, None);

        let args: PrArgs =
            serde_json::from_value(json!({ "project": "K", "pr": 1, "repo": "acme/app" })).unwrap();
        assert_eq!(args.repo.as_deref(), Some("acme/app"));

        // 本文は空でもそのまま渡す
        let args: SubmitReviewArgs = serde_json::from_value(json!({
            "project": "K",
            "pr": 1,
            "head_sha": "a",
            "summary": "",
            "findings": [{ "severity": "low", "title": "t", "body": "b", "file": "" }],
        }))
        .unwrap();
        assert_eq!(args.summary.as_deref(), Some(""));
        assert_eq!(args.findings.unwrap()[0].file, None);

        let args: UpdateReviewFindingArgs = serde_json::from_value(
            json!({ "project": "K", "finding_id": "x", "state": "fixed", "note": "" }),
        )
        .unwrap();
        assert_eq!(args.note.as_deref(), Some(""));
    }

    #[test]
    fn csv_joins_and_drops_an_empty_list() {
        assert_eq!(
            csv(
                Some(vec![StateArg::Open, StateArg::Fixed]),
                StateArg::as_str
            )
            .as_deref(),
            Some("open,fixed")
        );
        assert_eq!(
            csv(Some(vec![SeverityArg::Nit]), SeverityArg::as_str).as_deref(),
            Some("nit")
        );
        assert_eq!(csv(Some(Vec::new()), StateArg::as_str), None);
        assert_eq!(csv(None, StateArg::as_str), None);
    }

    const HEAD: &str = "60cdd77a1b2c3d4e5f60718293a4b5c6d7e8f901";

    fn finding(title: &str, body: &str, line: Option<i32>) -> CreateFindingInput {
        CreateFindingInput {
            severity: FindingSeverity::Low,
            title: title.into(),
            body: body.into(),
            file: None,
            line,
        }
    }

    fn request(
        pr_number: i32,
        head_sha: &str,
        findings: Vec<CreateFindingInput>,
    ) -> CreateReviewRequest {
        CreateReviewRequest {
            pr_number,
            head_sha: head_sha.into(),
            summary: String::new(),
            findings,
        }
    }

    #[test]
    fn violations_name_the_path() {
        let long_title = "x".repeat(201);
        let bad = request(
            0,
            HEAD,
            vec![finding(&long_title, "b", None), finding("t", "b", Some(0))],
        );
        assert_eq!(
            check_submit(&bad).unwrap_err().message,
            "review does not satisfy the API's limits:\n  findings[0].title: length min: 1 max: 200\n  findings[1].line: range min: 1\n  pr: range min: 1"
        );
    }

    #[test]
    fn submit_is_checked_in_the_order_of_the_cli() {
        // 形の違う `head_sha` は、ほかの違反より先に返す
        let short = request(0, "60cdd77", vec![finding(" ", "b", None)]);
        assert_eq!(
            check_submit(&short).unwrap_err().message,
            "`head_sha` must be the full 40-character commit SHA (got: 60cdd77)"
        );
        let upper = request(1, &HEAD.to_uppercase(), Vec::new());
        assert!(check_submit(&upper).is_err());

        let blank_title = request(
            1,
            HEAD,
            vec![finding("t", "b", None), finding(" \n", "b", None)],
        );
        assert_eq!(
            check_submit(&blank_title).unwrap_err().message,
            "findings[1].title is required"
        );
        let blank_body = request(1, HEAD, vec![finding("t", "", Some(0))]);
        assert_eq!(
            check_submit(&blank_body).unwrap_err().message,
            "findings[0].body is required"
        );

        // 対照: 指摘なしも、境界の値も通る
        assert_eq!(check_submit(&request(1, HEAD, Vec::new())), Ok(()));
        let at_the_limit = request(1, HEAD, vec![finding(&"x".repeat(200), "b", Some(1))]);
        assert_eq!(check_submit(&at_the_limit), Ok(()));
    }
}
