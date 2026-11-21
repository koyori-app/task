//! MCP のツールから REST API を同じプロセスの中で呼び直す。
//!
//! 認可（scope・テナント束縛・プロジェクト束縛）はハンドラーの中にあるので、`service` を
//! 直接呼ばずにルーターを通す。ツールが増えても認可の規則が 2 箇所に分かれない。

use axum::{
    Router,
    body::Body,
    http::{
        HeaderValue, Method, Request, StatusCode,
        header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
        request::Parts,
    },
};
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::Value;
use tower::ServiceExt;

use crate::extractors::McpAudience;

/// 内向きの応答として読む本文の上限。
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
/// JSON でないエラー本文をツールの結果へ写すときの上限（文字数）。
const MAX_ERROR_CHARS: usize = 2000;

/// ツールの失敗。`status` があれば API が返した拒否、無ければ引数や解決の誤り。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ToolFailure {
    pub(super) status: Option<StatusCode>,
    pub(super) message: String,
}

impl ToolFailure {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
        }
    }

    /// 名前解決の呼び出しが 403 のとき、原因を足す。API の語（`forbidden`）だけでは、
    /// 最後の呼び出しの scope が足りないのか、解決に要る scope が足りないのかが読めない
    pub(super) fn explain_forbidden(mut self, hint: impl FnOnce() -> String) -> Self {
        if self.status == Some(StatusCode::FORBIDDEN) {
            self.message = format!("{} ({})", self.message, hint());
        }
        self
    }

    /// ツールの結果に載せる文面。API の語は言い換えない（例 `403 forbidden`）
    pub(super) fn text(&self) -> String {
        match self.status {
            Some(status) => format!("{} {}", status.as_u16(), self.message),
            None => self.message.clone(),
        }
    }
}

impl From<ToolFailure> for CallToolResult {
    fn from(failure: ToolFailure) -> Self {
        CallToolResult::error(vec![ContentBlock::text(failure.text())])
    }
}

/// API の応答をそのまま返す。項目は削らない。本文の無い成功（204）は `ok`
pub(super) fn ok(value: Value) -> CallToolResult {
    let text = match value {
        Value::Null => "ok".to_string(),
        other => other.to_string(),
    };
    CallToolResult::success(vec![ContentBlock::text(text)])
}

/// 外側の要求の `Authorization` を持って、内向きに REST API を呼ぶ。
pub(super) struct InnerApi {
    router: Router,
    authorization: HeaderValue,
}

impl InnerApi {
    /// `require_bearer` を通った要求には必ず `Authorization` がある。無いのは配線の誤り
    pub(super) fn from_parts(router: &Router, parts: &Parts) -> Result<Self, ToolFailure> {
        let authorization = parts
            .headers
            .get(AUTHORIZATION)
            .cloned()
            .ok_or_else(|| ToolFailure::new("The request has no Authorization header"))?;
        Ok(Self {
            router: router.clone(),
            authorization,
        })
    }

    pub(super) async fn get(
        &self,
        segments: &[&str],
        query: &[(&str, String)],
    ) -> Result<Value, ToolFailure> {
        self.send(Method::GET, segments, query, None).await
    }

    pub(super) async fn post(&self, segments: &[&str], body: &Value) -> Result<Value, ToolFailure> {
        self.send(Method::POST, segments, &[], Some(body)).await
    }

    pub(super) async fn put(&self, segments: &[&str], body: &Value) -> Result<Value, ToolFailure> {
        self.send(Method::PUT, segments, &[], Some(body)).await
    }

    pub(super) async fn delete(&self, segments: &[&str]) -> Result<Value, ToolFailure> {
        self.send(Method::DELETE, segments, &[], None).await
    }

    pub(super) async fn patch(
        &self,
        segments: &[&str],
        body: &Value,
    ) -> Result<Value, ToolFailure> {
        self.send(Method::PATCH, segments, &[], Some(body)).await
    }

    /// 渡すヘッダーは `Authorization`・`Accept`・（本文があれば）`Content-Type` だけ。
    /// `Cookie` と `Origin` は渡さないので、内向きの呼び出しはセッションでは認証されない
    async fn send(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value, ToolFailure> {
        let mut builder = Request::builder()
            .method(method)
            .uri(request_target(segments, query))
            .header(AUTHORIZATION, self.authorization.clone())
            .header(ACCEPT, "application/json")
            // 内向きの呼び出しは MCP の入口宛ての要求の続きなので、OAuth のアクセストークンを通す
            .extension(McpAudience);
        let body = match body {
            Some(value) => {
                builder = builder.header(CONTENT_TYPE, "application/json");
                Body::from(value.to_string())
            }
            None => Body::empty(),
        };
        let request = builder
            .body(body)
            .map_err(|error| ToolFailure::new(format!("Could not build the request: {error}")))?;

        let response = match self.router.clone().oneshot(request).await {
            Ok(response) => response,
            Err(never) => match never {},
        };
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), MAX_RESPONSE_BYTES)
            .await
            .map_err(|_| ToolFailure {
                status: Some(status),
                message: format!("The response is larger than {MAX_RESPONSE_BYTES} bytes"),
            })?;

