use apalis::layers::WorkerBuilderExt;
use apalis::layers::retry::RetryPolicy;
use apalis::prelude::WorkerBuilder;
use apalis_board::axum::{
    framework::{ApiBuilder, RegisterRoute},
    sse::{TracingBroadcaster, TracingSubscriber},
    ui::ServeUI,
};
use axum::{
    Extension, Router,
    body::Body,
    http::{HeaderValue, Method, Request},
    middleware,
    response::Redirect,
    routing::get,
};
use axum_session::{SameSite, SessionConfig, SessionLayer, SessionStore};
use axum_session_redispool::SessionRedisPool;
use sentry::integrations::tower::NewSentryLayer;
use tokio::sync::watch;
use tower::ServiceBuilder;
use tower_http::cors::{AllowHeaders, CorsLayer};
use tracing::{info, warn};
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};
use utoipa_scalar::{Scalar, Servable};

use handler::AppState;
use handler::middlewares::logging::logging_middleware;
use job::{
    JobState,
    already_registered_email::{
        self, MAX_RETRIES as ALREADY_REGISTERED_MAX_RETRIES, QUEUE_NAME as ALREADY_REGISTERED_QUEUE,
    },
    github_issue_sync::{self, QUEUE_NAME as GITHUB_ISSUE_SYNC_QUEUE},
    github_webhook::{self, QUEUE_NAME as GITHUB_WEBHOOK_QUEUE},
    notification_email, notification_retention,
    password_reset_email::{
        self, MAX_RETRIES as PW_RESET_MAX_RETRIES, QUEUE_NAME as PW_RESET_QUEUE,
    },
    review_summary::{
        self, MAX_RETRIES as REVIEW_SUMMARY_MAX_RETRIES, QUEUE_NAME as REVIEW_SUMMARY_QUEUE,
    },
    supervise::supervise,
    tenant_invitation_email::{
        self, MAX_RETRIES as TENANT_INVITATION_MAX_RETRIES, QUEUE_NAME as TENANT_INVITATION_QUEUE,
    },
    verification_email::{self, MAX_RETRIES, QUEUE_NAME},
    webhook_delivery,
};

