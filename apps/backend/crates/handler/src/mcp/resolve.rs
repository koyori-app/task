//! キー・名前・参照から、API が要る ID を引く。
//!
//! 規則は CLI（`crates/cli/src/api.rs` の `resolve_tenant` / `pick_tenant` と
//! `crates/cli/src/resolve.rs`）と同じ。`handler` から `cli` は参照できないので、要る分を写している。
//! 片方を変えたら、もう片方も合わせる。

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
                "resolving tenant \"{name}\" needs the read:tenant scope; pass the tenant UUID instead"
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
    let projects = api
        .get(&["v1", "tenants", tenant_id, "projects"], &[])
        .await
        .map_err(|failure| {
            failure.explain_forbidden(|| {
                format!(
                    "resolving project key \"{project}\" needs the read:project scope; pass the project UUID instead"
                )
            })
        })?;
    list(&projects)
        .iter()
        .find(|candidate| str_field(candidate, "key").eq_ignore_ascii_case(project))
        .map(|found| str_field(found, "id").to_string())
        .ok_or_else(|| ToolFailure::new(format!("Project not found: {project}")))
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
        } => Ok((project_id(api, tenant_id, &project_key).await?, task_id)),
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
    let statuses = api
        .get(
            &[
                "v1", "tenants", tenant_id, "projects", project_id, "statuses",
            ],
            &[],
        )
        .await
        .map_err(|failure| {
            failure.explain_forbidden(|| {
                "resolving the status needs the read:task scope; pass the status UUID instead"
                    .to_string()
            })
        })?;
    Ok(list(&statuses).to_vec())
}

fn position(status: &Value) -> i64 {
    status.get("position").and_then(Value::as_i64).unwrap_or(0)
}

/// 状態を名前か UUID で指す（`crates/cli/src/resolve.rs` の `pick_status_by_name` / `pick_named_id`）。
///
/// 完全一致を優先し、無ければ大文字小文字を無視する。同名の候補が残れば UUID を求める。
pub(super) fn pick_status(statuses: &[Value], name: &str) -> Result<String, ToolFailure> {
    if is_uuid(name) {
        return Ok(name.to_string());
    }
    let mut statuses: Vec<&Value> = statuses.iter().collect();
    statuses.sort_by_key(|status| position(status));
    let exact: Vec<&Value> = statuses
        .iter()
        .copied()
        .filter(|status| str_field(status, "name") == name)
        .collect();
    let matches = if exact.is_empty() {
        statuses
            .iter()
            .copied()
            .filter(|status| str_field(status, "name").eq_ignore_ascii_case(name))
            .collect()
    } else {
        exact
    };
    match matches.as_slice() {
        [only] => Ok(str_field(only, "id").to_string()),
        [] => {
            let listed = if statuses.is_empty() {
                "none".to_string()
            } else {
                statuses
                    .iter()
                    .map(|status| str_field(status, "name"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            Err(ToolFailure::new(format!(
                "Status not found: {name} (this project has {listed})"
            )))
        }
        _ => Err(ToolFailure::new(format!(
            "Ambiguous Status: {name} (candidates: {}); specify a UUID",
            matches
                .iter()
                .map(|status| format!(
                    "{} ({})",
                    str_field(status, "name"),
                    str_field(status, "id")
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
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
