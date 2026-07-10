//! 登録済みメールアドレスへの新規登録試行通知メールの組み立てと SMTP 送信。
//!
//! #26: メールアドレス列挙対策として新規登録 API は既存メールでも未使用時と
//! 同一のレスポンスを返す。既存アカウント宛にはこの通知メールで代替する。

use crate::settings::Settings;
use crate::smtp::SmtpClient;

pub async fn send_already_registered_email(
    smtp: &SmtpClient,
    email: &str,
    settings: &Settings,
) -> Result<(), anyhow::Error> {
    let signin_url = format!(
        "{}/signin",
        settings.email_verification_app_url.trim_end_matches('/')
    );
    let subject = "アカウント登録のお知らせ";
    let text = format!(
        "このメールアドレスで新規アカウント登録が試みられましたが、\
         既にこのメールアドレスでアカウントが登録されています。\n\n\
         ご本人であればログインしてください。\n{signin_url}\n\n\
         パスワードを忘れた場合はログイン画面からパスワードの再設定を行ってください。\n\n\
         このメールに心当たりのない場合は無視してください。"
    );
    let html = format!(
        "<p>このメールアドレスで新規アカウント登録が試みられましたが、\
         既にこのメールアドレスでアカウントが登録されています。</p>\
         <p>ご本人であれば<a href=\"{signin_url}\">こちら</a>からログインしてください。</p>\
         <p>パスワードを忘れた場合はログイン画面からパスワードの再設定を行ってください。</p>\
         <p>このメールに心当たりのない場合は無視してください。</p>"
    );
    smtp.send_email(email, subject, &text, Some(&html))
        .await
        .map_err(|e| anyhow::anyhow!("send already registered email: {e}"))
}
