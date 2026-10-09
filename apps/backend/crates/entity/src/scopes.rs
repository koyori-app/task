use sea_orm::{EnumIter, FromJsonQueryResult};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, Serialize, Deserialize, ToSchema)]
pub enum Scope {
    /// すべての `read:*` / `write:*` を満たす（GitLab の `api` と同じ考え方）
    #[serde(rename = "api")]
    Api,
    /// すべての `read:*` を満たす（GitLab の `read_api` と同じ考え方）
    #[serde(rename = "read_api")]
    ReadApi,
    #[serde(rename = "read:tenant")]
    ReadTenant,
    #[serde(rename = "write:tenant")]
    WriteTenant,
    #[serde(rename = "read:project")]
    ReadProject,
    #[serde(rename = "write:project")]
    WriteProject,
    #[serde(rename = "read:drive")]
    ReadDrive,
    #[serde(rename = "write:drive")]
    WriteDrive,
    #[serde(rename = "read:task")]
    ReadTask,
    #[serde(rename = "write:task")]
    WriteTask,
    #[serde(rename = "read:milestone")]
    ReadMilestone,
    #[serde(rename = "write:milestone")]
    WriteMilestone,
    #[serde(rename = "read:sprint")]
    ReadSprint,
    #[serde(rename = "write:sprint")]
    WriteSprint,
    #[serde(rename = "read:review")]
    ReadReview,
    #[serde(rename = "write:review")]
    WriteReview,
}

impl Scope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::Api => "api",
            Scope::ReadApi => "read_api",
            Scope::ReadTenant => "read:tenant",
            Scope::WriteTenant => "write:tenant",
            Scope::ReadProject => "read:project",
            Scope::WriteProject => "write:project",
            Scope::ReadDrive => "read:drive",
            Scope::WriteDrive => "write:drive",
            Scope::ReadTask => "read:task",
            Scope::WriteTask => "write:task",
            Scope::ReadMilestone => "read:milestone",
            Scope::WriteMilestone => "write:milestone",
            Scope::ReadSprint => "read:sprint",
            Scope::WriteSprint => "write:sprint",
            Scope::ReadReview => "read:review",
            Scope::WriteReview => "write:review",
        }
    }
}

impl std::str::FromStr for Scope {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "api" => Ok(Scope::Api),
            "read_api" => Ok(Scope::ReadApi),
            "read:tenant" => Ok(Scope::ReadTenant),
            "write:tenant" => Ok(Scope::WriteTenant),
            "read:project" => Ok(Scope::ReadProject),
            "write:project" => Ok(Scope::WriteProject),
            "read:drive" => Ok(Scope::ReadDrive),
            "write:drive" => Ok(Scope::WriteDrive),
            "read:task" => Ok(Scope::ReadTask),
            "write:task" => Ok(Scope::WriteTask),
            "read:milestone" => Ok(Scope::ReadMilestone),
            "write:milestone" => Ok(Scope::WriteMilestone),
            "read:sprint" => Ok(Scope::ReadSprint),
            "write:sprint" => Ok(Scope::WriteSprint),
            "read:review" => Ok(Scope::ReadReview),
            "write:review" => Ok(Scope::WriteReview),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// スコープの動詞。`api` / `read_api` はこれを単位に効く
/// （apps/backend/docs/personal-access-tokens-authz.md の含意の規則）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeVerb {
    Read,
    Write,
    /// `api` / `read_api` 自身。資源を持たない
    Wildcard,
}

impl Scope {
    /// 各スコープの動詞。スコープを増やしたら必ずどれかへ割り振る
    /// （catch-all を置かず、割り振り忘れをコンパイルエラーにする）。
    pub fn verb(&self) -> ScopeVerb {
        match self {
            Scope::Api | Scope::ReadApi => ScopeVerb::Wildcard,
            Scope::ReadTenant
            | Scope::ReadProject
            | Scope::ReadDrive
            | Scope::ReadTask
            | Scope::ReadMilestone
            | Scope::ReadSprint
            | Scope::ReadReview => ScopeVerb::Read,
            Scope::WriteTenant
            | Scope::WriteProject
            | Scope::WriteDrive
            | Scope::WriteTask
            | Scope::WriteMilestone
            | Scope::WriteSprint
            | Scope::WriteReview => ScopeVerb::Write,
        }
    }

    /// このスコープを持つ鍵が `other` の要求を満たすか。
    ///
    /// 含意の規則は `self`（持っている側）に対する網羅 match で書く。catch-all を
    /// 置かないので、スコープを増やしたら何を含意するかを決めるまでコンパイルが通らない。
    pub fn implies(self, other: Scope) -> bool {
        if self == other {
            return true;
        }
        match self {
            // read / write の全スコープ（と read_api）を満たす
            Scope::Api => true,
            // read の全スコープを満たす。write は満たさない
            Scope::ReadApi => other.verb() == ScopeVerb::Read,
            // write は対になる read を含む
            Scope::WriteTenant => other == Scope::ReadTenant,
            Scope::WriteProject => other == Scope::ReadProject,
            Scope::WriteDrive => other == Scope::ReadDrive,
            Scope::WriteTask => other == Scope::ReadTask,
            Scope::WriteMilestone => other == Scope::ReadMilestone,
            Scope::WriteSprint => other == Scope::ReadSprint,
            Scope::WriteReview => other == Scope::ReadReview,
            // read は自分の分しか満たさない
            Scope::ReadTenant
            | Scope::ReadProject
            | Scope::ReadDrive
            | Scope::ReadTask
            | Scope::ReadMilestone
            | Scope::ReadSprint
            | Scope::ReadReview => false,
        }
    }
}

