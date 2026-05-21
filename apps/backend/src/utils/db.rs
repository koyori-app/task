//! データベースまわりの共通ヘルパ。
//!
//! トランザクションは SeaORM の [`TransactionTrait::transaction`] を使う。
//! 公式: <https://www.sea-ql.org/SeaORM/docs/advanced-query/transaction/>

use std::future::Future;
use std::pin::Pin;

use sea_orm::{DatabaseConnection, DbErr, TransactionError, TransactionTrait};

/// 一意制約違反（重複キーなど）として扱ってよさそうなエラーか。
pub fn is_postgres_unique_violation(err: &DbErr) -> bool {
    matches!(
        err.sql_err(),
        Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
    )
}

/// [`DatabaseConnection::transaction`] の薄いラッパ。
///
/// クロージャはドキュメントどおり `Box::pin(async move { ... })` を返す。
/// `Ok` で commit、`Err` で rollback する。
pub async fn with_transaction<T, E, F>(db: &DatabaseConnection, f: F) -> Result<T, E>
where
    F: for<'c> FnOnce(
            &'c sea_orm::DatabaseTransaction,
        ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
        + Send,
    T: Send,
    E: std::fmt::Display + std::fmt::Debug + Send + From<DbErr>,
{
    db.transaction(f).await.map_err(|e| match e {
        TransactionError::Connection(err) => E::from(err),
        TransactionError::Transaction(err) => err,
    })
}
