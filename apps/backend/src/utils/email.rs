//! メールアドレスの正規化（trim + ASCII 小文字）。

/// 前後空白を除去し、ASCII 小文字に揃える。
///
/// ドメイン部は RFC 上ケース非依存。ローカル部の Unicode 大文字小文字は変換しない
/// （一般的な Web サービスと同様に全体を小文字化する）。
pub fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_lowercases() {
        assert_eq!(
            normalize_email("  User@Example.COM  "),
            "user@example.com"
        );
    }

    #[test]
    fn leaves_already_normalized_unchanged() {
        assert_eq!(normalize_email("a@b.co"), "a@b.co");
    }
}
