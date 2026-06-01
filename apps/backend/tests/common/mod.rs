//! HTTP 統合テスト用の Axum アプリ構築ヘルパー。

use std::net::SocketAddr;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use axum_session::{SameSite, SessionConfig, SessionLayer, SessionMode, SessionStore};
use axum_session_redispool::SessionRedisPool;
use cookie::Key;
use backend::{
    AppState,
    entities::users,
    routes,
    settings,
    utils::{
        auth::create_password_hash,
        drive::DriveConfig,
        redis::RedisConnection,
        smtp::SmtpClient,
        storage::setup_storage,
    },
};
use http_body_util::BodyExt;
use reqwest::{Client, Response};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DatabaseConnection, EntityTrait,
};
use tokio::{net::TcpListener, sync::OnceCell};
use tower::ServiceExt;
use uuid::Uuid;

static SCHEMA_READY: OnceCell<()> = OnceCell::const_new();

fn test_session_config() -> SessionConfig {
    SessionConfig::default()
        .with_secure(false)
        .with_cookie_same_site(SameSite::Lax)
        .with_ip_and_user_agent(false)
        .with_prefix_with_host(false)
        .with_mode(SessionMode::Persistent)
        .with_key(Key::from(&[7u8; 64]))
        .with_database_key(Key::from(&[8u8; 64]))
}

async fn ensure_schema(db: &DatabaseConnection) {
    SCHEMA_READY
        .get_or_init(|| async {
            db.execute_unprepared(
                "ALTER TABLE users ADD COLUMN IF NOT EXISTS is_admin BOOLEAN NOT NULL DEFAULT false;
                 ALTER TABLE users ADD COLUMN IF NOT EXISTS is_suspended BOOLEAN NOT NULL DEFAULT false;
                 ALTER TABLE users ADD COLUMN IF NOT EXISTS sessions_revoked_at TIMESTAMPTZ;",
            )
            .await
            .expect("prepare admin user columns");

            db.get_schema_registry("backend::entities::*")
                .sync(db)
                .await
                .expect("sync schema");
        })
        .await;
}

pub struct TestApp {
    pub state: AppState,
    pub base_url: String,
    client: Client,
    router: Router,
}

pub struct TestUser {
    pub id: Uuid,
    pub email: String,
    pub password: String,
}

impl TestApp {
    pub async fn new() -> Self {
        dotenvy::dotenv().ok();
        let settings = settings::load_settings().expect("load settings from .env");
        let db = sea_orm::Database::connect(&settings.database_url)
            .await
            .expect("connect database");
        ensure_schema(&db).await;

        let smtp_client = SmtpClient::new(
            &settings.smtp_host,
            settings.smtp_port,
            &settings.smtp_username,
            &settings.smtp_password,
            &settings.smtp_from,
        )
        .expect("smtp client");
        let redis_client = RedisConnection::new(&settings.redis_url);
        redis_client.ping().await.expect("redis ping");

        let pg_pool = backend::jobs::setup_pool(&settings.database_url)
            .await
            .expect("pg pool");
        let verification_email_storage =
            backend::jobs::setup_verification_email_storage(&pg_pool, &settings)
                .await
                .expect("verification email storage");
        let storage = setup_storage().await.expect("storage backend");

        let state = AppState {
            settings,
            db,
            pg_pool,
            redis_client,
            smtp_client,
            verification_email_storage,
            storage,
            drive_config: DriveConfig::from_env(),
        };

        let session_store = SessionStore::<SessionRedisPool>::new(
            Some(state.redis_client.conn.clone().into()),
            test_session_config(),
        )
        .await
        .expect("session store");

        let (router, _) = routes::create_routes().split_for_parts();
        let router = router
            .with_state(state.clone())
            .layer(SessionLayer::new(session_store));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let addr: SocketAddr = listener.local_addr().expect("local addr");
        let app_router = router.clone();
        tokio::spawn(async move {
            axum::serve(listener, app_router)
                .await
                .expect("serve test app");
        });

        let client = Client::builder()
            .cookie_store(true)
            .build()
            .expect("reqwest client");

        Self {
            state,
            base_url: format!("http://{addr}"),
            client,
            router,
        }
    }

    pub async fn request(&self, req: Request<Body>) -> TestResponse {
        let response = self.router.clone().oneshot(req).await.expect("router response");
        let status = response.status();
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body collect")
            .to_bytes();
        let body = String::from_utf8_lossy(&body).into_owned();
        TestResponse {
            status,
            body,
            set_cookie,
        }
    }

    pub async fn insert_user(&self, is_admin: bool, is_suspended: bool) -> TestUser {
        insert_user(&self.state.db, is_admin, is_suspended).await
    }