        if !status.is_success() {
            return Err(ToolFailure {
                status: Some(status),
                message: error_message(&bytes),
            });
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|error| ToolFailure {
            status: Some(status),
            message: format!("The response is not JSON: {error}"),
        })
    }
}

/// パスとクエリを符号化してつなぐ。
fn request_target(segments: &[&str], query: &[(&str, String)]) -> String {
    let mut target = String::new();
    for segment in segments {
        target.push('/');
        target.push_str(&urlencoding::encode(segment));
    }
    for (index, (name, value)) in query.iter().enumerate() {
        target.push(if index == 0 { '?' } else { '&' });
        target.push_str(&urlencoding::encode(name));
        target.push('=');
        target.push_str(&urlencoding::encode(value));
    }
    target
}

/// エラー本文が `{"message": "..."}` ならその文字列、そうでなければ本文の先頭。
fn error_message(bytes: &[u8]) -> String {
    if let Ok(value) = serde_json::from_slice::<Value>(bytes)
        && let Some(message) = value.get("message").and_then(Value::as_str)
    {
        return message.to_string();
    }
    String::from_utf8_lossy(bytes)
        .chars()
        .take(MAX_ERROR_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_target_encodes_segments_and_query() {
        assert_eq!(
            request_target(
                &["v1", "tenants", "a/b", "tasks"],
                &[("q", "status:open a&b".to_string()), ("limit", "5".into())],
            ),
            "/v1/tenants/a%2Fb/tasks?q=status%3Aopen%20a%26b&limit=5"
        );
        assert_eq!(request_target(&["v1", "tenants"], &[]), "/v1/tenants");
    }

    /// 受けた要求のヘッダー名を返すだけのルーター。
    fn header_echo() -> Router {
        Router::new().fallback(|request: Request<Body>| async move {
            let mut names: Vec<String> = request
                .headers()
                .keys()
                .map(|name| name.as_str().to_string())
                .collect();
            names.sort();
            axum::Json(serde_json::json!({
                "headers": names,
                "authorization": request
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|value| value.to_str().ok()),
            }))
        })
    }

    /// 内向きの呼び出しへ外側から写すのは `Authorization` だけ。`Cookie` を写すと、
    /// Bearer を見ない経路が足されたときに PAT に無い権限がセッションで通る。
    /// 既存の認証は Bearer を優先するので、統合テストでは写しても落ちない
    #[tokio::test]
    async fn only_authorization_is_copied_from_the_outer_request() {
        let (parts, ()) = Request::builder()
            .uri("/mcp")
            .header(AUTHORIZATION, "Bearer pat-1")
            .header("cookie", "session=abc")
            .header("origin", "https://evil.example")
            .header("x-forwarded-for", "203.0.113.9")
            .header("mcp-protocol-version", "2025-11-25")
            .body(())
            .unwrap()
            .into_parts();
        let api = InnerApi::from_parts(&header_echo(), &parts).unwrap();

        let seen = api.get(&["v1", "tenants"], &[]).await.unwrap();
        assert_eq!(
            seen["headers"],
            serde_json::json!(["accept", "authorization"])
        );
        assert_eq!(seen["authorization"], "Bearer pat-1");

        let seen = api
            .post(&["v1", "tenants"], &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(
            seen["headers"],
            serde_json::json!(["accept", "authorization", "content-type"])
        );

        let seen = api
            .patch(&["v1", "tenants"], &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(
            seen["headers"],
            serde_json::json!(["accept", "authorization", "content-type"])
        );

        let seen = api.delete(&["v1", "tenants"]).await.unwrap();
        assert_eq!(
            seen["headers"],
            serde_json::json!(["accept", "authorization"])
        );
        assert_eq!(seen["authorization"], "Bearer pat-1");
    }

    #[test]
    fn request_without_authorization_is_refused() {
        let (parts, ()) = Request::builder()
            .uri("/mcp")
            .header("cookie", "session=abc")
            .body(())
            .unwrap()
            .into_parts();
        assert!(InnerApi::from_parts(&header_echo(), &parts).is_err());
    }

    #[test]
    fn error_message_prefers_the_api_message() {
        assert_eq!(error_message(br#"{"message":"forbidden"}"#), "forbidden");
        assert_eq!(error_message(b"plain text"), "plain text");
        let long = "x".repeat(MAX_ERROR_CHARS + 10);
        assert_eq!(error_message(long.as_bytes()).len(), MAX_ERROR_CHARS);
    }

    #[test]
    fn failure_text_keeps_the_api_wording() {
        let denied = ToolFailure {
            status: Some(StatusCode::FORBIDDEN),
            message: "forbidden".into(),
        };
        assert_eq!(denied.text(), "403 forbidden");
        assert_eq!(
            denied
                .clone()
                .explain_forbidden(|| "needs read:project".into())
                .text(),
            "403 forbidden (needs read:project)"
        );
        let missing = ToolFailure {
            status: Some(StatusCode::NOT_FOUND),
            message: "not-found".into(),
        };
        assert_eq!(
            missing.clone().explain_forbidden(|| "unused".into()),
            missing
        );
        assert_eq!(ToolFailure::new("bad input").text(), "bad input");
    }
}
