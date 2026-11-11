use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::or_unknown::OrUnknown;
use crate::tenant_members::OrgProfile;
use crate::users::UserSummary;
use entity::project_members::{self, ProjectRole};
use entity::users;

/// 担当者候補。`UserSummary` に、このテナントでの部署・チームを足したもの。
///
/// 部署・チームはプロフィールと同じ主体にしか返さない（`read:tenant` を持ち、テナントに
/// 入れる人。客分やプロジェクト限定の PAT には見せない）。それ以外の主体では全員 null
#[derive(Debug, Clone, Serialize, ToSchema, serde::Deserialize)]
pub struct AssignableUserResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub username: String,
    #[serde(default = "crate::users::default_account_kind")]
    #[schema(required = false)]
    pub account_kind: String,
    #[schema(nullable)]
    pub avatar_url: Option<String>,
    /// このテナントでの部署。未設定・読めない主体では null
    #[schema(nullable)]
    pub department: Option<String>,
    /// このテナントでのチーム。未設定・読めない主体では null
    #[schema(nullable)]
    pub team: Option<String>,
}

impl AssignableUserResponse {
    pub fn from_parts(user: users::Model, org: OrgProfile) -> Self {
        Self {
            id: user.id,
            username: user.username,
            account_kind: user.account_kind,
            avatar_url: user.avatar_url,
            department: org.department,
            team: org.team,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema, serde::Deserialize)]
pub struct ProjectMemberResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub project_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    #[schema(inline)]
    pub role: OrUnknown<ProjectRole>,
    /// 表示用のユーザー情報。メンバー管理 UI が名前・アバターを引けるように同梱する
    pub user: UserSummary,
}

impl ProjectMemberResponse {
    pub fn from_parts(member: project_members::Model, user: entity::users::Model) -> Self {
        Self {
            id: member.id,
            project_id: member.project_id,
            user_id: member.user_id,
            role: member.role.into(),
            user: user.into(),
        }
    }
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct AddMemberRequest {
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    pub role: ProjectRole,
}

#[derive(Validate, Debug, Deserialize, ToSchema)]
pub struct UpdateMemberRequest {
    pub role: ProjectRole,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AssignableUsersQuery {
    /// 指定するとその利用者だけを返す（大文字小文字を無視した完全一致）。
    /// 名前から ID を引くだけの経路で、候補の列挙とは公開範囲が違う。
    pub username: Option<String>,
}
