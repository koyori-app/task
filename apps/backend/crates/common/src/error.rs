//! ハンドラ共通の API エラー型。

use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;
use tracing::error;
use utoipa::ToSchema;

/// Sentry へ送信済みのエラーをログに出すときの tracing target。
/// tracing の Sentry レイヤーはこの target を送らない（同じエラーの二重送信を防ぐ）。
pub const SENTRY_CAPTURED_LOG_TARGET: &str = "common::error::sentry_captured";

/// API 共通のエラー応答ボディ。
#[derive(Serialize, ToSchema)]
pub struct ServerError {
    #[schema(example = "internal-error")]
    pub message: String,
}

/// 認証・認可以外の一般ハンドラ向けエラー。
#[derive(Debug, Error)]
pub enum AppError {
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("forbidden: {0}")]
    ForbiddenDetail(String),
    #[error("conflict")]
    Conflict,
    #[error("conflict: {0}")]
    ConflictDetail(String),
    #[error("bad request")]
    BadRequest,
    #[error("bad request: {0}")]
    BadRequestDetail(String),
    #[error("gone")]
    Gone,
    #[error("unprocessable entity")]
    UnprocessableEntity,
    #[error("content too large")]
    ContentTooLarge,
    #[error("precondition failed")]
    PreconditionFailed,
    #[error("too many requests")]
    TooManyRequests,
    /// 429 に `Retry-After: <秒>` を添える。
    #[error("too many requests")]
    TooManyRequestsRetryAfter(u64),
}

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self {
        let msg = err.to_string();
        if msg.contains("duplicate key") || msg.contains("UNIQUE constraint failed") {
            return AppError::Conflict;
        }
        AppError::Internal(err.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Internal(e) => {
                sentry::integrations::anyhow::capture_anyhow(&e);
                error!(target: SENTRY_CAPTURED_LOG_TARGET, "app error: {:#?}", e);
                internal_server_error().into_response()
            }
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(ServerError {
                    message: "not-found".into(),
                }),
            )
                .into_response(),
            AppError::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(ServerError {
                    message: "forbidden".into(),
                }),
            )
                .into_response(),
            AppError::ForbiddenDetail(msg) => {
                (StatusCode::FORBIDDEN, Json(ServerError { message: msg })).into_response()
            }
            AppError::Conflict => (
                StatusCode::CONFLICT,
                Json(ServerError {
                    message: "conflict".into(),
                }),
            )
                .into_response(),
            AppError::ConflictDetail(msg) => {
                (StatusCode::CONFLICT, Json(ServerError { message: msg })).into_response()
            }
            AppError::BadRequest => (
                StatusCode::BAD_REQUEST,
                Json(ServerError {
                    message: "bad-request".into(),
                }),
            )
                .into_response(),
            AppError::BadRequestDetail(msg) => {
                (StatusCode::BAD_REQUEST, Json(ServerError { message: msg })).into_response()
            }
            AppError::Gone => (
                StatusCode::GONE,
                Json(ServerError {
                    message: "gone".into(),
                }),
            )
                .into_response(),
            AppError::UnprocessableEntity => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ServerError {
                    message: "unprocessable-entity".into(),
                }),
            )
                .into_response(),
            AppError::ContentTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ServerError {
                    message: "content-too-large".into(),
                }),
            )
                .into_response(),
            AppError::PreconditionFailed => (
                StatusCode::PRECONDITION_FAILED,
                Json(ServerError {
                    message: "precondition-failed".into(),
                }),
            )
                .into_response(),
            AppError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ServerError {
                    message: "too-many-requests".into(),
                }),
            )
                .into_response(),
            AppError::TooManyRequestsRetryAfter(secs) => (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, secs.to_string())],
                Json(ServerError {
                    message: "too-many-requests".into(),
                }),
            )
                .into_response(),
        }
    }
}

/// 500 + `internal-error`（`AuthError` / `AppError` で共用）。
pub fn internal_server_error() -> (StatusCode, Json<ServerError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ServerError {
            message: "internal-error".into(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_error_is_captured_with_its_chain() {
        let events = sentry::test::with_captured_events(|| {
            let error = anyhow::anyhow!("root cause").context("outer context");
            let response = AppError::Internal(error).into_response();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        });

        assert_eq!(events.len(), 1);
        let values: Vec<_> = events[0]
            .exception
            .iter()
            .filter_map(|exception| exception.value.as_deref())
            .collect();
        assert!(values.contains(&"root cause"), "{values:?}");
        assert!(values.contains(&"outer context"), "{values:?}");
    }

    #[test]
    fn expected_errors_are_not_captured() {
        let events = sentry::test::with_captured_events(|| {
            for error in [AppError::NotFound, AppError::Forbidden, AppError::Conflict] {
                let _ = error.into_response();
            }
        });

        assert!(events.is_empty());
    }

    #[test]
    fn capture_without_client_is_a_no_op() {
        let response = AppError::Internal(anyhow::anyhow!("boom")).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
