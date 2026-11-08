//! 応答に載せる列挙値を、知らない値でも読めるように受ける型。
//!
//! CLI は payload の型でそのまま応答を読むので、サーバーが列挙に値を足すと、配布済みの CLI は
//! その値を含む応答を丸ごと読めなくなる（`unknown variant`）。応答の列挙値をこれで包むと、
//! 知らない値は文字列のまま受け、ほかの欄は読める。直列化は中身そのままなので JSON の形は
//! 変わらない。OpenAPI では `T` のスキーマを `allOf` で包み、目印 [`OR_UNKNOWN_EXTENSION`] を
//! 付ける。応答の列挙欄に目印が無いと `handler::openapi` の固定テストが落ちる。
//!
//! 欄には `#[schema(inline)]` を付ける。付けないと utoipa は `OrUnknown_T` という別の
//! コンポーネントへの `$ref` を出す（目印はそちらに付くので固定テストは通るが、型名が増える）。

use std::borrow::Cow;
use std::fmt;

use serde::de::{DeserializeOwned, IntoDeserializer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::ToSchema;
use utoipa::openapi::extensions::ExtensionsBuilder;
use utoipa::openapi::schema::{AllOfBuilder, Schema};
use utoipa::openapi::{Ref, RefOr};

/// OpenAPI で `OrUnknown` の欄に付ける目印（拡張キー）。
pub const OR_UNKNOWN_EXTENSION: &str = "x-or-unknown";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrUnknown<T> {
    Known(T),
    /// この版が知らない値（新しいサーバーが足した値）
    Unknown(String),
}

impl<T> OrUnknown<T> {
    pub fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown(_) => None,
        }
    }
}

impl<T> From<T> for OrUnknown<T> {
    fn from(value: T) -> Self {
        Self::Known(value)
    }
}

impl<T: PartialEq> PartialEq<T> for OrUnknown<T> {
    fn eq(&self, other: &T) -> bool {
        self.known() == Some(other)
    }
}

impl<T: fmt::Display> fmt::Display for OrUnknown<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(value) => value.fmt(f),
            Self::Unknown(raw) => f.write_str(raw),
        }
    }
}

impl<T: Serialize> Serialize for OrUnknown<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Known(value) => value.serialize(serializer),
            Self::Unknown(raw) => raw.serialize(serializer),
        }
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for OrUnknown<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        // serde の綴り（rename 等）で読む。`FromStr` の有無や綴りの食い違いに左右されない
        let known = T::deserialize(
            IntoDeserializer::<serde::de::value::Error>::into_deserializer(raw.as_str()),
        );
        Ok(known.map(Self::Known).unwrap_or(Self::Unknown(raw)))
    }
}

// utoipa の derive は汎用型の欄を `ComposeSchema::compose` で組み立てる（`PartialSchema` は
// その blanket 実装）。`T` の引数は使わず、登録済みの `T` を `$ref` で指す
impl<T: ToSchema> utoipa::__dev::ComposeSchema for OrUnknown<T> {
    fn compose(_: Vec<RefOr<Schema>>) -> RefOr<Schema> {
        AllOfBuilder::new()
            .item(Ref::from_schema_name(T::name()))
            .extensions(Some(
                ExtensionsBuilder::new()
                    .add(OR_UNKNOWN_EXTENSION, true)
                    .build(),
            ))
            .into()
    }
}

impl<T: ToSchema> ToSchema for OrUnknown<T> {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("OrUnknown")
    }

    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        schemas.push((T::name().into(), T::schema()));
        T::schemas(schemas);
    }
}

#[cfg(test)]
mod tests {
    use entity::review_findings::FindingState;

    use super::OrUnknown;
    use crate::reviews::SeverityStateCount;

