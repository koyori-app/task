//! PostgreSQL が認識する timezone 名の判定。

use std::collections::HashSet;
use std::sync::OnceLock;

use sea_orm::{ConnectionTrait, DatabaseBackend, DbErr, Statement};

/// `pg_timezone_names` は集合返却関数で、問い合わせのたびに tz database 全体を読む。
/// tz の追加は PostgreSQL 側の更新を伴うので、process 内に一度だけ読み込んで使い回す。
static TIMEZONE_NAMES: OnceLock<HashSet<String>> = OnceLock::new();

pub async fn exists<C: ConnectionTrait>(db: &C, name: &str) -> Result<bool, DbErr> {
    let names = match TIMEZONE_NAMES.get() {
        Some(names) => names,
        None => {
            let rows = db
                .query_all_raw(Statement::from_string(
                    DatabaseBackend::Postgres,
                    "SELECT name FROM pg_timezone_names",
                ))
                .await?;
            let loaded = rows
                .iter()
                .map(|row| row.try_get::<String>("", "name"))
                .collect::<Result<HashSet<_>, _>>()?;
            TIMEZONE_NAMES.get_or_init(|| loaded)
        }
    };
    Ok(names.contains(name))
}
