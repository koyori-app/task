//! データベース制約との照合用ヘルパ。

use sea_orm::DbErr;

/// 一意制約違反（重複キーなど）として扱ってよさそうなエラーか。
pub fn is_postgres_unique_violation(err: &DbErr) -> bool {
    matches!(
        err.sql_err(),
        Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
    )
}