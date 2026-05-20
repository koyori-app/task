//! OpenAPI コンポーネント登録。

pub mod responses;

use utoipa::openapi::OpenApi;
use utoipa::{PartialSchema, ToSchema};

pub use crate::utils::auth::ServerError;
pub use responses::{
    CredentialErrors, InternalOnlyError, RegisterErrors, ResendVerificationErrors,
    SessionAuthErrors, UnauthorizedErrors, VerifyEmailErrors,
};

/// スキーマのうち、ハンドラだけでは OpenAPI に載らないものを登録する。
pub fn register_schemas(openapi: &mut OpenApi) {
    let components = openapi
        .components
        .get_or_insert_with(utoipa::openapi::Components::new);

    register_schema::<ServerError>(components);
}

fn register_schema<T>(components: &mut utoipa::openapi::Components)
where
    T: ToSchema + PartialSchema,
{
    let name = T::name().into_owned();
    components.schemas.entry(name).or_insert_with(T::schema);
}
