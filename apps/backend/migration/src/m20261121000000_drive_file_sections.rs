use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// ドライブの Markdown 本文を見出しごとの節に切った索引（docs/dev/features/drive/files.md の「本文の索引」）。
///
/// `tenant_id` / `project_id` は写さない。移動は本文を読み直さずに `drive_files.project_id` を
/// 書き換えるので、検索は `drive_files` と結合して絞る。削除は FK の CASCADE に任せる。
/// 既存のファイルの行は、起動時の backfill（`service::drive_sections::backfill_missing`）が作る。
/// 新しいバックエンドをマイグレーションより先に起動すると SeaORM の `sync()` が表を作るので、
/// `IF NOT EXISTS` で受ける（trigram の索引は `sync()` が作らないので、ここで足す）。
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE IF NOT EXISTS drive_file_sections (
                id            UUID PRIMARY KEY,
                drive_file_id UUID NOT NULL REFERENCES drive_files(id) ON DELETE CASCADE,
                position      INTEGER NOT NULL,
                heading_path  VARCHAR[] NOT NULL,
                body          TEXT NOT NULL,
                CONSTRAINT drive_file_sections_drive_file_id_position_key
                    UNIQUE (drive_file_id, position)
            )
        "#,
            )
            .await?;
        // 本文の部分一致（ILIKE）を引く trigram 索引。拡張は m20261020000000 が入れている
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_drive_file_sections_body_trgm
                     ON drive_file_sections USING GIN (body gin_trgm_ops);",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS drive_file_sections;")
            .await?;
        Ok(())
    }
}
