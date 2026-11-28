//! `api_url` が task の API を指しているかを、秘密を何も送らずに確かめる。
//!
//! `task init` と `task config set api_url` がここを通る。認証なしで
//! `GET <候補>/v1/personal_tokens/me` を送り、401 と JSON の `message` が返れば API とみなす。
//! トークンは config のものも `TASK_TOKEN` も読まない。

use std::error::Error as _;
use std::time::Duration;

use reqwest::{StatusCode, Url};

use crate::error::{CliError, Result};

/// 1 候補を確かめる時間の上限。
const TIMEOUT: Duration = Duration::from_secs(10);

/// 試した 1 候補と、その結果。
pub struct ProbeAttempt {
    pub url: String,
    pub outcome: String,
}

/// 確かめる候補。入力そのまま、入力 + `/api` の順。入力が既に `/api` で終わるなら 1 つだけ。
///
/// 前後の空白と末尾の `/` は落とす。`http` / `https` の URL として読めない入力と、
/// query・fragment の付いた入力は終了コード 2。
///
/// ユーザー名やパスワードの付いた URL（`https://user:pass@host`）も終了コード 2。reqwest は
/// URL の userinfo を `Authorization: Basic` にして送るので、通すと認証なしの確認にならない。
/// `@` を含む入力は、エラーの文にも出さない。
pub fn candidates(input: &str) -> Result<Vec<String>> {
    let invalid = || {
        if input.contains('@') {
            CliError::validation("Invalid api_url: expected an http(s) URL")
        } else {
            CliError::validation(format!("Invalid api_url {input}: expected an http(s) URL"))
        }
    };
    let trimmed = input.trim().trim_end_matches('/');
    let url = Url::parse(trimmed).map_err(|_| invalid())?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(CliError::validation(
            "Invalid api_url: a URL with a username or password is not supported",
        ));
    }
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none_or(str::is_empty)
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let mut candidates = vec![trimmed.to_string()];
    let ends_with_api = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        == Some("api");
    if !ends_with_api {
        candidates.push(format!("{trimmed}/api"));
    }
    Ok(candidates)
}

fn probe_url(candidate: &str) -> String {
    format!("{candidate}/v1/personal_tokens/me")
}

/// 原因の連なりを 1 行にする（reqwest の文だけでは「送れなかった」としか分からない）。
fn describe(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        return "timed out".to_string();
    }
    let mut parts = Vec::new();
    let mut source = err.source();
    while let Some(cause) = source {
        parts.push(cause.to_string());
        source = cause.source();
    }
    match parts.last() {
        Some(root) => root.clone(),
        None => "request failed".to_string(),
    }
}

/// 認証なしの 401 と JSON の `message` が返るか。
async fn attempt(http: &reqwest::Client, candidate: &str) -> std::result::Result<(), String> {
    let response = http
        .get(probe_url(candidate))
        .send()
        .await
        .map_err(|err| describe(&err))?;
    let status = response.status();
    if status.is_redirection() {
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("(no Location)");
        return Err(format!("{} -> {location}", status.as_u16()));
    }
    if status != StatusCode::UNAUTHORIZED {
        return Err(status.as_u16().to_string());
    }
    let body = response.bytes().await.map_err(|err| describe(&err))?;
    let is_api = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .is_some_and(|value| value.get("message").is_some_and(|m| m.is_string()));
    if is_api {
        Ok(())
    } else {
        Err("401 without a task API error body".to_string())
    }
}

/// 確かめられた `api_url` を返す（入力に `/api` を足した形になることがある）。
///
/// どの候補も当たらなければ終了コード 2。通信の失敗も「外れ」の 1 つとして次の候補へ進む。
/// `hint` は、確かめられなかったときの文の末尾に足す案内（呼び出し側ごとに違う）。
pub async fn probe_api_url(input: &str, hint: &str) -> Result<String> {
    let candidates = candidates(input)?;
    // `Authorization` を付けない専用のクライアント。リダイレクトを辿ると、確かめた URL と
    // 実際に答えた先が食い違う
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .default_headers(
            [(
                reqwest::header::ACCEPT,
                reqwest::header::HeaderValue::from_static("application/json"),
            )]
            .into_iter()
            .collect(),
        )
        .build()
        .map_err(CliError::from)?;
    let mut attempts = Vec::new();
    for candidate in candidates {
        match attempt(&http, &candidate).await {
            Ok(()) => return Ok(candidate),
            Err(outcome) => attempts.push(ProbeAttempt {
                url: probe_url(&candidate),
                outcome,
            }),
        }
    }
    let mut message = format!("Could not confirm a task API at {}. Tried:", input.trim());
    for attempt in &attempts {
        message.push_str(&format!("\n  {}: {}", attempt.url, attempt.outcome));
    }
    message.push('\n');
    message.push_str(hint);
    Err(CliError::validation(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tries_the_input_and_then_the_api_path_below_it() {
        let both = |base: &str| vec![base.to_string(), format!("{base}/api")];
        assert_eq!(candidates("https://h").unwrap(), both("https://h"));
        assert_eq!(candidates("https://h/").unwrap(), both("https://h"));
        assert_eq!(candidates("  https://h//\n").unwrap(), both("https://h"));
        assert_eq!(
            candidates("https://h/task").unwrap(),
            both("https://h/task")
        );
        assert_eq!(
            candidates("http://127.0.0.1:8080").unwrap(),
            both("http://127.0.0.1:8080")
        );
    }

    #[test]
    fn an_input_that_already_ends_with_api_is_the_only_candidate() {
        assert_eq!(candidates("https://h/api").unwrap(), ["https://h/api"]);
        assert_eq!(candidates("https://h/api/").unwrap(), ["https://h/api"]);
        assert_eq!(
            candidates("https://h/task/api").unwrap(),
            ["https://h/task/api"]
        );
    }

    #[test]
    fn rejects_what_is_not_a_plain_http_url() {
        for input in [
            "h",
            "ftp://h",
            "https://h?x=1",
            "https://h#x",
            "",
            "  ",
            "https://",
        ] {
            let err = candidates(input).unwrap_err();
            assert_eq!(err.exit_code, 2, "{input:?}");
            assert!(err.message.contains("expected an http(s) URL"), "{input:?}");
        }
    }

    #[test]
    fn rejects_a_url_with_credentials_without_repeating_them() {
        for input in [
            "https://alice:secret@h/api",
            "https://alice@h",
            "https://:secret@h",
            "http://alice:secret@127.0.0.1:8080/",
            // http(s) として読めない入力も、`@` があれば文に出さない
            "alice:secret@h",
            "ftp://alice:secret@h",
        ] {
            let err = candidates(input).unwrap_err();
            assert_eq!(err.exit_code, 2, "{input:?}");
            assert!(err.message.starts_with("Invalid api_url: "), "{input:?}");
            assert!(!err.message.contains("alice"), "{}", err.message);
            assert!(!err.message.contains("secret"), "{}", err.message);
        }
        // `@` が path にあるだけなら userinfo ではない
        assert_eq!(
            candidates("https://h/@x/api").unwrap(),
            ["https://h/@x/api"]
        );
    }
}
