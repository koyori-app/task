use sea_orm::{EnumIter, FromJsonQueryResult};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, Serialize, Deserialize, ToSchema)]
pub enum Scope {
    #[serde(rename = "read:project")]
    ReadProject,
    #[serde(rename = "write:project")]
    WriteProject,
    #[serde(rename = "read:drive")]
    ReadDrive,
    #[serde(rename = "write:drive")]
    WriteDrive,
    #[serde(rename = "admin:tenant")]
    AdminTenant,
    #[serde(rename = "admin:project")]
    AdminProject,
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
            Scope::ReadProject => "read:project",
            Scope::WriteProject => "write:project",
            Scope::ReadDrive => "read:drive",
            Scope::WriteDrive => "write:drive",
            Scope::AdminTenant => "admin:tenant",
            Scope::AdminProject => "admin:project",
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
            "read:project" => Ok(Scope::ReadProject),
            "write:project" => Ok(Scope::WriteProject),
            "read:drive" => Ok(Scope::ReadDrive),
            "write:drive" => Ok(Scope::WriteDrive),
            "admin:tenant" => Ok(Scope::AdminTenant),
            "admin:project" => Ok(Scope::AdminProject),
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

/// スコープが守る資源の層。層を単位に効く wildcard は `admin:project` だけである
/// （層の割り振り表は apps/backend/docs/personal-access-tokens-authz.md）。
///
/// `admin:tenant` は層に依らぬ最上位で、要求されたスコープが何であれ通す
/// （`Scope::implies`）。層は `admin:project` の効き目を限る道具であって、
/// `admin:tenant` を限る道具ではない。層を増やしても `admin:tenant` は通る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeLayer {
    Tenant,
    Project,
}

impl Scope {
    /// 各スコープの層。スコープを増やしたら必ずどちらかへ割り振る
    /// （catch-all を置かず、割り振り忘れをコンパイルエラーにする）。
    pub fn layer(&self) -> ScopeLayer {
        match self {
            Scope::AdminTenant => ScopeLayer::Tenant,
            Scope::AdminProject
            | Scope::ReadProject
            | Scope::WriteProject
            | Scope::ReadDrive
            | Scope::WriteDrive
            | Scope::ReadTask
            | Scope::WriteTask
            | Scope::ReadMilestone
            | Scope::WriteMilestone
            | Scope::ReadSprint
            | Scope::WriteSprint
            | Scope::ReadReview
            | Scope::WriteReview => ScopeLayer::Project,
        }
    }

    /// このスコープを持つ鍵が `other` の要求を満たすか。
    ///
    /// 含意の規則は `self`（持っている側）に対する網羅 match で書く。catch-all を
    /// 置かぬので、スコープを増やしたら何を含意するかを決めるまでコンパイルが通らぬ。
    pub fn implies(self, other: Scope) -> bool {
        if self == other {
            return true;
        }
        match self {
            // 層に依らぬ最上位。要求が何であれ通す
            Scope::AdminTenant => true,
            // project 層に限った wildcard
            Scope::AdminProject => other.layer() == ScopeLayer::Project,
            // write は対になる read を含む
            Scope::WriteProject => other == Scope::ReadProject,
            Scope::WriteDrive => other == Scope::ReadDrive,
            Scope::WriteTask => other == Scope::ReadTask,
            Scope::WriteMilestone => other == Scope::ReadMilestone,
            Scope::WriteSprint => other == Scope::ReadSprint,
            Scope::WriteReview => other == Scope::ReadReview,
            // read は己の分しか満たさぬ
            Scope::ReadProject
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
    const SCOPE_COUNT: usize = 14;

    /// project 層のスコープ。全バリアントから `layer()` で絞るので、写しを持たぬ。
    fn project_layer_scopes() -> Vec<Scope> {
        Scope::iter()
            .filter(|scope| scope.layer() == ScopeLayer::Project)
            .collect()
    }

    #[test]
    fn scope_iter_covers_every_variant() {
        assert_eq!(
            Scope::iter().count(),
            SCOPE_COUNT,
            "スコープを増減したなら、含意の規則と層の割り振りを見直してからこの数を直せ"
        );
        for scope in Scope::iter() {
            assert_eq!(
                scope.as_str().parse::<Scope>(),
                Ok(scope),
                "{scope} の文字列表現が往復せぬ（as_str と FromStr の対応漏れ）"
            );
        }
    }

    #[test]
    fn admin_project_satisfies_all_project_layer_scopes() {
        let project_scopes = project_layer_scopes();
        // 絞り込みが空振りしておらぬこと。空の for は黙って素通りする
        assert_eq!(
            project_scopes.len(),
            SCOPE_COUNT - 1,
            "tenant 層に属するのは admin:tenant の 1 件だけのはず"
        );

        let scopes = ScopeList(vec![Scope::AdminProject]);
        for scope in project_scopes {
            assert!(
                scopes.has_scope(scope),
                "admin:project は project 層の {scope} を満たすはず"
            );
        }
    }

    #[test]
    fn admin_tenant_satisfies_every_scope() {
        // admin:tenant の wildcard は層に依らず全スコープに効く（既存意味論）。
        // admin:project も含む
        let scopes = ScopeList(vec![Scope::AdminTenant]);
        for scope in Scope::iter() {
            assert!(
                scopes.has_scope(scope),
                "admin:tenant は層に依らず {scope} を満たすはず"
            );
        }
    }

    #[test]
    fn admin_project_does_not_satisfy_tenant_layer() {
        let scopes = ScopeList(vec![Scope::AdminProject]);
        assert!(
            !scopes.has_scope(Scope::AdminTenant),
            "admin:project は tenant 層の admin:tenant を満たしてはならぬ"
        );
    }

    #[test]
    fn enumerated_scope_does_not_imply_admin_project() {
        let scopes = ScopeList(vec![Scope::ReadTask, Scope::WriteTask]);
        assert!(!scopes.has_scope(Scope::AdminProject));
        assert!(!scopes.has_scope(Scope::AdminTenant));
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
                "{read} が {write} を含意してはならぬ"
            );
            // 腕が広がり過ぎておらぬこと。write が満たすのは己と対の read だけである
            for other in Scope::iter() {
                if other == write || other == read {
                    continue;
                }
                assert!(
                    !ScopeList(vec![write]).has_scope(other),
                    "{write} が {other} まで含意してはならぬ"
                );
            }
            pairs += 1;
        }
        assert_eq!(pairs, 6, "write/read の対は project を含めて 6 組");

        // 対を跨いでは効かぬ
        let write_task = ScopeList(vec![Scope::WriteTask]);
        assert!(!write_task.has_scope(Scope::ReadDrive));
        assert!(!write_task.has_scope(Scope::ReadProject));
    }

    #[test]
    fn admin_project_round_trips_as_str() {
        assert_eq!(Scope::AdminProject.as_str(), "admin:project");
        assert_eq!("admin:project".parse::<Scope>(), Ok(Scope::AdminProject));
    }
}