    /// 新しいサーバーが足した状態を含む応答も、知らない値を文字列のまま残して読める。
    #[test]
    fn unknown_values_are_kept_as_strings() {
        let actions: Vec<OrUnknown<FindingState>> =
            serde_json::from_str(r#"["fixed", "brand-new"]"#).expect("deserialize");
        assert_eq!(
            actions,
            vec![
                OrUnknown::Known(FindingState::Fixed),
                OrUnknown::Unknown("brand-new".into()),
            ]
        );
        assert_eq!(actions[1].as_str(), "brand-new");

        let count: SeverityStateCount =
            serde_json::from_str(r#"{"severity": "high", "state": "brand-new", "count": 2}"#)
                .expect("deserialize count");
        assert_eq!(count.state, OrUnknown::Unknown("brand-new".into()));
        assert_eq!(count.count, 2);
    }

    /// CLI が読む応答 DTO の列挙欄は、どれも知らない値を文字列のまま残して読める。
    #[test]
    fn response_dtos_keep_unknown_enum_values() {
        use entity::scopes::Scope;
        use serde_json::{Value, json};

        use crate::custom_fields::CustomFieldDefinitionSummary;
        use crate::mentions::MentionedUser;
        use crate::my_tasks::MyTaskItem;
        use crate::personal_tokens::PersonalTokenIdentityResponse;
        use crate::project_members::ProjectMemberResponse;
        use crate::reviews::{FindingResponse, FindingTransitionResponse, ReviewSummaryResponse};
        use crate::schedule::ScheduleTask;
        use crate::sprints::SprintResponse;
        use crate::tasks::TaskResponse;

        const ID: &str = "00000000-0000-4000-8000-000000000000";
        const AT: &str = "2026-01-01T00:00:00Z";
        fn read<T: serde::de::DeserializeOwned>(value: Value) -> T {
            serde_json::from_value(value).expect("deserialize")
        }
        fn new<T>() -> OrUnknown<T> {
            OrUnknown::Unknown("brand-new".to_string())
        }
        let user = json!({ "id": ID, "username": "alice" });
        let project = json!({ "id": ID, "name": "App", "key": "APP", "is_personal": false });
        let status = json!({ "id": ID, "name": "Todo", "color": "#000", "is_done_state": false });

        let finding: FindingResponse = read(json!({
            "id": ID, "review_id": ID, "pr_number": 1, "round": 1, "severity": "brand-new",
            "title": "t", "body": "b", "state": "brand-new", "created_at": AT, "updated_at": AT,
            "transitions": [], "available_actions": ["brand-new"],
        }));
        assert_eq!(finding.severity, new());
        assert_eq!(finding.state, new());
        assert_eq!(finding.available_actions, vec![new()]);

        let transition: FindingTransitionResponse = read(json!({
            "id": ID, "actor": user, "from_state": "brand-new", "to_state": "brand-new",
            "created_at": AT,
        }));
        assert_eq!(transition.from_state, Some(new()));
        assert_eq!(transition.to_state, new());

        let summary: ReviewSummaryResponse = read(json!({
            "pr_number": 1, "rounds": 1, "blocking": 0, "owner_override_rejections": 0,
            "mergeable": true, "gate": "brand-new",
            "counts": [{ "severity": "brand-new", "state": "open", "count": 1 }],
        }));
        assert_eq!(summary.gate, new());
        assert_eq!(summary.counts[0].severity, new());

        let task: TaskResponse = read(json!({
            "id": ID, "project_id": ID, "seq_id": 1, "title": "t", "status_id": ID,
            "priority": "brand-new", "progress_pct": 0, "is_archived": false, "assignees": [],
            "labels": [], "created_at": AT, "updated_at": AT,
        }));
        assert_eq!(task.priority, new());

        let my_task: MyTaskItem = read(json!({
            "id": ID, "seq_id": 1, "seq_key": "APP-1", "title": "t", "status": status,
            "priority": "brand-new", "project": project, "is_personal": false,
        }));
        assert_eq!(my_task.priority, new());

        let scheduled: ScheduleTask = read(json!({
            "id": ID, "seq_id": 1, "seq_key": "APP-1", "title": "t", "project": project,
            "status": status, "priority": "brand-new", "progress_pct": 0, "logged_minutes": 0,
            "assignees": [],
        }));
        assert_eq!(scheduled.priority, new());

        let sprint: SprintResponse = read(json!({
            "id": ID, "project_id": ID, "name": "s", "start_date": "2026-01-01",
            "end_date": "2026-01-14", "status": "brand-new", "created_by": ID,
            "created_at": AT, "updated_at": AT,
        }));
        assert_eq!(sprint.status, new());

        let mentioned: MentionedUser = read(json!({ "status": "brand-new", "name": "alice" }));
        assert_eq!(mentioned.status, new());

        let field: CustomFieldDefinitionSummary = read(json!({
            "id": ID, "name": "f", "field_type": "brand-new", "is_required": false, "position": 0,
        }));
        assert_eq!(field.field_type, new());

        let member: ProjectMemberResponse = read(json!({
            "id": ID, "project_id": ID, "user_id": ID, "role": "brand-new", "user": user,
        }));
        assert_eq!(member.role, new());

        // serde の綴り（rename）で読む。`read:task` は既知、足された値だけ文字列で残る
        let identity: PersonalTokenIdentityResponse = read(json!({
            "id": ID, "name": "bot", "user_id": ID, "username": "bot", "tenant_id": ID,
            "scopes": ["read:task", "brand-new"],
        }));
        assert_eq!(
            identity.scopes,
            vec![OrUnknown::Known(Scope::ReadTask), new()]
        );
    }

    /// 直列化は中身そのまま。包んでも JSON の形は変わらない。
    #[test]
    fn serializes_as_the_plain_value() {
        let known = serde_json::to_value(OrUnknown::Known(FindingState::Open)).expect("serialize");
        assert_eq!(known, serde_json::json!("open"));
        let unknown = serde_json::to_value(OrUnknown::<FindingState>::Unknown("x".into()))
            .expect("serialize");
        assert_eq!(unknown, serde_json::json!("x"));
    }
}
