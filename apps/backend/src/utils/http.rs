use reqwest::Client;

/// Shared HTTP client for outbound requests (OAuth token exchange, user info, etc.).
pub fn create_http_client() -> Result<Client, reqwest::Error> {
    Client::builder()
        .user_agent("task-oauth-backend")
        .build()
}