/// アクセストークン等に付与する権限スコープのリスト。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult, ToSchema)]
#[serde(transparent)]
pub struct ScopeList(pub Vec<Scope>);

impl ScopeList {
    pub fn has_scope(&self, scope: Scope) -> bool {
        self.0.iter().any(|held| held.implies(scope))
    }
}

impl From<Scope> for sea_orm::Value {
    fn from(source: Scope) -> Self {
        sea_orm::Value::String(Some(source.as_str().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::Iterable;

    /// enum のバリアント数。増やしたら `scope_iter_covers_every_variant` が落ちる。
    const SCOPE_COUNT: usize = 16;

    fn scopes_with_verb(verb: ScopeVerb) -> Vec<Scope> {
        Scope::iter().filter(|scope| scope.verb() == verb).collect()
    }

    #[test]
    fn scope_iter_covers_every_variant() {
        assert_eq!(
            Scope::iter().count(),
            SCOPE_COUNT,
            "スコープを増減したなら、含意の規則と動詞の割り振りを見直してからこの数を直す"
        );
        for scope in Scope::iter() {
            assert_eq!(
                scope.as_str().parse::<Scope>(),
                Ok(scope),
                "{scope} の文字列表現が往復しない（as_str と FromStr の対応漏れ）"
            );
            assert_eq!(
                serde_json::to_value(scope).expect("serialize scope"),
                serde_json::Value::String(scope.as_str().to_string()),
                "{scope} の serde 名が as_str と食い違う"
            );
        }
    }

    #[test]
    fn api_satisfies_every_scope() {
        let scopes = ScopeList(vec![Scope::Api]);
        for scope in Scope::iter() {
            assert!(scopes.has_scope(scope), "api は {scope} を満たすはず");
        }
    }

    #[test]
    fn read_api_satisfies_every_read_scope_and_no_write_scope() {
        let reads = scopes_with_verb(ScopeVerb::Read);
        let writes = scopes_with_verb(ScopeVerb::Write);
        // 絞り込みが空振りしていないこと。空の for は黙って素通りする
        assert_eq!(reads.len(), 7, "read:* は tenant を含めて 7 個");
        assert_eq!(writes.len(), 7, "write:* は tenant を含めて 7 個");

        let scopes = ScopeList(vec![Scope::ReadApi]);
        for scope in reads {
            assert!(scopes.has_scope(scope), "read_api は {scope} を満たすはず");
        }
        for scope in writes {
            assert!(
                !scopes.has_scope(scope),
                "read_api が {scope} を満たしてはならない"
            );
        }
        assert!(
            !scopes.has_scope(Scope::Api),
            "read_api は api を満たさない"
        );
    }

    #[test]
    fn enumerated_scopes_do_not_imply_wildcards() {
        let scopes = ScopeList(scopes_with_verb(ScopeVerb::Write));
        assert!(!scopes.has_scope(Scope::Api));
        assert!(!scopes.has_scope(Scope::ReadApi));
    }

    #[test]
    fn write_scope_still_implies_read_pair() {
        // 対は名前から導く。スコープを増やしても写しを直さずに済む
        let mut pairs = 0;
        for write in Scope::iter() {
            let Some(resource) = write.as_str().strip_prefix("write:") else {
                continue;
            };
            let read: Scope = format!("read:{resource}")
                .parse()
                .unwrap_or_else(|_| panic!("{write} と対になる read スコープが無い"));
            assert!(
                ScopeList(vec![write]).has_scope(read),
                "{write} は {read} を含意するはず"
            );
            assert!(
                !ScopeList(vec![read]).has_scope(write),
                "{read} が {write} を含意してはならない"
            );
            // 腕が広がり過ぎていないこと。write が満たすのは自分と対の read だけ
            for other in Scope::iter() {
                if other == write || other == read {
                    continue;
                }
                assert!(
                    !ScopeList(vec![write]).has_scope(other),
                    "{write} が {other} まで含意してはならない"
                );
            }
            pairs += 1;
        }
        assert_eq!(
            pairs, 7,
            "write/read の対は tenant と project を含めて 7 組"
        );

        // 対を跨いでは効かない
        let write_task = ScopeList(vec![Scope::WriteTask]);
        assert!(!write_task.has_scope(Scope::ReadDrive));
        assert!(!write_task.has_scope(Scope::ReadProject));
    }

    #[test]
    fn retired_admin_scopes_no_longer_parse() {
        // 既存トークンの値はマイグレーション（m20261004000000_pat_scopes_api）で書き換える
        assert!("admin:tenant".parse::<Scope>().is_err());
        assert!("admin:project".parse::<Scope>().is_err());
    }
}
