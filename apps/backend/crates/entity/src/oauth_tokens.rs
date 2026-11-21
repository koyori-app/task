//! OAuth tokens entity（MCP の OAuth のアクセストークンとリフレッシュトークン）。
//! 親の許可は `personal_tokens` の行（`oauth_client_id` が NULL でないもの）。
pub use super::_generated::oauth_tokens::*;

/// `kind` 列の値。
pub const KIND_ACCESS: &str = "access";
pub const KIND_REFRESH: &str = "refresh";
