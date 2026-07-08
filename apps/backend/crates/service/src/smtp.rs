//! SMTPクライアントを提供するモジュール

use lettre::Tokio1Executor;
use lettre::message::{Mailbox, Message, MultiPart, SinglePart};
use lettre::{AsyncSmtpTransport, AsyncTransport, transport::smtp::authentication::Credentials};
use std::time::Duration;
use tokio::time::timeout;

/// SMTP送信タイムアウト（秒）
const SMTP_SEND_TIMEOUT_SECS: u64 = 30;

/// SMTPクライアントの構造体
#[derive(Clone)]
pub struct SmtpClient {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    smtp_from: String,
}

/// SmtpClientの実装
impl SmtpClient {
    /// 新しいSMTPクライアントを作成する関数
    ///
    /// # Arguments
    /// * `smtp_server` - SMTPサーバーのアドレス
    /// * `smtp_port` - SMTPサーバーのポート番号
    /// * `username` - SMTPサーバーの認証に使用するユーザー名
    /// * `password` - SMTPサーバーの認証に使用するパスワード
    /// * `smtp_from` - メールの送信元アドレス（設定の `smtp_from` と同じ値を渡す想定）
    ///
    ///  # Examples
    ///
    ///  ```no_run
    /// use backend::utils::smtp::SmtpClient;
    ///
    /// let smtp_client = SmtpClient::new(
    ///     "smtp.example.com",
    ///     587,
    ///     "user",
    ///     "pass",
    ///     "noreply@example.com",
    /// )
    /// .unwrap();
    /// ```
    pub fn new(
        smtp_server: &str,
        smtp_port: u16,
        username: &str,
        password: &str,
        smtp_from: &str,
    ) -> Result<Self, lettre::transport::smtp::Error> {
        let creds = Credentials::new(username.to_string(), password.to_string());

        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_server)?
            .port(smtp_port)
            .credentials(creds)
            .build();

        Ok(SmtpClient {
            mailer,
            smtp_from: smtp_from.to_string(),
        })
    }

    /// メールを送信する関数
    ///
    /// # Arguments
    /// * `to` - 送信先のメールアドレス
    /// * `subject` - メールの件名
    /// * `body_text` - メールのテキスト形式の本文
    /// * `body_html` - メールのHTML形式の本文（オプション）
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use backend::utils::smtp::SmtpClient;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client =
    ///     SmtpClient::new("smtp.example.com", 587, "user", "pass", "sender@example.com")?;
    /// client.send_email(
    ///     "receiver@example.com",
    ///     "Hello from Async Rust!",
    ///     "This is a test email sent asynchronously.",
    ///     Some("<p>This is a <b>test</b> email sent asynchronously.</p>"),
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_email(
        &self,
        to: &str,
        subject: &str,
        body_text: &str,
        body_html: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let builder = Message::builder()
            .from(self.smtp_from.parse::<Mailbox>()?)
            .to(to.parse::<Mailbox>()?)
            .subject(subject);

        let email = match body_html {
            Some(html) => builder.multipart(
                MultiPart::alternative()
                    .singlepart(SinglePart::plain(body_text.to_string()))
                    .singlepart(SinglePart::html(html.to_string())),
            )?,
            None => builder.singlepart(SinglePart::plain(body_text.to_string()))?,
        };

        // 送信処理にタイムアウトを30秒に設定
        timeout(
            Duration::from_secs(SMTP_SEND_TIMEOUT_SECS),
            self.mailer.send(email),
        )
        .await
        .map_err(|_| "SMTP send timeout")??;

        Ok(())
    }
}
