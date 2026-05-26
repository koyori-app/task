//! OpenAPI コンポーネント登録。

pub mod responses;

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::openapi::OpenApi;
use utoipa::{PartialSchema, ToSchema};

pub use crate::error::ServerError;
pub use responses::{
    CredentialErrors, CrudErrors, InternalOnlyError, RegisterErrors, ResendVerificationErrors,
    SessionAuthErrors, UnauthorizedErrors, VerifyEmailErrors,
};

/// スキーマのうち、ハンドラだけでは OpenAPI に載らないものを登録する。
pub fn register_schemas(openapi: &mut OpenApi) {
    let components = openapi
        .components
        .get_or_insert_with(utoipa::openapi::Components::new);

    register_schema::<ServerError>(components);
    register_security_schemes(components);
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
