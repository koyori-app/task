//! バイナリ用の薄い glue クレート。実装は各ワークスペースクレートに分割済み（#151 Phase 4）:
//! entity → common → payload → service → job → handler → backend(bin)
//!
//! 統合テスト・export_openapi からの既存パス（`backend::handlers` 等）互換のため
//! 再エクスポートを維持する。

pub mod server;

pub use common::{error, settings};
pub use handler::{AppState, auth_helpers, extractors, handlers, middlewares, openapi, routes};
pub use job as jobs;
pub use service as utils;
