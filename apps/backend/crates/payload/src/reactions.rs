//! コメントとタスク本体の絵文字リアクション。

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::ValidationError;

use crate::task_comments::CommentUser;

/// 絵文字の値の上限（バイト数）。
pub const MAX_EMOJI_BYTES: usize = 64;

/// 絵文字の値の形だけを検める（本当に絵文字かは見ない）。
///
/// 値は URL のパスから来るので、`Validate` の derive ではなく関数で持つ。
/// 結合子（U+200D）や異体字セレクタは制御文字ではないので、複合の絵文字も通る。
pub fn validate_emoji(emoji: &str) -> Result<(), ValidationError> {
    if emoji.is_empty()
        || emoji.len() > MAX_EMOJI_BYTES
        || emoji.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(ValidationError::new("emoji"));
    }
    Ok(())
}

/// 1 つの絵文字の集計。
#[derive(Serialize, Deserialize, ToSchema)]
pub struct ReactionSummary {
    pub emoji: String,
    pub count: u64,
    /// 呼び出した本人が押しているか
    pub reacted: bool,
    /// 押した人の先頭 5 人まで（本人がいれば先頭、残りは押した順）
    pub users: Vec<CommentUser>,
}

impl ReactionSummary {
    /// 誰も押していない（または見せない）絵文字の集計。
    pub fn empty(emoji: String) -> Self {
        Self {
            emoji,
            count: 0,
            reacted: false,
            users: Vec::new(),
        }
    }
}

/// 1 つの絵文字を押した全員（押した順、上限なし）。
#[derive(Serialize, Deserialize, ToSchema)]
pub struct ReactionUsersResponse {
    pub emoji: String,
    pub count: u64,
    pub users: Vec<CommentUser>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_emoji_and_any_non_blank_value_up_to_the_byte_limit() {
        for ok in [
            "👍",
            "👍🏽",
            "❤️",
            "👨\u{200d}👩\u{200d}👧\u{200d}👦",
            ":custom:",
            &"a".repeat(MAX_EMOJI_BYTES),
        ] {
            assert!(validate_emoji(ok).is_ok(), "{ok:?}");
        }
    }

    #[test]
    fn rejects_empty_too_long_whitespace_and_control_characters() {
        for ng in [
            "",
            &"a".repeat(MAX_EMOJI_BYTES + 1),
            " ",
            "👍 ",
            "\u{3000}👍",
            "a\tb",
            "a\nb",
            "\u{7f}",
            "\u{0}",
        ] {
            assert!(validate_emoji(ng).is_err(), "{ng:?}");
        }
    }
}
