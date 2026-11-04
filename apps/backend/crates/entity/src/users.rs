//! Users entity — schema-first generated output re-exported for stable module path.
pub use super::_generated::users::*;

impl Model {
    pub fn is_bot(&self) -> bool {
        self.account_kind == "bot"
    }
}

/// BOT 専用の到達不能なメール名を判定する。
pub fn is_bot_email(email: &str) -> bool {
    email
        .trim()
        .to_ascii_lowercase()
        .ends_with("@bots.task.invalid")
}
