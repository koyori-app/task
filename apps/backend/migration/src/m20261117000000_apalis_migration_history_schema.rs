use sea_orm_migration::prelude::*;

/// apalis-postgres 1.0.0-rc.9 以降は、適用の記録を `public._sqlx_migrations` ではなく
/// `apalis._sqlx_migrations` に持つ。1.0.0-rc.8 以前に作った DB は、新しい版を起動する前に、記録の移動と
/// 最初の移行のチェックサムの付け替えが要る（apalis-postgres の README の「Upgrading to 1.0」）。
/// これをバックエンドの起動より前に当てるため、ここで行う。
///
/// 当てる条件は、`public._sqlx_migrations` に apalis の最初の移行（20220530084123）が在ること。
/// 他の物が同じ名前の表を使っていても、この版が無ければ触らない。新しい DB や、既に移し終えた DB では
/// 何もしない。移す前に新しい版のバックエンドが起動して、空の `apalis._sqlx_migrations` を作って
/// しまった DB も受ける（README が「失敗したら空の表を DROP してから移す」とする状態）。
const MOVE_APALIS_MIGRATION_HISTORY: &str = r#"
DO $$
DECLARE
    has_old boolean := false;
BEGIN
    -- 表が無い DB で `EXISTS (SELECT .. FROM public._sqlx_migrations ..)` を計画すると失敗する。
    -- 条件の `AND` は評価の順を保証しないので、表の有無を先に確かめてから行を調べる。
    IF to_regclass('public._sqlx_migrations') IS NOT NULL AND to_regnamespace('apalis') IS NOT NULL THEN
        EXECUTE 'SELECT EXISTS (SELECT 1 FROM public._sqlx_migrations WHERE version = 20220530084123)'
            INTO has_old;
    END IF;
    IF has_old THEN
        IF to_regclass('apalis._sqlx_migrations') IS NOT NULL THEN
            IF (SELECT count(*) FROM apalis._sqlx_migrations) > 0 THEN
                RAISE EXCEPTION 'apalis._sqlx_migrations already has rows; public._sqlx_migrations was not moved';
            END IF;
            DROP TABLE apalis._sqlx_migrations;
        END IF;
        ALTER TABLE public._sqlx_migrations SET SCHEMA apalis;
        -- 最初の移行は、`CREATE SCHEMA` から `CREATE SCHEMA IF NOT EXISTS` に変わった。
        -- 1.0.0-rc.9 以降の移行のチェックサムに付け替える。
        UPDATE apalis._sqlx_migrations
           SET checksum = decode(
               'd0839c6f57a379769dc27ccd581feb3d2709239c8f138e05271c9e3c760c4517a78a4d8912ab3d63b074b28d15ec74e9',
               'hex')
         WHERE version = 20220530084123;
    END IF;
END
$$;
"#;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(MOVE_APALIS_MIGRATION_HISTORY)
            .await?;
        Ok(())
    }

    /// 元へは戻さない。apalis-postgres を 1.0.0-rc.8 以前へ戻す時は、README の逆の手順を手で行う。
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
