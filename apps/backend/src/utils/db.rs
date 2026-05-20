//! データベース制約との照合用ヘルパ。

use sea_orm::DbErr;

/// 一意制約違反（重複キーなど）として扱ってよさそうなエラーか。
pub fn is_postgres_unique_violation(err: &DbErr) -> bool {
    let s = err.to_string();
    s.contains("23505")
        || {
            let l = s.to_ascii_lowercase();
            l.contains("duplicate") && l.contains("unique")
        }
}
