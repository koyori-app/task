use std::ops::Deref;

use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use axum_session_redispool::SessionRedisPool;
use sea_orm::{ColumnTrait, EntityTrait, JoinType, QueryFilter, QuerySelect, RelationTrait, prelude::Uuid};

use crate::{
    AppState,
    entities::{project_members, projects, scopes::Scope, tenants, users},
    error::AppError,
    utils::auth::{AuthError, authenticate_personal_token},
};

type Session = axum_session::Session<SessionRedisPool>;

async fn user_id_from_session(parts: &mut Parts, state: &AppState) -> Result<Uuid, AuthError> {
    let session = Session::from_request_parts(parts, state)
        .await
        .map_err(|_| AuthError::Internal(anyhow::anyhow!("session layer missing")))?;

    session
        .get::<Uuid>("user_id")
        .ok_or(AuthError::Unauthorized)
}

fn bearer_token_from_parts(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
}

#[derive(Debug, Clone)]
pub enum AuthMethod {
    Session,
    PersonalToken {
        token_id: Uuid,
        tenant_id: Uuid,
        allowed_project_ids: Option<Vec<Uuid>>,
        scopes: crate::entities::scopes::ScopeList,
    },
}

/// 認証済みユーザー（セッションまたは PAT）
pub struct AuthUser {
    pub user_id: Uuid,
    pub method: AuthMethod,
}

impl AuthUser {
    /// PAT 管理 API などセッション専用エンドポイント向け。
    pub fn require_session(&self) -> Result<(), AppError> {
        match self.method {
            AuthMethod::Session => Ok(()),
            AuthMethod::PersonalToken { .. } => Err(AppError::Forbidden),
        }
    }

    /// 操作スコープチェック。セッションは常に通過。
    pub fn require_scope(&self, scope: Scope) -> Result<(), AppError> {
        match &self.method {
            AuthMethod::Session => Ok(()),
            AuthMethod::PersonalToken { scopes, .. } => {
                if scopes.has_scope(scope) {
                    Ok(())
                } else {
                    Err(AppError::Forbidden)
                }
            }
        }
    }

