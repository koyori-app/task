//! GitHub 連携の横断クレート。OAuth プロトコル層（PKCE / state / トークン暗号化）と
//! GitHub App API クライアントを提供する。DB スキーマ（entity）には依存しない。

pub mod github_api;
pub mod github_oauth_state;
pub mod github_token_crypto;
pub mod oauth;
