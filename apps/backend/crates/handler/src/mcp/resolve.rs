//! キー・名前・参照から、API が要る ID を引く。
//!
//! 規則は CLI（`crates/cli/src/api.rs` の `resolve_tenant` / `pick_tenant` と
//! `crates/cli/src/resolve.rs` の `pick_named_id` / `resolve_user_id` と、
//! `crates/cli/src/commands/tasks.rs` の `FieldResolver`）と同じ。`handler` から `cli` は
//! 参照できないので、要る分を写している。片方を変えたら、もう片方も合わせる。

use serde_json::Value;
use uuid::Uuid;

use super::inner::{InnerApi, ToolFailure};

fn is_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok()
}

fn str_field<'a>(value: &'a Value, name: &str) -> &'a str {
    value.get(name).and_then(Value::as_str).unwrap_or_default()
}

fn list(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

/// 行き先のテナントを決める（`crates/cli/src/api.rs` の `resolve_tenant`）。
///
/// - UUID: そのまま使う（何も呼ばない）。
/// - UUID でない: 名前（`display_id`）として `GET /v1/tenants` から引く。scope `read:tenant` が要る。
/// - 省略: PAT の束縛先を `GET /v1/personal_tokens/me` から引く。1 件ならそれを使う。
///   複数なら、黙って 1 つを選ぶと別のテナントへ書き込みかねないので、明示を求めて止まる。
pub(super) async fn tenant_id(api: &InnerApi, tenant: Option<&str>) -> Result<String, ToolFailure> {
    let Some(name) = tenant else {
        let identity = api.get(&["v1", "personal_tokens", "me"], &[]).await?;
        return pick_bound_tenant(&identity);
    };
    if is_uuid(name) {
        return Ok(name.to_string());
    }
    let tenants = api.get(&["v1", "tenants"], &[]).await.map_err(|failure| {
        failure.explain_forbidden(|| {
            format!(
                "could not look up tenant \"{name}\" by name: the token may lack the read:tenant scope; if so, pass the tenant UUID instead"
            )
        })
    })?;
    pick_tenant(list(&tenants), name)
}

/// `/personal_tokens/me` の束縛。`tenant_ids` が空なら旧形式の `tenant_id` を 1 件として読む。
fn pick_bound_tenant(identity: &Value) -> Result<String, ToolFailure> {
    let mut bound: Vec<&str> = list(&identity["tenant_ids"])
        .iter()
        .filter_map(Value::as_str)
        .collect();
    if bound.is_empty()
        && let Some(only) = identity.get("tenant_id").and_then(Value::as_str)
    {
        bound.push(only);
    }
    match bound.as_slice() {
        [only] => Ok((*only).to_string()),
        [] => Err(ToolFailure::new("The token is not bound to any tenant")),
        many => Err(ToolFailure::new(format!(
            "This token can be used in {} tenants: {}. Pass `tenant`.",
            many.len(),
            many.join(", ")
        ))),
    }
}

/// 名前（`display_id`）に合うテナント。完全一致を優先し、無ければ大文字小文字を無視して
/// 1 件だけなら採る（`crates/cli/src/api.rs` の `pick_tenant`）。
fn pick_tenant(tenants: &[Value], name: &str) -> Result<String, ToolFailure> {
    let describe = |list: &[&Value]| {
        list.iter()
            .map(|tenant| {
                format!(
                    "{} ({})",
                    str_field(tenant, "display_id"),
                    str_field(tenant, "id")
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(exact) = tenants
        .iter()
        .find(|tenant| str_field(tenant, "display_id") == name)
    {
        return Ok(str_field(exact, "id").to_string());
    }
    let folded: Vec<&Value> = tenants
        .iter()
        .filter(|tenant| str_field(tenant, "display_id").eq_ignore_ascii_case(name))
        .collect();
    match folded.as_slice() {
        [only] => Ok(str_field(only, "id").to_string()),
        [] => {
            let usable = if tenants.is_empty() {
                "none".to_string()
            } else {
                describe(&tenants.iter().collect::<Vec<_>>())
            };
            Err(ToolFailure::new(format!(
                "Tenant not found: {name} (this token can be used in: {usable})"
            )))
        }
        _ => Err(ToolFailure::new(format!(
            "Ambiguous tenant: {name} (candidates: {}); specify a UUID",
            describe(&folded)
        ))),
    }
}

/// プロジェクトをキーか UUID で指す（`crates/cli/src/resolve.rs` の `resolve_project`）。
///
/// UUID はそのまま返す（存在は次の呼び出しが確かめる）。キーは `GET …/projects` から
/// 大文字小文字を無視して引くので、scope `read:project` が要る。
pub(super) async fn project_id(
    api: &InnerApi,
    tenant_id: &str,
    project: &str,
) -> Result<String, ToolFailure> {
    if is_uuid(project) {
        return Ok(project.to_string());
    }
    Ok(find_project_by_key(api, tenant_id, project).await?.id)
}

/// キーから引いたプロジェクト。`key` は保存されている綴り（入力の大文字小文字ではない）
struct FoundProject {
    id: String,
    key: String,
}

/// 403 は scope の不足とは限らない（束縛の外のテナントでも 403）。原因を断定せずに添える
async fn find_project_by_key(
    api: &InnerApi,
    tenant_id: &str,
    key: &str,
) -> Result<FoundProject, ToolFailure> {
    let projects = api
        .get(&["v1", "tenants", tenant_id, "projects"], &[])
        .await
        .map_err(|failure| {
            failure.explain_forbidden(|| {
                format!(
                    "could not look up project key \"{key}\": the token may lack the read:project scope, or it is not allowed in this tenant; if the scope is missing, pass the project UUID instead"
                )
            })
        })?;
    list(&projects)
        .iter()
        .find(|candidate| str_field(candidate, "key").eq_ignore_ascii_case(key))
        .map(|found| FoundProject {
            id: str_field(found, "id").to_string(),
            key: str_field(found, "key").to_string(),
        })
        .ok_or_else(|| ToolFailure::new(format!("Project not found: {key}")))
}

/// `KEY-N` のうち番号の部分（最後の `-` の後ろ）。
fn seq_number(task_id: &str) -> &str {
    task_id.rsplit('-').next().unwrap_or(task_id)
}

/// タスクの指し方。`KEY-42` は API がそのまま解決できるので文字列のまま渡す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TaskRef {
    Uuid(Uuid),
    Seq {
        project_key: String,
        task_id: String,
    },
}

/// `KEY-N` は最後の `-` で分け、後ろが数字だけのもの（`crates/cli/src/resolve.rs` の `parse_task_ref`）。
pub(super) fn parse_task_ref(value: &str) -> Result<TaskRef, ToolFailure> {
    if let Ok(uuid) = Uuid::parse_str(value) {
        return Ok(TaskRef::Uuid(uuid));
    }
    let invalid = || ToolFailure::new(format!("Invalid task reference: {value}"));
    let dash = value
        .rfind('-')
        .filter(|index| *index > 0)
        .ok_or_else(invalid)?;
    let (project_key, seq) = value.split_at(dash);
    let seq = &seq[1..];
    if seq.is_empty() || !seq.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    Ok(TaskRef::Seq {
        project_key: project_key.to_string(),
        task_id: value.to_string(),
    })
}

/// タスクの置き場所（プロジェクトの ID と、パスに入れるタスクの ID）を決める。
///
/// `KEY-N` はキーからプロジェクトを引く。UUID はプロジェクトが分からないので `project` が要る。
pub(super) async fn task_location(
    api: &InnerApi,
    tenant_id: &str,
    task: &str,
    project: Option<&str>,
) -> Result<(String, String), ToolFailure> {
    match parse_task_ref(task)? {
        TaskRef::Seq {
            project_key,
            task_id,
        } => {
            // キーは大文字小文字を無視して引く。API はタスクの `KEY-N` を保存した綴りで
            // 照合するので、見つけたプロジェクトのキーで組み直して渡す（`task-42` も通る）
            let found = find_project_by_key(api, tenant_id, &project_key).await?;
            let task_id = format!("{}-{}", found.key, seq_number(&task_id));
            Ok((found.id, task_id))
        }
        TaskRef::Uuid(uuid) => {
            let project = project
                .ok_or_else(|| ToolFailure::new("project is required when task is a UUID"))?;
            Ok((project_id(api, tenant_id, project).await?, uuid.to_string()))
        }
    }
}

/// プロジェクトの状態の一覧。名前か省略で状態を指すときだけ引く。scope `read:task` が要る。
pub(super) async fn list_statuses(
    api: &InnerApi,
    tenant_id: &str,
    project_id: &str,
) -> Result<Vec<Value>, ToolFailure> {
    list_collection(api, tenant_id, project_id, "statuses", &[], || {
        "could not list the statuses to resolve the status: the token may lack the read:task scope, or it is not allowed in this project; if the scope is missing, pass the status UUID instead"
            .to_string()
    })
    .await
}

/// `GET /v1/tenants/{tenant}/projects/{project}/{collection}` の一覧。403 には `hint` を添える
async fn list_collection(
    api: &InnerApi,
    tenant_id: &str,
    project_id: &str,
    collection: &str,
    query: &[(&str, String)],
    hint: impl FnOnce() -> String,
) -> Result<Vec<Value>, ToolFailure> {
    let items = api
        .get(
            &[
                "v1", "tenants", tenant_id, "projects", project_id, collection,
            ],
            query,
        )
        .await
        .map_err(|failure| failure.explain_forbidden(hint))?;
    Ok(list(&items).to_vec())
}

fn position(status: &Value) -> i64 {
    status.get("position").and_then(Value::as_i64).unwrap_or(0)
}

/// 名前で引けなかったときのエラー（`crates/cli/src/resolve.rs` の `not_found_with_candidates`）。
///
/// 綴りを外したときに何が使えるか分からないと詰まるので、そのプロジェクトの候補を添える
fn not_found_with_candidates(kind: &str, name: &str, candidates: &[&str]) -> ToolFailure {
    let listed = if candidates.is_empty() {
        "none".to_string()
    } else {
        candidates.join(", ")
    };
    ToolFailure::new(format!(
        "{kind} not found: {name} (this project has {listed})"
    ))
}

/// 候補（`(id, 名前)` の組）から名前で 1 件を選ぶ（`crates/cli/src/resolve.rs` の `pick_named_id`）。
///
/// 完全一致を優先し、無ければ大文字小文字を無視する。同名の候補が残れば UUID を求める。
/// UUID の形の判定は呼ぶ側がする
fn pick_named<'a>(
    kind: &str,
    name: &str,
    candidates: &[(&'a str, &'a str)],
) -> Result<&'a str, ToolFailure> {
    let exact: Vec<&(&str, &str)> = candidates
        .iter()
        .filter(|(_, value)| *value == name)
        .collect();
    let matches = if exact.is_empty() {
        candidates
            .iter()
            .filter(|(_, value)| value.eq_ignore_ascii_case(name))
            .collect()
    } else {
        exact
    };
    match matches.as_slice() {
        [(id, _)] => Ok(*id),
        [] => Err(not_found_with_candidates(
            kind,
            name,
            &candidates
                .iter()
                .map(|(_, value)| *value)
                .collect::<Vec<_>>(),
        )),
        _ => Err(ToolFailure::new(format!(
            "Ambiguous {kind}: {name} (candidates: {}); specify a UUID",
            matches
                .iter()
                .map(|(id, value)| format!("{value} ({id})"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// `(id, 名前)` の組。名前の欄は種類で違う（担当者は `username`）
fn pairs<'a>(items: &'a [Value], name_field: &str) -> Vec<(&'a str, &'a str)> {
    items
        .iter()
        .map(|item| (str_field(item, "id"), str_field(item, name_field)))
        .collect()
}

/// 一覧から読んだ ID。UUID でないのは API の応答が壊れたときだけ
fn listed_uuid(kind: &str, id: &str) -> Result<Uuid, ToolFailure> {
    Uuid::parse_str(id).map_err(|_| ToolFailure::new(format!("{kind} id is not a UUID")))
}

/// 状態を名前か UUID で指す（`crates/cli/src/resolve.rs` の `pick_status_by_name`）。
///
/// 候補は位置の順に並べてから選ぶ（見つからないときの候補もこの順）。
pub(super) fn pick_status(statuses: &[Value], name: &str) -> Result<String, ToolFailure> {
    if is_uuid(name) {
        return Ok(name.to_string());
    }
    let mut statuses: Vec<&Value> = statuses.iter().collect();
    statuses.sort_by_key(|status| position(status));
    let candidates: Vec<(&str, &str)> = statuses
        .iter()
        .map(|status| (str_field(status, "id"), str_field(status, "name")))
        .collect();
    pick_named("Status", name, &candidates).map(str::to_string)
}

/// 名前で引ける、プロジェクトの下の一覧の種類
#[derive(Clone, Copy)]
enum Named {
    Label,
    TaskType,
    Milestone,
    Sprint,
}

impl Named {
    /// エラーの見出し（CLI と同じ綴り）
    fn kind(self) -> &'static str {
        match self {
            Self::Label => "Label",
            Self::TaskType => "Type",
            Self::Milestone => "Milestone",
            Self::Sprint => "Sprint",
        }
    }

    fn collection(self) -> &'static str {
        match self {
            Self::Label => "labels",
            Self::TaskType => "task-types",
            Self::Milestone => "milestones",
            Self::Sprint => "sprints",
        }
    }

    /// 一覧を読む scope。403 の補足に使う
    fn scope(self) -> &'static str {
        match self {
            Self::Label | Self::TaskType => "read:task",
            Self::Milestone => "read:milestone",
            Self::Sprint => "read:sprint",
        }
    }

    /// 補足の文面に入れる単数と複数の語
    fn words(self) -> (&'static str, &'static str) {
        match self {
            Self::Label => ("label", "labels"),
            Self::TaskType => ("task type", "task types"),
            Self::Milestone => ("milestone", "milestones"),
            Self::Sprint => ("sprint", "sprints"),
        }
    }
}

/// プロジェクトのラベル・種別・マイルストーン・スプリント・担当者を名前か UUID で引く
/// （`crates/cli/src/commands/tasks.rs` の `FieldResolver`）。
///
/// 一覧は種類ごとに、名前で引くときに初めて 1 回だけ取って使い回す。UUID は確かめずにそのまま返す
pub(super) struct ProjectNames<'a> {
    api: &'a InnerApi,
    tenant_id: &'a str,
    project_id: &'a str,
    labels: Option<Vec<Value>>,
    task_types: Option<Vec<Value>>,
    milestones: Option<Vec<Value>>,
    sprints: Option<Vec<Value>>,
}

impl<'a> ProjectNames<'a> {
    pub(super) fn new(api: &'a InnerApi, tenant_id: &'a str, project_id: &'a str) -> Self {
        Self {
            api,
            tenant_id,
            project_id,
            labels: None,
            task_types: None,
            milestones: None,
            sprints: None,
        }
    }

    pub(super) async fn label(&mut self, value: &str) -> Result<Uuid, ToolFailure> {
        self.named(Named::Label, value).await
    }

    /// 順に引く。重複は除かない（CLI と同じ）
    pub(super) async fn labels(&mut self, values: &[String]) -> Result<Vec<Uuid>, ToolFailure> {
        let mut ids = Vec::with_capacity(values.len());
        for value in values {
            ids.push(self.label(value).await?);
        }
        Ok(ids)
    }

    pub(super) async fn task_type(&mut self, value: &str) -> Result<Uuid, ToolFailure> {
        self.named(Named::TaskType, value).await
    }

    pub(super) async fn milestone(&mut self, value: &str) -> Result<Uuid, ToolFailure> {
        self.named(Named::Milestone, value).await
    }

    pub(super) async fn sprint(&mut self, value: &str) -> Result<Uuid, ToolFailure> {
        self.named(Named::Sprint, value).await
    }

    async fn named(&mut self, named: Named, value: &str) -> Result<Uuid, ToolFailure> {
        if let Ok(uuid) = Uuid::parse_str(value) {
            return Ok(uuid);
        }
        let (api, tenant_id, project_id) = (self.api, self.tenant_id, self.project_id);
        let slot = match named {
            Named::Label => &mut self.labels,
            Named::TaskType => &mut self.task_types,
            Named::Milestone => &mut self.milestones,
            Named::Sprint => &mut self.sprints,
        };
        if slot.is_none() {
            let (singular, plural) = named.words();
            let items = list_collection(api, tenant_id, project_id, named.collection(), &[], || {
                format!(
                    "could not list the {plural} to resolve the {singular}: the token may lack the {} scope, or it is not allowed in this project; if the scope is missing, pass the {singular} UUID instead",
                    named.scope()
                )
            })
            .await?;
            *slot = Some(items);
        }
        let items = slot.as_deref().unwrap_or_default();
        let id = pick_named(named.kind(), value, &pairs(items, "name"))?;
        listed_uuid(named.kind(), id)
    }

    /// 担当者はユーザー名で指す（`crates/cli/src/resolve.rs` の `resolve_user_id`）。
    ///
    /// 名前を 1 つ引くだけの `?username=` は `read:task` でも通る。一致が無ければ候補を
    /// 添えるために全件を引き直す。
    pub(super) async fn user(&mut self, value: &str) -> Result<Uuid, ToolFailure> {
        const KIND: &str = "Assignable user";
        if let Ok(uuid) = Uuid::parse_str(value) {
            return Ok(uuid);
        }
        let matched = list_collection(
            self.api,
            self.tenant_id,
            self.project_id,
            "assignable-users",
            &[("username", value.to_string())],
            || {
                format!(
                    "could not look up the assignee \"{value}\": the token may lack the read:task or write:task scope, or it is not allowed in this project; if the scope is missing, pass the user UUID instead"
                )
            },
        )
        .await?;
        if matched
            .iter()
            .any(|user| str_field(user, "username").eq_ignore_ascii_case(value))
        {
            let id = pick_named(KIND, value, &pairs(&matched, "username"))?;
            return listed_uuid(KIND, id);
        }
        // 候補の列挙は `write:task` が要り、読むだけのトークンでは読めない。読めなくても
        // 「見つからない」ことは確かなので、CLI と同じく候補なし（none）で返す
        let candidates = list_collection(
            self.api,
            self.tenant_id,
            self.project_id,
            "assignable-users",
            &[],
            String::new,
        )
        .await
        .unwrap_or_default();
        Err(not_found_with_candidates(
            KIND,
            value,
            &candidates
                .iter()
                .map(|user| str_field(user, "username"))
                .collect::<Vec<_>>(),
        ))
    }
}

/// `status` を省いたときに使う状態（`crates/cli/src/resolve.rs` の `pick_default_status`）。
///
/// 作成 API は `status_id` が必須。既定の状態、無ければ並び順の先頭を使う。
pub(super) fn pick_default_status(statuses: &[Value]) -> Result<String, ToolFailure> {
    statuses
        .iter()
        .find(|status| status.get("is_default").and_then(Value::as_bool) == Some(true))
        .or_else(|| statuses.iter().min_by_key(|status| position(status)))
        .map(|status| str_field(status, "id").to_string())
        .ok_or_else(|| ToolFailure::new("This project has no statuses to create a task in"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn status(name: &str, is_default: bool, position: i64) -> Value {
        json!({
            // 並びの検証をするので、id は位置から決まる固定値にする
            "id": Uuid::from_u128(position as u128 + 1).to_string(),
            "name": name,
            "is_default": is_default,
            "position": position,
        })
    }

    fn id_of(status: &Value) -> String {
        status["id"].as_str().unwrap().to_string()
    }

    #[test]
    fn reads_uuid_and_key_number_task_references() {
        let uuid = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(
            parse_task_ref("00000000-0000-4000-8000-000000000001").unwrap(),
            TaskRef::Uuid(uuid)
        );
        assert_eq!(
            parse_task_ref("KEY-1").unwrap(),
            TaskRef::Seq {
                project_key: "KEY".into(),
                task_id: "KEY-1".into()
            }
        );
    }

    #[test]
    fn seq_number_is_the_part_after_the_last_dash() {
        assert_eq!(seq_number("KEY-1"), "1");
        assert_eq!(seq_number("my-key-12"), "12");
    }

    #[test]
    fn keeps_a_multi_dash_project_key_whole() {
        assert_eq!(
            parse_task_ref("MY-KEY-12").unwrap(),
            TaskRef::Seq {
                project_key: "MY-KEY".into(),
                task_id: "MY-KEY-12".into()
            }
        );
    }

    #[test]
    fn rejects_references_without_a_numeric_sequence() {
        for value in ["KEY-x", "KEY-", "-1", "KEY", "KEY-4a"] {
            let failure = parse_task_ref(value).unwrap_err();
            assert_eq!(
                failure.message,
                format!("Invalid task reference: {value}"),
                "{value}"
            );
            assert_eq!(failure.status, None, "{value}");
        }
    }

    #[test]
    fn status_prefers_exact_matches_and_rejects_ambiguity() {
        let upper = status("Bug", false, 0);
        let lower = status("bug", false, 1);
        for statuses in [
            vec![upper.clone(), lower.clone()],
            vec![lower.clone(), upper.clone()],
        ] {
            assert_eq!(pick_status(&statuses, "Bug").unwrap(), id_of(&upper));
            assert_eq!(pick_status(&statuses, "bug").unwrap(), id_of(&lower));
            let failure = pick_status(&statuses, "BUG").unwrap_err();
            for expected in [id_of(&upper), id_of(&lower), "UUID".into()] {
                assert!(failure.message.contains(&expected), "{}", failure.message);
            }
            assert!(
                failure.message.starts_with("Ambiguous Status: BUG"),
                "{}",
                failure.message
            );
        }
        // 同じ綴りが複数あれば、完全一致でも任意の一件を選ばない
        let twin = status("Bug", false, 1);
        assert!(pick_status(&[upper.clone(), twin], "Bug").is_err());
        assert_eq!(
            pick_status(std::slice::from_ref(&upper), "BUG").unwrap(),
            id_of(&upper)
        );
    }

    /// CLI の `named_resources_prefer_exact_matches_and_reject_ambiguity` と同じ表
    #[test]
    fn named_resources_prefer_exact_matches_and_reject_ambiguity() {
        let first = Uuid::from_u128(1).to_string();
        let second = Uuid::from_u128(2).to_string();
        let kind = "Label";
        for candidates in [
            [(first.as_str(), "Bug"), (second.as_str(), "bug")],
            [(second.as_str(), "bug"), (first.as_str(), "Bug")],
        ] {
            assert_eq!(pick_named(kind, "Bug", &candidates).unwrap(), first);
            assert_eq!(pick_named(kind, "bug", &candidates).unwrap(), second);
            let failure = pick_named(kind, "BUG", &candidates).unwrap_err();
            assert_eq!(failure.status, None);
            for expected in [first.as_str(), second.as_str(), "UUID"] {
                assert!(failure.message.contains(expected), "{}", failure.message);
            }
        }
        // 同じ綴りが複数あれば、完全一致でも任意の一件を選ばない
        assert!(pick_named(kind, "Bug", &[(&first, "Bug"), (&second, "Bug")]).is_err());
        assert_eq!(pick_named(kind, "BUG", &[(&first, "Bug")]).unwrap(), first);
    }

    #[test]
    fn named_failures_use_the_cli_wording() {
        let first = Uuid::from_u128(1).to_string();
        let second = Uuid::from_u128(2).to_string();
        assert_eq!(
            pick_named(
                "Label",
                "buf",
                &[(first.as_str(), "bug"), (second.as_str(), "enhancement")]
            )
            .unwrap_err()
            .message,
            "Label not found: buf (this project has bug, enhancement)"
        );
        assert_eq!(
            pick_named("Sprint", "week-1", &[]).unwrap_err().message,
            "Sprint not found: week-1 (this project has none)"
        );
        assert_eq!(
            pick_named(
                "Label",
                "BUG",
                &[(first.as_str(), "Bug"), (second.as_str(), "bug")]
            )
            .unwrap_err()
            .message,
            format!(
                "Ambiguous Label: BUG (candidates: Bug ({first}), bug ({second})); specify a UUID"
            )
        );
    }

    /// `?username=` には一致なしを、候補の全件には 403 を返すルーター。
    /// 読むだけのトークンで綴りを外したときに当たる
    fn assignable_users_without_listing() -> axum::Router {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
            response::IntoResponse,
        };
        axum::Router::new().fallback(|request: Request<Body>| async move {
            if request
                .uri()
                .query()
                .is_some_and(|query| query.contains("username="))
            {
                axum::Json(json!([])).into_response()
            } else {
                (
                    StatusCode::FORBIDDEN,
                    axum::Json(json!({ "message": "forbidden" })),
                )
                    .into_response()
            }
        })
    }

    /// 候補の列挙が読めなくても、見つからないことは候補なしで返す（CLI と同じ）
    #[tokio::test]
    async fn unknown_assignee_without_listing_scope_has_no_candidates() {
        let (parts, ()) = axum::http::Request::builder()
            .uri("/mcp")
            .header(axum::http::header::AUTHORIZATION, "Bearer pat-1")
            .body(())
            .unwrap()
            .into_parts();
        let api = InnerApi::from_parts(&assignable_users_without_listing(), &parts).unwrap();
        let mut names = ProjectNames::new(&api, "t", "p");
        assert_eq!(
            names.user("nobody").await.unwrap_err().message,
            "Assignable user not found: nobody (this project has none)"
        );
        // UUID は一覧を引かずにそのまま返す
        let id = Uuid::from_u128(7);
        assert_eq!(names.user(&id.to_string()).await.unwrap(), id);
    }

    /// `?username=` は read:task と write:task のどちらでも通るので、403 の補足は両方を挙げる
    #[tokio::test]
    async fn forbidden_assignee_lookup_names_both_scopes() {
        let (parts, ()) = axum::http::Request::builder()
            .uri("/mcp")
            .header(axum::http::header::AUTHORIZATION, "Bearer pat-1")
            .body(())
            .unwrap()
            .into_parts();
        let forbidden = axum::Router::new().fallback(|| async {
            (
                axum::http::StatusCode::FORBIDDEN,
                axum::Json(json!({ "message": "forbidden" })),
            )
        });
        let api = InnerApi::from_parts(&forbidden, &parts).unwrap();
        let failure = ProjectNames::new(&api, "t", "p")
            .user("alice")
            .await
            .unwrap_err();
        assert_eq!(
            failure.text(),
            "403 forbidden (could not look up the assignee \"alice\": the token may lack the read:task or write:task scope, or it is not allowed in this project; if the scope is missing, pass the user UUID instead)"
        );
    }

    #[test]
    fn status_uuid_is_used_as_is() {
        let id = "00000000-0000-4000-8000-000000000009";
        assert_eq!(pick_status(&[], id).unwrap(), id);
    }

    #[test]
    fn matches_a_status_name_regardless_of_case() {
        let statuses = vec![status("Todo", true, 0), status("Doing", false, 1)];
        assert_eq!(
            pick_status(&statuses, "doing").unwrap(),
            id_of(&statuses[1])
        );
    }

    /// 綴りを外したとき、そのプロジェクトで何が使えるかを位置の順で添える
    #[test]
    fn missing_status_lists_the_candidates_by_position() {
        let statuses = vec![
            status("Done", false, 2),
            status("Todo", true, 0),
            status("In Progress", false, 1),
        ];
        assert_eq!(
            pick_status(&statuses, "nope").unwrap_err().message,
            "Status not found: nope (this project has Todo, In Progress, Done)"
        );
        assert_eq!(
            pick_status(&[], "nope").unwrap_err().message,
            "Status not found: nope (this project has none)"
        );
    }

    #[test]
    fn default_status_prefers_the_marked_one_then_the_first_position() {
        let marked = vec![status("Backlog", false, 0), status("Todo", true, 1)];
        assert_eq!(pick_default_status(&marked).unwrap(), id_of(&marked[1]));

        let unmarked = vec![status("Later", false, 2), status("First", false, 1)];
        assert_eq!(pick_default_status(&unmarked).unwrap(), id_of(&unmarked[1]));

        assert_eq!(
            pick_default_status(&[]).unwrap_err().message,
            "This project has no statuses to create a task in"
        );
    }

    #[test]
    fn bound_tenant_is_used_only_when_there_is_exactly_one() {
        let one = json!({ "tenant_ids": ["a"], "tenant_id": "a" });
        assert_eq!(pick_bound_tenant(&one).unwrap(), "a");

        // 旧形式の応答（`tenant_id` だけ）も 1 件として読む
        let legacy = json!({ "tenant_id": "a" });
        assert_eq!(pick_bound_tenant(&legacy).unwrap(), "a");

        let none = json!({ "tenant_ids": [], "tenant_id": null });
        assert_eq!(
            pick_bound_tenant(&none).unwrap_err().message,
            "The token is not bound to any tenant"
        );

        let many = json!({ "tenant_ids": ["a", "b"], "tenant_id": null });
        assert_eq!(
            pick_bound_tenant(&many).unwrap_err().message,
            "This token can be used in 2 tenants: a, b. Pass `tenant`."
        );
    }

    #[test]
    fn tenant_name_prefers_exact_match_then_a_single_case_insensitive_one() {
        let tenants = vec![
            json!({ "id": "1", "display_id": "Acme" }),
            json!({ "id": "2", "display_id": "acme" }),
            json!({ "id": "3", "display_id": "Solo" }),
        ];
        assert_eq!(pick_tenant(&tenants, "acme").unwrap(), "2");
        assert_eq!(pick_tenant(&tenants, "solo").unwrap(), "3");
        assert!(
            pick_tenant(&tenants, "ACME")
                .unwrap_err()
                .message
                .starts_with("Ambiguous tenant: ACME")
        );
        assert_eq!(
            pick_tenant(&tenants[2..], "nope").unwrap_err().message,
            "Tenant not found: nope (this token can be used in: Solo (3))"
        );
    }
}
