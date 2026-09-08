use sea_orm::FromJsonQueryResult;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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

/// スコープが守る資源の層。`admin:tenant` / `admin:project` の wildcard は
/// この層を単位に効く（層の割り振り表は
/// apps/backend/docs/personal-access-tokens-authz.md）。
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
}

/// アクセストークン等に付与する権限スコープのリスト。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromJsonQueryResult, ToSchema)]
#[serde(transparent)]
pub struct ScopeList(pub Vec<Scope>);

impl ScopeList {
    pub fn has_scope(&self, scope: Scope) -> bool {
        self.0.contains(&scope)
            || self.0.contains(&Scope::AdminTenant)
            || (scope.layer() == ScopeLayer::Project && self.0.contains(&Scope::AdminProject))
            || (scope == Scope::ReadDrive && self.0.contains(&Scope::WriteDrive))
            || (scope == Scope::ReadTask && self.0.contains(&Scope::WriteTask))
            || (scope == Scope::ReadMilestone && self.0.contains(&Scope::WriteMilestone))
            || (scope == Scope::ReadSprint && self.0.contains(&Scope::WriteSprint))
            || (scope == Scope::ReadReview && self.0.contains(&Scope::WriteReview))
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

    const PROJECT_LAYER_SCOPES: [Scope; 13] = [
        Scope::AdminProject,
        Scope::ReadProject,
        Scope::WriteProject,
        Scope::ReadDrive,
        Scope::WriteDrive,
        Scope::ReadTask,
        Scope::WriteTask,
        Scope::ReadMilestone,
        Scope::WriteMilestone,
        Scope::ReadSprint,
        Scope::WriteSprint,
        Scope::ReadReview,
        Scope::WriteReview,
    ];

    #[test]
    fn admin_project_satisfies_all_project_layer_scopes() {
        let scopes = ScopeList(vec![Scope::AdminProject]);
        for scope in PROJECT_LAYER_SCOPES {
            assert!(
                scopes.has_scope(scope),
                "admin:project は project 層の {scope} を満たすはず"
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
    fn admin_tenant_satisfies_admin_project() {
        // admin:tenant の wildcard は全スコープに効く（既存意味論）。admin:project も含む
        let scopes = ScopeList(vec![Scope::AdminTenant]);
        assert!(scopes.has_scope(Scope::AdminProject));
        assert!(scopes.has_scope(Scope::AdminTenant));
    }

    #[test]
    fn enumerated_scope_does_not_imply_admin_project() {
        let scopes = ScopeList(vec![Scope::ReadTask, Scope::WriteTask]);
        assert!(!scopes.has_scope(Scope::AdminProject));
        assert!(!scopes.has_scope(Scope::AdminTenant));
    }

    #[test]
    fn write_scope_still_implies_read_pair() {
        // 既存の write ⊃ read 対が admin:project の追加で崩れておらぬこと
        let scopes = ScopeList(vec![Scope::WriteTask]);
        assert!(scopes.has_scope(Scope::ReadTask));
        assert!(!scopes.has_scope(Scope::ReadDrive));
    }

    #[test]
    fn admin_project_round_trips_as_str() {
        assert_eq!(Scope::AdminProject.as_str(), "admin:project");
        assert_eq!("admin:project".parse::<Scope>(), Ok(Scope::AdminProject));
    }
}