pub async fn run(state: AppState) -> Result<(), Box<dyn std::error::Error>> {
    let log_filter = tracing_subscriber::EnvFilter::new(
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info,sqlx=warn".into()),
    );

    let broadcaster = TracingBroadcaster::create();
    let board_tracing = TracingSubscriber::new(&broadcaster)
        .layer()
        .with_filter(log_filter.clone());

    tracing_subscriber::registry()
        .with(log_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(board_tracing)
        .init();

    let is_prod = std::env::var("RUST_ENV").unwrap_or_default() == "production";
    let settings = &state.settings;
    let addr = settings.listen_addr.clone();

    let session_config = SessionConfig::default()
        .with_secure(is_prod)
        .with_cookie_same_site(if is_prod {
            SameSite::None
        } else {
            SameSite::Lax
        });

    let session_store = SessionStore::<SessionRedisPool>::new(
        Some(state.redis_client.conn.clone().into()),
        session_config,
    )
    .await?;

    let (router, mut openapi) = utoipa_axum::router::OpenApiRouter::new()
        .merge(handler::routes::create_routes())
        .split_for_parts();

    handler::openapi::register_schemas(&mut openapi);

    let cors = CorsLayer::new()
        .allow_origin(settings.allow_origin.parse::<HeaderValue>()?)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(true);

    let email_storage = state.verification_email_storage.as_ref().clone();
    let board_api = ApiBuilder::new(Router::new())
        .register(email_storage)
        .build();

    // ワーカーには AppState 全体ではなく、実際に使う依存だけを渡す（job → handler の循環回避）。
    let job_state = JobState {
        settings: state.settings.clone(),
        db: state.db.clone(),
        redis_client: state.redis_client.clone(),
        smtp_client: state.smtp_client.clone(),
        http_client: state.http_client.clone(),
        review_summary_storage: state.review_summary_storage.clone(),
        github_issue_sync_storage: state.github_issue_sync_storage.clone(),
        pg_pool: state.pg_pool.clone(),
        oauth_settings: state.oauth_settings.clone(),
    };

    // 各ワーカーは監督ループの下で動かし、heartbeat の失敗などで終了したら世代を作り直す
    // （job::supervise）。
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let email_worker_storage = state.verification_email_storage.as_ref().clone();
    let email_worker_state = job_state.clone();
    let email_worker_concurrency = verification_email::worker_concurrency(settings);
    let email_worker = tokio::spawn(supervise(
        format!("{QUEUE_NAME}-worker"),
        move |mut ctx| {
            let worker = WorkerBuilder::new(ctx.name())
                .backend(email_worker_storage.clone())
                .retry(RetryPolicy::retries(MAX_RETRIES))
                .enable_tracing()
                .concurrency(email_worker_concurrency)
                .data(email_worker_state.clone())
                .build(verification_email::process);
            Box::pin(async move { worker.run_with_ctx(&mut ctx).await })
        },
        shutdown_rx.clone(),
    ));

    let pw_reset_worker_storage = state.password_reset_email_storage.as_ref().clone();
    let pw_reset_worker_state = job_state.clone();
    let pw_reset_worker_concurrency = password_reset_email::worker_concurrency(settings);
    let pw_reset_worker = tokio::spawn(supervise(
        format!("{PW_RESET_QUEUE}-worker"),
        move |mut ctx| {
            let worker = WorkerBuilder::new(ctx.name())
                .backend(pw_reset_worker_storage.clone())
                .retry(RetryPolicy::retries(PW_RESET_MAX_RETRIES))
                .enable_tracing()
                .concurrency(pw_reset_worker_concurrency)
                .data(pw_reset_worker_state.clone())
                .build(password_reset_email::process);
            Box::pin(async move { worker.run_with_ctx(&mut ctx).await })
        },
        shutdown_rx.clone(),
    ));

    let already_registered_worker_storage = state.already_registered_email_storage.as_ref().clone();
    let already_registered_worker_state = job_state.clone();
    let already_registered_worker_concurrency =
        already_registered_email::worker_concurrency(settings);
    let already_registered_worker = tokio::spawn(supervise(
        format!("{ALREADY_REGISTERED_QUEUE}-worker"),
        move |mut ctx| {
            let worker = WorkerBuilder::new(ctx.name())
                .backend(already_registered_worker_storage.clone())
                .retry(RetryPolicy::retries(ALREADY_REGISTERED_MAX_RETRIES))
                .enable_tracing()
                .concurrency(already_registered_worker_concurrency)
                .data(already_registered_worker_state.clone())
                .build(already_registered_email::process);
            Box::pin(async move { worker.run_with_ctx(&mut ctx).await })
        },
        shutdown_rx.clone(),
    ));

    let tenant_invitation_worker_storage = state.tenant_invitation_email_storage.as_ref().clone();
    let tenant_invitation_worker_state = job_state.clone();
    let tenant_invitation_worker = tokio::spawn(supervise(
        format!("{TENANT_INVITATION_QUEUE}-worker"),
        move |mut ctx| {
            let worker = WorkerBuilder::new(ctx.name())
                .backend(tenant_invitation_worker_storage.clone())
                .retry(RetryPolicy::retries(TENANT_INVITATION_MAX_RETRIES))
                .enable_tracing()
                .data(tenant_invitation_worker_state.clone())
                .build(tenant_invitation_email::process);
            Box::pin(async move { worker.run_with_ctx(&mut ctx).await })
        },
        shutdown_rx.clone(),
    ));

    let github_worker_storage = state.github_webhook_storage.as_ref().clone();
    let github_worker_state = job_state.clone();
    let github_worker_concurrency = github_webhook::worker_concurrency(settings);
    let github_worker = tokio::spawn(supervise(
        format!("{GITHUB_WEBHOOK_QUEUE}-worker"),
        move |mut ctx| {
            let worker = WorkerBuilder::new(ctx.name())
                .backend(github_worker_storage.clone())
                .retry(RetryPolicy::retries(github_webhook::MAX_RETRIES))
                .enable_tracing()
                .concurrency(github_worker_concurrency)
                .data(github_worker_state.clone())
                .build(github_webhook::process);
            Box::pin(async move { worker.run_with_ctx(&mut ctx).await })
        },
        shutdown_rx.clone(),
    ));

    let review_summary_worker_storage = state.review_summary_storage.as_ref().clone();
    let review_summary_worker_state = job_state.clone();
    let review_summary_worker_concurrency = review_summary::worker_concurrency(settings);
    let review_summary_worker = tokio::spawn(supervise(
        format!("{REVIEW_SUMMARY_QUEUE}-worker"),
        move |mut ctx| {
            let worker = WorkerBuilder::new(ctx.name())
                .backend(review_summary_worker_storage.clone())
                .retry(RetryPolicy::retries(REVIEW_SUMMARY_MAX_RETRIES))
                .enable_tracing()
                .concurrency(review_summary_worker_concurrency)
                .data(review_summary_worker_state.clone())
                .build(review_summary::process);
            Box::pin(async move { worker.run_with_ctx(&mut ctx).await })
        },
        shutdown_rx.clone(),
    ));

    let job_state_for_sweeper = job_state.clone();
    let issue_sync_worker_storage = state.github_issue_sync_storage.as_ref().clone();
    let issue_sync_worker_state = job_state;
    let issue_sync_worker_concurrency = github_issue_sync::worker_concurrency(settings);
    let issue_sync_worker = tokio::spawn(supervise(
        format!("{GITHUB_ISSUE_SYNC_QUEUE}-worker"),
        move |mut ctx| {
            let worker = WorkerBuilder::new(ctx.name())
                .backend(issue_sync_worker_storage.clone())
                .retry(RetryPolicy::retries(github_issue_sync::MAX_RETRIES))
                .enable_tracing()
                .concurrency(issue_sync_worker_concurrency)
                .data(issue_sync_worker_state.clone())
                .build(github_issue_sync::process);
            Box::pin(async move { worker.run_with_ctx(&mut ctx).await })
        },
        shutdown_rx.clone(),
    ));

    // 書き戻し要求（pending_push）の取りこぼしを定期的に拾い直す。
    // タスク更新後のジョブ登録が失敗しても、要求は DB に残っているのでここで回収する。
    let sweep_db = state.db.clone();
    let sweep_storage = state.github_issue_sync_storage.clone();
    let mut sweep_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            github_issue_sync::SWEEP_INTERVAL_SECS,
        ));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match github_issue_sync::sweep_pending(&sweep_db, &sweep_storage).await {
                        Ok(0) => {}
                        Ok(n) => info!(count = n, "github issue push sweep re-enqueued"),
                        Err(e) => warn!(error = %e, "github issue push sweep failed"),
                    }
                }
                _ = sweep_shutdown.changed() => break,
            }
        }
    });

    // 通知メールの掃き出し。通知の行が outbox なので、ジョブの投入ではなく
    // 定期的に「メール待ち」の行を拾って送る（job::notification_email）。
    tokio::spawn(notification_email::run_sweeper(
        job_state_for_sweeper.clone(),
        shutdown_rx.clone(),
    ));

    // 保持期間を過ぎた通知の掃除（job::notification_retention）。1 日 1 回、起動直後にも走る
    tokio::spawn(notification_retention::run_sweeper(
        job_state_for_sweeper.clone(),
        shutdown_rx.clone(),
    ));

    // Webhook の配信。配信の行が outbox なので、同じく定期的に期限の来た行を拾って送る
    // （job::webhook_delivery）。90 日より古い履歴の掃除もこのループが行う。
    tokio::spawn(webhook_delivery::run_sweeper(
        job_state_for_sweeper,
        shutdown_rx.clone(),
    ));

    let api = router
        .merge(Scalar::with_url("/scalar", openapi.clone()))
        .with_state(state.clone())
        .layer(cors)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            handler::middlewares::csrf::csrf_origin_check,
        ))
        .layer(middleware::from_fn(logging_middleware))
        .layer(SessionLayer::new(session_store.clone()))
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top()));

    // apalis-board の UI はビルド時に API=/api/v1・静的ファイル=/ 直下を前提とする。
    // /jobs にネストすると JS/WASM が 404 になり真っ白になる。
    // ジョブペイロード（メール認証トークン・メールアドレス）と SSE ログを露出するため、
    // board 系ルートはすべて管理者セッション必須。board API は再試行等の状態変更を持ち
    // 公開 API と同じ SameSite=None セッション Cookie で認証するため、CSRF 検査も api と同様に通す。
    let board = Router::new()
        .nest("/api/v1", board_api)
        .route("/jobs", get(|| async { Redirect::permanent("/") }))
        .route("/jobs/", get(|| async { Redirect::permanent("/") }))
        .fallback_service(ServeUI::new())
        .layer(Extension(broadcaster))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            handler::middlewares::admin_gate::require_admin_session,
        ))
        .layer(middleware::from_fn_with_state(
            state,
            handler::middlewares::csrf::csrf_origin_check,
        ))
        .layer(SessionLayer::new(session_store));

    let app = Router::new().merge(api).merge(board);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Listening on http://{addr}");
    info!("Apalis board: http://{addr}/");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal_inner().await;
            let _ = shutdown_tx.send(true);
            info!("shutting down HTTP server; Apalis workers finishing in-flight jobs");
        })
        .await?;

    for handle in [
        email_worker,
        pw_reset_worker,
        already_registered_worker,
        tenant_invitation_worker,
        github_worker,
        review_summary_worker,
        issue_sync_worker,
    ] {
        if let Err(e) = handle.await {
            warn!("apalis worker supervisor join error: {e}");
        }
    }
    info!("apalis workers stopped");

    Ok(())
}

async fn shutdown_signal_inner() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        sigterm.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => warn!("received Ctrl+C"),
        () = terminate => warn!("received SIGTERM"),
    }
}
