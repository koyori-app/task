use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::users::UserSummary;
use entity::project_members::ProjectRole;
use entity::tenant_members::{self, TenantRole};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TenantMemberResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub tenant_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub role: TenantRole,
    /// 表示用のユーザー情報。メンバー管理 UI が名前・アバターを引けるように同梱する
    pub user: UserSummary,
    /// このテナント（会社）での部署
    #[schema(nullable)]
    pub department: Option<String>,
    /// このテナント（会社）での役職
    #[schema(nullable)]
    pub position: Option<String>,
    /// このテナント（会社）でのチーム
    #[schema(nullable)]
    pub team: Option<String>,
}

impl TenantMemberResponse {
    pub fn from_parts(member: tenant_members::Model, user: entity::users::Model) -> Self {
        Self {
            id: member.id,
            tenant_id: member.tenant_id,
            user_id: member.user_id,
            role: member.role,
            user: user.into(),
            department: member.department,
            position: member.position,
            team: member.team,
        }
    }

    /// テナント owner は `tenant_members` に行を持たないため、一覧表示用に合成する。
    pub fn from_owner(tenant_id: Uuid, user: entity::users::Model) -> Self {
        Self {
            id: user.id,
            tenant_id,
            user_id: user.id,
            role: TenantRole::Admin,
            user: user.into(),
            department: None,
            position: None,
            team: None,
        }
    }
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct AddTenantMemberRequest {
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub role: TenantRole,
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
/// どの欄も省略（`None`）なら変更しない。部署・役職・チームは trim して空なら NULL に戻す。
/// `role` を省略可にしているのは、所属だけを直す保存が古い `role` を送り返して、
/// 別の管理者による降格を巻き戻さないようにするため
pub struct UpdateTenantMemberRequest {
    pub role: Option<TenantRole>,
    #[validate(length(max = 100))]
    pub department: Option<String>,
    #[validate(length(max = 100))]
    pub position: Option<String>,
    #[validate(length(max = 100))]
    pub team: Option<String>,
}

/// その人が持つ、プロジェクト単位の明示 ACE。
/// テナント所属（継承）を外しても明示 ACE は独立して残り、持ち主は客分としてそのプロジェクトに入り続ける。
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ExplicitProjectAce {
    #[schema(value_type = String, format = "uuid")]
    pub project_id: Uuid,
    pub key: String,
    pub name: String,
    pub role: ProjectRole,
}

/// その人がこのテナントで持つ明示 ACE の一覧。
/// テナント除名の前に「除名しても、この人はこれらのプロジェクトに入り続ける」と確かめるために使う。
/// 管理者が「除名したのにまだ入れる」と驚かないためのもので、除名の後に呼んでも同じ内容を返す。
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ExplicitProjectsResponse {
    /// 呼び出し側から見えるプロジェクトの明示 ACE。
    /// 閉め出したい場合は、ここに挙がったプロジェクトのメンバーからも外す。
    pub explicit_projects: Vec<ExplicitProjectAce>,
    /// 呼び出し側には見えないプロジェクトに残る明示 ACE の件数。
    /// 非公開プロジェクトの名前や key を、そのプロジェクトに入れない人へ出さないために数だけ返す。
    pub hidden_count: u64,
}
