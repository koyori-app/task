//! OpenAPI コンポーネント登録。

pub mod responses;

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::openapi::tag::TagBuilder;
use utoipa::openapi::OpenApi;
use utoipa::{PartialSchema, ToSchema};

pub use crate::error::ServerError;
pub use responses::{
    CredentialErrors, CrudErrors, DriveFolderErrors, InternalOnlyError, PublicShareErrors,
    RegisterErrors, ResendVerificationErrors, SessionAuthErrors, UnauthorizedErrors,
    VerifyEmailErrors,
};

/// スキーマのうち、ハンドラだけでは OpenAPI に載らないものを登録する。
pub fn register_schemas(openapi: &mut OpenApi) {
    let components = openapi
        .components
        .get_or_insert_with(utoipa::openapi::Components::new);

    register_schema::<ServerError>(components);
    register_security_schemes(components);
    register_tags(openapi);
}

/// Scalar のグループ表示順とタグ説明を定義する。
fn register_tags(openapi: &mut OpenApi) {
    openapi.tags = Some(vec![
        TagBuilder::new().name("Auth").description(Some("認証・セッション管理")).build(),
        TagBuilder::new().name("Tenants").description(Some("テナント管理")).build(),
        TagBuilder::new().name("Projects").description(Some("プロジェクト管理")).build(),
        TagBuilder::new().name("Project Members").description(Some("プロジェクトメンバー管理")).build(),
        TagBuilder::new().name("Labels").description(Some("ラベル一覧")).build(),
        TagBuilder::new().name("Personal Tokens").description(Some("パーソナルアクセストークン（PAT）管理")).build(),
        TagBuilder::new().name("Drive Files").description(Some("ドライブ — ファイル操作")).build(),
        TagBuilder::new().name("Drive Folders").description(Some("ドライブ — フォルダ管理")).build(),
        TagBuilder::new().name("Drive Shares").description(Some("ドライブ — フォルダ共有・公開リンク")).build(),
        TagBuilder::new()
            .name("Admin Users")
            .description(Some("管理者 — ユーザー管理"))
            .build(),
        TagBuilder::new()
            .name("Admin Tenants")
            .description(Some("管理者 — テナント閲覧・削除"))
            .build(),
        TagBuilder::new()
            .name("Admin Audit Logs")
            .description(Some("管理者 — 監査ログ一覧"))
            .build(),
        TagBuilder::new()
            .name("Admin System Settings")
            .description(Some("管理者 — システム設定"))
            .build(),
    ]);
}

/// PAT 等で利用する Bearer 認証スキーム（`Authorization: Bearer <token>`）。
fn register_security_schemes(components: &mut utoipa::openapi::Components) {
    components.security_schemes.insert(
        "bearerAuth".to_string(),
        SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format("PAT")
                .build(),
        ),
    );
}

fn register_schema<T>(components: &mut utoipa::openapi::Components)
where
    T: ToSchema + PartialSchema,
{
    let name = T::name().into_owned();
    components.schemas.entry(name).or_insert_with(T::schema);
}
