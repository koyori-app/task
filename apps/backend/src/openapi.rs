//! OpenAPI 用の共通型。

pub mod responses;

use utoipa::openapi::OpenApi;
use utoipa::{PartialSchema, ToSchema};

pub use responses::{
    CredentialErrors, InternalOnlyError, SessionAuthErrors, UnauthorizedErrors,
};
pub use crate::utils::auth::ServerError;

/// `IntoResponses` 経由で参照されるが path body からは収集されないスキーマを登録する。
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
    components
        .schemas
        .entry(name)
        .or_insert_with(T::schema);
}
