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
}

/// SmtpClientの実装
impl SmtpClient {
    /// Creates a new SMTP client configured for STARTTLS using the supplied server, port, and credentials.
    ///
    /// The client is configured to use an async STARTTLS SMTP transport; returned errors indicate failures building that transport.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use backend::utils::smtp::SmtpClient;
    ///
    /// let smtp_client = SmtpClient::new("smtp.example.com", 587, "user", "pass").unwrap();
    /// ```
    pub fn new(
        smtp_server: &str,
        smtp_port: u16,
        username: &str,
        password: &str,
    ) -> Result<Self, lettre::transport::smtp::Error> {
        let creds = Credentials::new(username.to_string(), password.to_string());

        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_server)?
            .port(smtp_port)
            .credentials(creds)
            .build();

        Ok(SmtpClient { mailer })
    }

    /// Send an email using the client's configured SMTP transport.
    ///
    /// This builds a `lettre::message::Message` from the provided `from`, `to`, `subject`,
    /// and body parts, and sends it via the client's async SMTP transport. If `body_html` is
    /// `Some`, the message is sent as a multipart/alternative containing both plain text and HTML;
    /// otherwise it is sent as plain text only. The entire send operation is subject to the
    /// module-wide timeout (`SMTP_SEND_TIMEOUT_SECS`) and will fail if that timeout elapses.
    ///
    /// # Errors
    ///
    /// Returns an error if address parsing fails, message construction fails, the SMTP send
    /// fails, or the send operation times out.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use backend::utils::smtp::SmtpClient;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = SmtpClient::new("smtp.example.com", 587, "user", "pass")?;
    /// client.send_email(
    ///     "sender@example.com",
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
        from: &str,
        to: &str,
        subject: &str,
        body_text: &str,
        body_html: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let builder = Message::builder()
            .from(from.parse::<Mailbox>()?)
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