    /// テナントオーナー専用操作向け。PAT 境界チェック + オーナー確認を一括実施し、
    /// テナントモデルを返す（呼び出し側で再取得不要）。
    /// `ensure_tenant_access` + `owner_id` 二重チェックの代替として使用する。
    pub async fn ensure_tenant_owner(
        &self,
        state: &AppState,
        tenant_id: Uuid,
    ) -> Result<tenants::Model, AppError> {
        // PAT は自テナント以外への操作を禁止
        if let AuthMethod::PersonalToken {
            tenant_id: pat_tenant,
            allowed_project_ids,
            ..
        } = &self.method
        {
            if tenant_id != *pat_tenant {
                return Err(AppError::Forbidden);
            }
            // プロジェクト制限付き PAT はテナント全体操作不可
            if allowed_project_ids.is_some() {
                return Err(AppError::Forbidden);
            }
        }
        let tenant = tenants::Entity::find_by_id(tenant_id)
            .one(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;
        if tenant.owner_id != self.user_id {
            return Err(AppError::Forbidden);
        }
        Ok(tenant)
    }

    /// テナント / プロジェクト境界チェック。
    pub async fn ensure_tenant_access(
        &self,
        state: &AppState,
        tenant_id: Uuid,
        project_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        match &self.method {
            AuthMethod::Session => {
                session_has_tenant_access(state, self.user_id, tenant_id, project_id).await
            }
            AuthMethod::PersonalToken {
                tenant_id: pat_tenant,
                allowed_project_ids,
                ..
            } => {
                if tenant_id != *pat_tenant {
                    return Err(AppError::Forbidden);
                }
                if let Some(project_id) = project_id {
                    if let Some(allowed) = allowed_project_ids {
                        if !allowed.contains(&project_id) {
                            return Err(AppError::Forbidden);
                        }
                    }
                    verify_project_in_tenant(state, tenant_id, project_id).await?;
                } else if allowed_project_ids.is_some() {
                    // プロジェクト制限付き PAT はテナント全体操作（project_id=None）を禁止
                    return Err(AppError::Forbidden);
                }
                Ok(())
            }
        }
    }
}

async fn verify_project_in_tenant(
    state: &AppState,
    tenant_id: Uuid,
    project_id: Uuid,
) -> Result<(), AppError> {
    let exists = projects::Entity::find_by_id(project_id)
        .filter(projects::Column::TenantId.eq(tenant_id))
        .one(&state.db)
        .await?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

async fn session_has_tenant_access(
    state: &AppState,
    user_id: Uuid,
    tenant_id: Uuid,
    project_id: Option<Uuid>,
) -> Result<(), AppError> {
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if tenant.owner_id == user_id {
        if let Some(pid) = project_id {
            verify_project_in_tenant(state, tenant_id, pid).await?;
        }
        return Ok(());
    }

    if let Some(pid) = project_id {
        verify_project_in_tenant(state, tenant_id, pid).await?;
        let is_member = project_members::Entity::find()
            .filter(project_members::Column::ProjectId.eq(pid))
            .filter(project_members::Column::UserId.eq(user_id))
            .one(&state.db)
            .await?
            .is_some();
        if is_member {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    } else {
        let is_member = project_members::Entity::find()
            .join(JoinType::InnerJoin, project_members::Relation::Projects.def())
            .filter(project_members::Column::UserId.eq(user_id))
            .filter(projects::Column::TenantId.eq(tenant_id))
            .one(&state.db)
            .await?
            .is_some();
        if is_member {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}

/// 認証任意（未認証は `None`）。コンテンツ配信などで使用。
pub struct OptionalAuthUser(pub Option<AuthUser>);

impl FromRequestParts<AppState> for OptionalAuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match AuthUser::from_request_parts(parts, state).await {
            Ok(auth) => Ok(OptionalAuthUser(Some(auth))),
            Err(AuthError::Unauthorized) => Ok(OptionalAuthUser(None)),
            Err(e) => Err(e),
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(token) = bearer_token_from_parts(parts) {
            let record = authenticate_personal_token(&state.db, &state.settings.personal_token_secret, &token).await?;
            let user = users::Entity::find_by_id(record.user_id)
                .one(&state.db)
                .await?
                .ok_or(AuthError::Unauthorized)?;
            if user.is_suspended {
                return Err(AuthError::Suspended);
            }
            Ok(AuthUser {
                user_id: record.user_id,
                method: AuthMethod::PersonalToken {
                    token_id: record.id,
                    tenant_id: record.tenant_id,
                    allowed_project_ids: match record.allowed_project_ids.as_ref() {
                        None => None,
                        Some(v) => crate::entities::personal_tokens::parse_allowed_project_ids(v)
                            .map_err(|e| AuthError::Internal(anyhow::anyhow!("allowed_project_ids parse error: {e}")))?,
                    },
                    scopes: record.scopes.clone(),
                },
            })
        } else {
            let user_id = user_id_from_session(parts, state).await?;
            let user = users::Entity::find_by_id(user_id)
                .one(&state.db)
                .await?
                .ok_or(AuthError::Unauthorized)?;
            if user.is_suspended {
                return Err(AuthError::Suspended);
            }
            Ok(AuthUser {
                user_id,
                method: AuthMethod::Session,
            })
        }
    }
}

/// 管理者専用エクストラクタ（セッション認証のみ）
pub struct AdminUser {
    pub user_id: Uuid,
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user_id = user_id_from_session(parts, state).await?;
        let user = users::Entity::find_by_id(user_id)
            .one(&state.db)
            .await?
            .ok_or(AuthError::Unauthorized)?;
        if user.is_suspended {
            return Err(AuthError::Suspended);
        }
        if !user.is_admin {
            return Err(AuthError::Forbidden);
        }
        Ok(AdminUser { user_id })
    }
}

/// 認証済みユーザーの DB レコード（ハンドラで毎回取得する必要なし）
pub struct CurrentUser(pub users::Model);

impl Deref for CurrentUser {
    type Target = users::Model;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user_id = user_id_from_session(parts, state).await?;
        let user = users::Entity::find_by_id(user_id)
            .one(&state.db)
            .await?
            .ok_or(AuthError::Unauthorized)?;
        Ok(CurrentUser(user))
    }
}
