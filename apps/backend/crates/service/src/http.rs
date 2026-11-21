use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use reqwest::dns::{Name, Resolve, Resolving};
use reqwest::{Client, ClientBuilder};

/// Shared HTTP client for outbound requests (OAuth token exchange, user info, etc.).
pub fn create_http_client() -> Result<Client, reqwest::Error> {
    Client::builder()
        .user_agent("task-oauth-backend")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
}

/// どの名前も解決しないリゾルバー。
struct RefuseDns;

impl Resolve for RefuseDns {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            Err(format!("refusing to resolve an unpinned host: {}", name.as_str()).into())
        })
    }
}

/// 検証済みのアドレスに接続を固定するクライアントの土台（SSRF 対策。外の URL を取りに行く・
/// 送りに行く箇所が共通で使う）。
///
/// `resolve_to_addrs` の固定は、キーが要求のホスト名と一致したときだけ効き、一致しなければ
/// 通常の名前解決に落ちる。落ちた先のアドレスは検証を通っていないので、固定したホスト以外の
/// 解決は必ず失敗させる。プロキシは使わず、リダイレクトは追わず、接続は持ち越さない。
pub fn pinned_client_builder(host: &str, addrs: &[SocketAddr]) -> ClientBuilder {
    Client::builder()
        .no_proxy()
        .dns_resolver(Arc::new(RefuseDns))
        .resolve_to_addrs(host, addrs)
        .redirect(reqwest::redirect::Policy::none())
        .pool_max_idle_per_host(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// 1 回だけ 200 を返すサーバーを立て、ポートを返す。
    async fn serve_once() -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let mut chunk = [0; 1024];
                let count = stream.read(&mut chunk).await.unwrap();
                if count == 0 {
                    return;
                }
                request.extend_from_slice(&chunk[..count]);
            }
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await;
        });
        port
    }

    fn loopback(port: u16) -> Vec<SocketAddr> {
        vec![SocketAddr::from(([127, 0, 0, 1], port))]
    }

    #[tokio::test]
    async fn pinned_host_connects_to_the_given_address() {
        let port = serve_once().await;
        // `pinned.invalid` は名前解決できない名前。固定したアドレスへ繋がる
        let client = pinned_client_builder("pinned.invalid", &loopback(port))
            .build()
            .unwrap();

        let response = client
            .get(format!("http://pinned.invalid:{port}/"))
            .send()
            .await
            .expect("pinned request");

        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    /// 固定のキーと要求のホスト名が食い違ったら、通常の名前解決には落ちずに失敗する。
    /// `localhost` は通常の名前解決なら手元のサーバーへ繋がってしまう名前である。
    #[tokio::test]
    async fn unpinned_host_is_not_resolved() {
        let port = serve_once().await;
        let client = pinned_client_builder("pinned.invalid", &loopback(port))
            .build()
            .unwrap();

        let result = client.get(format!("http://localhost:{port}/")).send().await;

        let error = result.expect_err("固定していないホストへは繋がない");
        assert!(error.is_connect(), "{error:?}");
    }
}
