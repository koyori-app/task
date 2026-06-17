//! メールアドレスの正規化（trim + ASCII 小文字）。

/// メールアドレスを登録・照合用に正規化する。
///
/// ドメイン部は RFC 上ケース非依存。ローカル部の Unicode 大文字小文字は変換しない
/// （一般的な Web サービスと同様に全体を `to_ascii_lowercase` する）。
///
/// # Arguments
/// * `email` - 正規化前のメールアドレス（前後に空白があってもよい）
///
/// # Returns
/// * トリム済み・ASCII 小文字化済みの文字列（DB 保存および検索に使う）
pub fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_lowercases() {
        assert_eq!(normalize_email("  User@Example.COM  "), "user@example.com");
    }

    #[test]
    fn leaves_already_normalized_unchanged() {
        assert_eq!(normalize_email("a@b.co"), "a@b.co");
    }
}
