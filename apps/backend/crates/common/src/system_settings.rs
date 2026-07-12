use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, Statement};

/// `system_settings` の singleton 行を保証する（未適用マイグレーション環境でも register が読めるように）。
///
/// SeaORM `sync()` はカラム DEFAULT を付けないため、`(singleton)` のみの INSERT は
/// NOT NULL 制約違反になる。マイグレーションと同じ値を明示する。
pub async fn ensure_system_settings_row(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute_raw(Statement::from_string(
        db.get_database_backend(),
        "INSERT INTO system_settings (
            singleton,
            user_registration_enabled,
            drive_default_quota_mb,
            drive_system_max_quota_mb,
            updated_at
        ) VALUES (true, true, 10240, 102400, now())
        ON CONFLICT (singleton) DO NOTHING"
            .to_string(),
    ))
    .await?;
    Ok(())
}