    pub fn reset_session_client(&mut self) {
        self.client = Client::builder()
            .cookie_store(true)
            .build()
            .expect("reqwest client");
    }

    pub fn session_client(&self) -> Client {
        self.client.clone()
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn login_session(&mut self, email: &str, password: &str) {
        let response = self
            .client
            .post(format!("{}/v1/auth/login", self.base_url))
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await
            .expect("login request");
        assert_eq!(
            response.status(),
            StatusCode::NO_CONTENT,
            "login failed: {}",
            response.text().await.unwrap_or_default()
        );
    }

    pub async fn get_with_session(&self, path: &str) -> Response {
        self.client
            .get(format!("{}{path}", self.base_url))
            .send()
            .await
            .expect("get request")
    }

    pub async fn post_json_with_session(&self, path: &str, body: serde_json::Value) -> Response {
        self.client
            .post(format!("{}{path}", self.base_url))
            .json(&body)
            .send()
            .await
            .expect("post request")
    }

    pub async fn post_json_with_session_and_headers(
        &self,
        path: &str,
        body: serde_json::Value,
        ip: &str,
        user_agent: &str,
    ) -> Response {
        self.client
            .post(format!("{}{path}", self.base_url))
            .header("x-forwarded-for", ip)
            .header(header::USER_AGENT, user_agent)
            .json(&body)
            .send()
            .await
            .expect("post request")
    }

    pub async fn patch_json_with_session(&self, path: &str, body: serde_json::Value) -> Response {
        self.client
            .patch(format!("{}{path}", self.base_url))
            .json(&body)
            .send()
            .await
            .expect("patch request")
    }

    pub async fn get_with_bearer(&self, path: &str, token: &str) -> Response {
        self.client
            .get(format!("{}{path}", self.base_url))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await
            .expect("bearer request")
    }

    pub async fn cleanup_user(&self, user_id: Uuid) {
        let _ = users::Entity::delete_by_id(user_id)
            .exec(&self.state.db)
            .await;
    }
}

pub struct TestResponse {
    pub status: StatusCode,
    pub body: String,
    pub set_cookie: Option<String>,
}

pub async fn insert_user(
    db: &DatabaseConnection,
    is_admin: bool,
    is_suspended: bool,
) -> TestUser {
    let id = Uuid::new_v4();
    let email = format!("admin-test-{id}@example.com");
    let password = "TestPassword123!".to_string();
    let password_hash = create_password_hash(&password).expect("password hash");

    users::ActiveModel {
        id: Set(id),
        username: Set(format!("test_{}", &id.to_string()[..8])),
        bio: Set(Some(String::new())),
        avatar_url: Set(None),
        email: Set(email.clone()),
        email_verified: Set(true),
        password_hash: Set(password_hash),
        is_admin: Set(is_admin),
        is_suspended: Set(is_suspended),
        sessions_revoked_at: Set(None),
    }
    .insert(db)
    .await
    .expect("insert user");

    TestUser {
        id,
        email,
        password,
    }
}

pub async fn insert_personal_token_for_test(
    db: &DatabaseConnection,
    user_id: Uuid,
    tenant_id: Uuid,
    secret: &str,
) -> String {
    use backend::utils::auth::generate_personal_token;
    use sea_orm::Statement;

    let (token, token_hash) = generate_personal_token(secret).expect("generate pat");
    let id = Uuid::new_v4();
    let last_four = token[token.len().saturating_sub(4)..].to_string();
    let stmt = Statement::from_sql_and_values(
        db.get_database_backend(),
        r#"INSERT INTO personal_tokens
            (id, name, token, token_hash, token_last_four, user_id, tenant_id, revoked, scopes)
            VALUES ($1, $2, $3, $4, $5, $6, $7, false, '["admin:tenant"]'::json)"#,
        vec![
            id.into(),
            "integration-test".into(),
            token.clone().into(),
            token_hash.into(),
            last_four.into(),
            user_id.into(),
            tenant_id.into(),
        ],
    );
    db.execute_raw(stmt).await.expect("insert legacy pat");
    token
}

pub async fn insert_tenant(db: &DatabaseConnection, owner_id: Uuid) -> Uuid {
    use backend::entities::tenants;

    let id = Uuid::new_v4();
    tenants::ActiveModel {
        id: Set(id),
        display_id: Set(format!("t-{}", &id.to_string()[..8])),
        name: Set("Test Tenant".into()),
        description: Set(String::new()),
        icon_url: Set(String::new()),
        owner_id: Set(owner_id),
        drive_quota_bytes: Set(None),
    }
    .insert(db)
    .await
    .expect("insert tenant");
    id
}
