//! Sentry の初期化と、tracing・HTTP レイヤーへの組み込み。

use std::borrow::Cow;
use std::sync::{Arc, LazyLock};

use axum::{Router, body::Body, http::Request, middleware};
use common::error::SENTRY_CAPTURED_LOG_TARGET;
use common::settings::Settings;
use handler::middlewares::sentry_context::scrub_request;
use regex::Regex;
use sentry::integrations::tower::{NewSentryLayer, SentryHttpLayer};
use sentry::integrations::tracing::{EventFilter, SentryLayer, default_event_filter};
use sentry::protocol::{Breadcrumb, Context, Event, Value};
use tower::ServiceBuilder;
use tracing::{Metadata, Subscriber};
use tracing_subscriber::registry::LookupSpan;

/// apalis の `.enable_tracing()` が出すイベントの target の接頭辞
const APALIS_TRACE_TARGET_PREFIX: &str = "apalis::layers::tracing";

const FILTERED: &str = "[Filtered]";

static EMAIL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap());

/// メールのリンクや OAuth のコールバックに載る資格（`?token=...` `&code=...` など）
static CREDENTIAL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)([?&](?:code|state)|\b(?:[a-z]+_)?token|\bpassword|\bsecret)=[^&\s"']+"#)
        .unwrap()
});

/// DSN が設定されているときの Sentry の設定。未設定・空なら `None`（初期化せず、何も送らない）。
pub fn client_options(
    settings: &Settings,
) -> Result<Option<sentry::ClientOptions>, sentry::types::ParseDsnError> {
    let Some(dsn) = settings
        .sentry_dsn
        .as_deref()
        .map(str::trim)
        .filter(|dsn| !dsn.is_empty())
    else {
        return Ok(None);
    };
    Ok(Some(sentry::ClientOptions {
        dsn: Some(dsn.parse()?),
        release: sentry::release_name!(),
        environment: settings
            .rust_env
            .as_deref()
            .map(str::trim)
            .filter(|env| !env.is_empty())
            .map(|env| Cow::Owned(env.to_owned())),
        traces_sample_rate: settings.traces_sample_rate(),
        // 本番既定は false。必要時のみ環境変数で opt-in 推奨
        send_default_pii: false,
        before_send: Some(Arc::new(|mut event| {
            scrub_event(&mut event);
            Some(event)
        })),
        before_breadcrumb: Some(Arc::new(|mut breadcrumb| {
            scrub_breadcrumb(&mut breadcrumb);
            Some(breadcrumb)
        })),
        ..Default::default()
    }))
}

/// tracing の error をイベントに、warn / info をブレッドクラムにする。
/// Sentry を初期化していなければ、送り先が無いので何もしない。
pub fn tracing_layer<S>() -> SentryLayer<S>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    sentry::integrations::tracing::layer().event_filter(event_filter)
}

fn event_filter(metadata: &Metadata<'_>) -> EventFilter {
    let target = metadata.target();
    // AppError は capture_anyhow で送信済み。ジョブの失敗は試行ごとに出るうえエラー文に
    // 宛先が混ざるので送らず、最終的な失敗を job::failure_report が送る
    if target == SENTRY_CAPTURED_LOG_TARGET || target.starts_with(APALIS_TRACE_TARGET_PREFIX) {
        return EventFilter::Ignore;
    }
    default_event_filter(metadata)
}

/// リクエストごとの Hub・トランザクションを張る。外側から順に
/// `NewSentryLayer` → `SentryHttpLayer` → `scrub_request`。
pub fn with_request_tracking(router: Router) -> Router {
    router.layer(
        ServiceBuilder::new()
            .layer(NewSentryLayer::<Request<Body>>::new_from_top())
            .layer(SentryHttpLayer::new().enable_transaction())
            .layer(middleware::from_fn(scrub_request)),
    )
}

fn scrub_text(text: &str) -> Option<String> {
    let scrubbed = EMAIL_PATTERN.replace_all(text, FILTERED);
    let scrubbed = CREDENTIAL_PATTERN.replace_all(&scrubbed, format!("$1={FILTERED}"));
    (scrubbed != text).then(|| scrubbed.into_owned())
}

fn scrub_string(text: &mut String) {
    if let Some(scrubbed) = scrub_text(text) {
        *text = scrubbed;
    }
}

fn scrub_value(value: &mut Value) {
    match value {
        Value::String(text) => scrub_string(text),
        Value::Array(items) => items.iter_mut().for_each(scrub_value),
        Value::Object(map) => map.values_mut().for_each(scrub_value),
        _ => {}
    }
}

fn scrub_breadcrumb(breadcrumb: &mut Breadcrumb) {
    if let Some(message) = breadcrumb.message.as_mut() {
        scrub_string(message);
    }
    breadcrumb.data.values_mut().for_each(scrub_value);
}

