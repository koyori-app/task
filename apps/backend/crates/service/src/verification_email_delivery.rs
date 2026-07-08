//! 認証メール本文の組み立てと SMTP 送信。

use crate::settings::Settings;
use crate::{email_verification, smtp::SmtpClient};

pub fn build_verify_url(settings: &Settings, token: &str) -> String {
    let encoded = urlencoding::encode(token);
    format!(
        "{}/verify-email?token={}",
        settings.email_verification_app_url.trim_end_matches('/'),
        encoded
    )
}

pub async fn send_verification_email(
    smtp: &SmtpClient,
    email: &str,
    settings: &Settings,
    token: &str,
) -> Result<(), anyhow::Error> {
    let verify_url = build_verify_url(settings, token);
    let mins = email_verification::TOKEN_TTL_SECS / 60;
    smtp.send_email(
        email,
        "メール認証",
        &format!(
            "以下のリンクからアプリを開き、表示に従ってメールアドレスの確認を完了してください（有効期限は約{mins}分です）。\n{verify_url}",
        ),
        Some(&format!(
            "<p>以下のリンクからアプリを開き、表示に従ってメールアドレスの確認を完了してください（有効期限は約{mins}分です）。</p><p><a href=\"{verify_url}\">{verify_url}</a></p>",
        )),
    )
    .await
    .map_err(|e| anyhow::anyhow!("send verification email: {e}"))?;
    Ok(())
}
