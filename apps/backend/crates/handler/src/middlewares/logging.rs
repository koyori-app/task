use std::borrow::Cow;

use axum::{body::Body, http::Request, middleware::Next, response::IntoResponse};

pub async fn logging_middleware(req: Request<Body>, next: Next) -> impl IntoResponse {
    // Log path only — never the query string (password-reset verify carries ?token=).
    let path = redact_path(req.uri().path()).into_owned();
    let method = req.method().clone();

    tracing::info!(%method, %path, "received request");

    next.run(req).await
}

const PUBLIC_SHARE_PREFIX: &str = "/v1/drive/share/";

/// 公開共有のパスはトークン自体が資格なので、ログには固定文字に置き換えて出す。
fn redact_path(path: &str) -> Cow<'_, str> {
    match path.strip_prefix(PUBLIC_SHARE_PREFIX) {
        Some(rest) if !rest.is_empty() => {
            let tail = rest.find('/').map_or("", |i| &rest[i..]);
            Cow::Owned(format!("{PUBLIC_SHARE_PREFIX}{{token}}{tail}"))
        }
        _ => Cow::Borrowed(path),
    }
}

#[cfg(test)]
mod tests {
    use super::redact_path;

    #[test]
    fn redacts_share_token_in_folder_path() {
        assert_eq!(
            redact_path("/v1/drive/share/tokSECRET123"),
            "/v1/drive/share/{token}"
        );
    }

    #[test]
    fn redacts_share_token_and_keeps_the_rest() {
        assert_eq!(
            redact_path("/v1/drive/share/tokSECRET123/files"),
            "/v1/drive/share/{token}/files"
        );
    }

    #[test]
    fn leaves_unrelated_paths_untouched() {
        let path = "/v1/drive/files/0b6f3f9e-0000-4000-8000-000000000000/content";
        assert_eq!(redact_path(path), path);
    }
}
