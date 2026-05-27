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
    #[serde(rename = "read:task")]
    ReadTask,
    #[serde(rename = "write:task")]
    WriteTask,
    #[serde(rename = "read:milestone")]
    ReadMilestone,
    #[serde(rename = "write:milestone")]
    WriteMilestone,
}

impl Scope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::ReadProject => "read:project",
            Scope::WriteProject => "write:project",
            Scope::ReadDrive => "read:drive",
            Scope::WriteDrive => "write:drive",
            Scope::AdminTenant => "admin:tenant",
            Scope::ReadTask => "read:task",
            Scope::WriteTask => "write:task",
            Scope::ReadMilestone => "read:milestone",
            Scope::WriteMilestone => "write:milestone",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "read:project" => Some(Scope::ReadProject),
            "write:project" => Some(Scope::WriteProject),
            "read:drive" => Some(Scope::ReadDrive),
            "write:drive" => Some(Scope::WriteDrive),
            "admin:tenant" => Some(Scope::AdminTenant),
            "read:task" => Some(Scope::ReadTask),
            "write:task" => Some(Scope::WriteTask),
            "read:milestone" => Some(Scope::ReadMilestone),
            "write:milestone" => Some(Scope::WriteMilestone),
            _ => None,
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
            || (scope == Scope::ReadDrive && self.0.contains(&Scope::WriteDrive))
            || (scope == Scope::ReadTask && self.0.contains(&Scope::WriteTask))
            || (scope == Scope::ReadMilestone && self.0.contains(&Scope::WriteMilestone))
    }
}

impl From<Scope> for sea_orm::Value {
    fn from(source: Scope) -> Self {
        sea_orm::Value::String(Some(source.as_str().to_string()))
    }
}
