//! HTTP 統合テスト用の Axum アプリ構築ヘルパー。

use std::net::SocketAddr;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use axum_session::{SameSite, SessionConfig, SessionLayer, SessionMode, SessionStore};
use axum_session_redispool::SessionRedisPool;
use backend::{
    AppState,
    entities::users,
    jobs::{setup_pool, setup_verification_email_storage},
    routes,
    settings,
    utils::{
        auth::create_password_hash,
        drive::DriveConfig,
        redis::RedisConnection,
        smtp::SmtpClient,
        storage::setup_storage,
        totp::build_totp,
    },
};
use cookie::Key;
use http_body_util::BodyExt;
use reqwest::{Client, Response};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DatabaseConnection, EntityTrait};
use tokio::{net::TcpListener, sync::OnceCell};
use tower::ServiceExt;
use url::Url;
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

            db.execute_unprepared(
                r#"
ALTER TABLE users ADD COLUMN IF NOT EXISTS totp_enabled BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS require_2fa BOOLEAN NOT NULL DEFAULT false;
UPDATE users SET totp_enabled = false WHERE totp_enabled IS NULL;
ALTER TABLE users ALTER COLUMN totp_enabled SET NOT NULL;
ALTER TABLE users ALTER COLUMN totp_enabled SET DEFAULT false;

CREATE TABLE IF NOT EXISTS totp_credentials (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    secret_enc TEXT NOT NULL,
    is_verified BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS recovery_codes (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash VARCHAR NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_recovery_codes_user ON recovery_codes(user_id);
"#,
            )
            .await
            .expect("prepare 2fa schema");

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

pub struct Enabled2fa {
    pub recovery_codes: Vec<String>,
}

pub struct TestResponse {
    pub status: StatusCode,
    pub body: String,
    pub set_cookie: Option<String>,
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

        let pg_pool = setup_pool(&settings.database_url)
            .await
            .expect("pg pool");
        let verification_email_storage =
            setup_verification_email_storage(&pg_pool, &settings)
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

    pub async fn insert_user_default(&self) -> TestUser {
        insert_user(&self.state.db, false, false).await
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

    pub async fn login_session(&mut self, email: &str, password: &str) -> StatusCode {
        let response = self
            .client
            .post(format!("{}/v1/auth/login", self.base_url))
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await
            .expect("login request");
        response.status()
    }

    pub async fn login_session_no_content(&mut self, email: &str, password: &str) {
        let status = self.login_session(email, password).await;
        assert_eq!(
            status,
            StatusCode::NO_CONTENT,
            "login expected 204 NO_CONTENT"
        );
    }

    pub async fn enable_2fa(&mut self, user: &TestUser) -> Enabled2fa {
        let status = self.login_session(&user.email, &user.password).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "full login before 2fa setup");

        let setup = self
            .post_json_with_session("/v1/auth/2fa/totp/setup", serde_json::json!({}))
            .await;
        assert_eq!(setup.status(), StatusCode::OK);
        let setup_body: serde_json::Value = setup.json().await.expect("setup json");
        let uri = setup_body["otpauth_uri"]
            .as_str()
            .expect("otpauth_uri");
        let secret = secret_from_otpauth_uri(uri);
        let totp = build_totp(
            &secret,
            &self.state.settings.totp_issuer,
            &user.email,
        )
        .expect("build totp");
        let code = totp.generate_current().expect("totp code");

        let verify = self
            .post_json_with_session(
                "/v1/auth/2fa/totp/verify-setup",
                serde_json::json!({ "code": code }),
            )
            .await;
        assert_eq!(verify.status(), StatusCode::OK);
        let body: serde_json::Value = verify.json().await.expect("verify-setup json");
        let codes: Vec<String> = body["recovery_codes"]
            .as_array()
            .expect("recovery_codes array")
            .iter()
            .map(|v| v.as_str().expect("code str").to_string())
            .collect();

        self.reset_session_client();
        Enabled2fa {
            recovery_codes: codes,
        }
    }

    pub async fn login_half_authed(&mut self, user: &TestUser) {
        let response = self
            .client
            .post(format!("{}/v1/auth/login", self.base_url))
            .json(&serde_json::json!({ "email": user.email, "password": user.password }))
            .send()
            .await
            .expect("login");
        assert_eq!(response.status(), StatusCode::OK, "2fa login returns JSON body");
        let body: serde_json::Value = response.json().await.expect("login json");
        assert_eq!(body["requires_2fa"].as_bool(), Some(true));
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

    pub async fn delete_json_with_session(&self, path: &str, body: serde_json::Value) -> Response {
        self.client
            .delete(format!("{}{path}", self.base_url))
            .json(&body)
            .send()
            .await
            .expect("delete request")
    }

    pub async fn post_json(&self, path: &str, body: serde_json::Value) -> Response {
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

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub async fn cleanup_user(&self, user_id: Uuid) {
        let _ = users::Entity::delete_by_id(user_id)
            .exec(&self.state.db)
            .await;
    }
}

pub fn secret_from_otpauth_uri(uri: &str) -> String {
    let parsed = Url::parse(uri).expect("otpauth uri");
    parsed
        .query_pairs()
        .find(|(k, _)| k == "secret")
        .map(|(_, v)| v.into_owned())
        .expect("secret query param")
}

pub fn current_totp_code(secret: &str, issuer: &str, email: &str) -> String {
    let totp = build_totp(secret, issuer, email).expect("totp");
    totp.generate_current().expect("code")
}

pub async fn insert_user(
    db: &DatabaseConnection,
    is_admin: bool,
    is_suspended: bool,
) -> TestUser {
    let id = Uuid::new_v4();
    let email = format!("test-{id}@example.com");
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
        totp_enabled: Set(false),
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
        require_2fa: Set(false),
    }
    .insert(db)
    .await
    .expect("insert tenant");
    id
}
