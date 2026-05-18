/// SMTPクライアントを提供するモジュール

use lettre::message::{Mailbox, Message, MultiPart, SinglePart};
use lettre::{AsyncSmtpTransport, AsyncTransport, transport::smtp::authentication::Credentials};
use lettre::Tokio1Executor;

/// SMTPクライアントの構造体
#[derive(Clone, Debug)]
pub struct SmtpClient {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
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
    /// 
    ///  # Examples
    /// 
    ///  ```
    /// let smtp_client = SmtpClient::new("smtp.example.com", 587, "user", "pass").unwrap();
    /// ```
    pub fn new(
        smtp_server: &str, 
        smtp_port: u16, 
        username: &str, 
        password: &str
    ) -> Result<Self, lettre::transport::smtp::Error> {
        let creds = Credentials::new(username.to_string(), password.to_string());
        
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_server)?
            .port(smtp_port)
            .credentials(creds)
            .build();
            
        Ok(SmtpClient { mailer })
    }

    /// メールを送信する関数
    /// 
    /// # Arguments
    /// * `from` - 送信元のメールアドレス
    /// * `to` - 送信先のメールアドレス
    /// * `subject` - メールの件名
    /// * `body_text` - メールのテキスト形式の本文
    /// * `body_html` - メールのHTML形式の本文（オプション）
    /// 
    /// # Examples
    /// 
    /// ```
    ///client.send_email(
    ///    "sender@example.com",
    ///    "receiver@example.com",
    ///    "Hello from Async Rust!",
    ///    "This is a test email sent asynchronously.",
    ///    Some("<p>This is a <b>test</b> email sent asynchronously.</p>"),
    ///).await?;
    /// ```
    /// 
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

        self.mailer.send(email).await?;
        
        Ok(())
    }
}