/// ログのメッセージやフィールドに混ざったメールアドレスと資格を伏せる。
/// エラー文は SMTP の応答や外部 API の本文を含むことがあり、書く側では防ぎ切れない。
fn scrub_event(event: &mut Event<'static>) {
    if let Some(message) = event.message.as_mut() {
        scrub_string(message);
    }
    if let Some(logentry) = event.logentry.as_mut() {
        scrub_string(&mut logentry.message);
        logentry.params.iter_mut().for_each(scrub_value);
    }
    for exception in event.exception.values.iter_mut() {
        if let Some(value) = exception.value.as_mut() {
            scrub_string(value);
        }
    }
    event.tags.values_mut().for_each(scrub_string);
    event.extra.values_mut().for_each(scrub_value);
    for context in event.contexts.values_mut() {
        if let Context::Other(fields) = context {
            fields.values_mut().for_each(scrub_value);
        }
    }
    event
        .breadcrumbs
        .values
        .iter_mut()
        .for_each(scrub_breadcrumb);
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, http::StatusCode, routing::get};
    use common::error::AppError;
    use sentry::Envelope;
    use sentry::protocol::{EnvelopeItem, Map};
    use tower::ServiceExt;
    use tracing_subscriber::layer::SubscriberExt;

    fn settings(dsn: Option<&str>) -> Settings {
        let mut settings: Settings = serde_json::from_value(serde_json::json!({
            "database_url": "",
            "redis_url": "",
            "smtp_host": "",
            "smtp_port": 0,
            "smtp_username": "",
            "smtp_password": "",
            "smtp_from": "",
            "email_verification_app_url": "http://localhost:3000",
            "personal_token_secret": "",
            "recovery_code_secret": "",
            "totp_encryption_key": "",
        }))
        .unwrap();
        settings.sentry_dsn = dsn.map(str::to_owned);
        settings
    }

    /// tracing の Sentry レイヤーを積んだ購読者の下で `run` を実行し、送られたものを返す
    fn captured_events(run: impl FnOnce()) -> Vec<Event<'static>> {
        sentry::test::with_captured_events(|| {
            let subscriber = tracing_subscriber::registry().with(tracing_layer());
            tracing::subscriber::with_default(subscriber, run);
        })
    }

    #[test]
    fn unset_or_empty_dsn_does_not_initialize() {
        assert!(client_options(&settings(None)).unwrap().is_none());
        // docker-compose の `${SENTRY_DSN:-}` は未設定を空文字で渡す
        assert!(client_options(&settings(Some(""))).unwrap().is_none());
    }

    #[test]
    fn invalid_dsn_is_an_error() {
        assert!(client_options(&settings(Some("not-a-dsn"))).is_err());
    }

    #[test]
    fn options_carry_environment_and_sample_rate() {
        let mut settings = settings(Some("https://public@sentry.example.com/1"));
        settings.rust_env = Some("production".into());
        settings.sentry_traces_sample_rate = Some(0.5);

        let options = client_options(&settings).unwrap().unwrap();

        assert_eq!(options.environment.as_deref(), Some("production"));
        assert_eq!(options.traces_sample_rate, 0.5);
        assert!(!options.send_default_pii);
    }

    #[test]
    fn layer_without_client_is_a_no_op() {
        assert!(sentry::Hub::current().client().is_none());
        let subscriber = tracing_subscriber::registry().with(tracing_layer());
        tracing::subscriber::with_default(subscriber, || {
            tracing::error!("not sent anywhere");
            tracing::info_span!("span").in_scope(|| tracing::warn!("not sent anywhere"));
        });
    }

    #[test]
    fn errors_become_events_and_lower_levels_become_breadcrumbs() {
        let events = captured_events(|| {
            tracing::info!("info crumb");
            tracing::warn!("warn crumb");
            tracing::debug!("debug ignored");
            tracing::error!("error event");
        });

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].message.as_deref(), Some("error event"));
        let crumbs: Vec<_> = events[0]
            .breadcrumbs
            .iter()
            .filter_map(|crumb| crumb.message.as_deref())
            .collect();
        assert_eq!(crumbs, ["info crumb", "warn crumb"]);
    }

    #[test]
    fn app_error_is_sent_once_through_the_layer() {
        use axum::response::IntoResponse;

        let events = captured_events(|| {
            let _ = AppError::Internal(anyhow::anyhow!("boom")).into_response();
        });

        assert_eq!(events.len(), 1);
        assert!(events[0].message.is_none());
        assert_eq!(events[0].exception.len(), 1);
    }

    #[test]
    fn apalis_trace_events_are_not_sent() {
        let events = captured_events(|| {
            tracing::error!(
                target: "apalis::layers::tracing::on_failure",
                result = "550 <victim@example.com>",
                "task.failed"
            );
        });

        assert!(events.is_empty());
    }

    #[test]
    fn scrub_text_hides_emails_and_credentials() {
        let scrubbed = scrub_text(
            "550 <victim@example.com> https://a.test/cb?code=abc&state=def token=ghi \
             access_token=jkl password=mno",
        )
        .unwrap();

        for secret in ["victim@", "abc", "def", "ghi", "jkl", "mno"] {
            assert!(!scrubbed.contains(secret), "{scrubbed}");
        }
        assert_eq!(scrub_text("status code 500"), None);
    }

    #[test]
    fn scrub_event_reaches_every_text_field() {
        let secret = "victim@example.com";
        let mut event = Event {
            message: Some(secret.into()),
            ..Default::default()
        };
        event.exception.values.push(sentry::protocol::Exception {
            value: Some(secret.into()),
            ..Default::default()
        });
        event.tags.insert("tag".into(), secret.into());
        event.extra.insert("extra".into(), secret.into());
        event.contexts.insert(
            "Rust Tracing Fields".into(),
            Context::Other(Map::from_iter([(
                "nested".to_owned(),
                serde_json::json!({ "list": [secret] }),
            )])),
        );
        event.breadcrumbs.values.push(Breadcrumb {
            message: Some(secret.into()),
            data: Map::from_iter([("email".to_owned(), secret.into())]),
            ..Default::default()
        });

        scrub_event(&mut event);

        let rendered = serde_json::to_string(&event).unwrap();
        assert!(!rendered.contains(secret), "{rendered}");
    }

    async fn failing_handler() -> Result<(), AppError> {
        handler::middlewares::sentry_context::set_user(uuid::Uuid::nil());
        handler::middlewares::sentry_context::set_tenant(uuid::Uuid::max());
        Err(AppError::Internal(anyhow::anyhow!("boom")))
    }

    fn request_envelopes() -> Vec<Envelope> {
        sentry::test::with_captured_envelopes_options(
            || {
                let router = with_request_tracking(
                    Router::new().route("/v1/share/{token}", get(failing_handler)),
                );
                let request = Request::builder()
                    .uri("/v1/share/pathSECRET?token=querySECRET")
                    .header("host", "api.example.com")
                    .header("cookie", "session=cookieSECRET")
                    .header("x-github-select-token", "headerSECRET")
                    .body(Body::empty())
                    .unwrap();
                let response = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(router.oneshot(request))
                    .unwrap();
                assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
            },
            sentry::ClientOptions {
                traces_sample_rate: 1.0,
                ..Default::default()
            },
        )
    }

    #[test]
    fn request_is_tracked_without_leaking_credentials() {
        let envelopes = request_envelopes();

        let mut event = None;
        let mut transaction = None;
        for envelope in &envelopes {
            for item in envelope.items() {
                match item {
                    EnvelopeItem::Event(found) => event = Some(found.clone()),
                    EnvelopeItem::Transaction(found) => transaction = Some(found.clone()),
                    _ => {}
                }
            }
        }
        let event = event.expect("error event");
        let transaction = transaction.expect("transaction");

        assert_eq!(
            transaction.name.as_deref(),
            Some("GET /v1/share/{token}"),
            "matched path should name the transaction"
        );
        for rendered in [
            serde_json::to_string(&event).unwrap(),
            serde_json::to_string(&transaction).unwrap(),
        ] {
            assert!(!rendered.contains("SECRET"), "{rendered}");
        }

        let user = event.user.as_ref().expect("user");
        assert_eq!(
            user.id.as_deref(),
            Some(uuid::Uuid::nil().to_string().as_str())
        );
        assert_eq!(user.email, None);
        assert_eq!(
            event.tags.get("tenant_id").map(String::as_str),
            Some(uuid::Uuid::max().to_string().as_str())
        );
    }

    #[test]
    fn scope_does_not_leak_between_requests() {
        let envelopes = sentry::test::with_captured_envelopes(|| {
            let router =
                with_request_tracking(Router::new().route("/authed", get(failing_handler)).route(
                    "/anonymous",
                    get(|| async { Err::<(), _>(AppError::Internal(anyhow::anyhow!("boom"))) }),
                ));
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            for uri in ["/authed", "/anonymous"] {
                let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
                runtime.block_on(router.clone().oneshot(request)).unwrap();
            }
        });

        let events: Vec<_> = envelopes
            .iter()
            .flat_map(Envelope::items)
            .filter_map(|item| match item {
                EnvelopeItem::Event(event) => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 2);
        assert!(events[0].user.is_some());
        assert!(events[1].user.is_none());
        assert!(!events[1].tags.contains_key("tenant_id"));
    }
}
