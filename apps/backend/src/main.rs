use backend::{AppState, server::run, utils::smtp::SmtpClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = backend::settings::load_settings()?;
    let _guard = if let Some(ref sentry_dsn) = settings.sentry_dsn {
        Some(sentry::init(sentry::ClientOptions {
            dsn: Some(sentry_dsn.parse()?),
            release: sentry::release_name!(),
            // 本番既定は false。必要時のみ環境変数で opt-in 推奨
            send_default_pii: false,
            ..Default::default()
        }))
    } else {
        None
    };
    let db = sea_orm::Database::connect(&settings.database_url).await?;
    db.get_schema_registry("backend::entities::*")
        .sync(&db)
        .await?;

    let smtp_client = SmtpClient::new(
        &settings.smtp_host,
        settings.smtp_port,
        &settings.smtp_username,
        &settings.smtp_password,
        &settings.smtp_from,
    )
     .map_err(|err| {
         std::io::Error::other(format!(
             "SMTP client initialization failed. If email is required in this environment, check smtp_host/smtp_port/smtp_username/smtp_password. Underlying error: {err}"
         ))
     })?;
    let redis_client = backend::utils::redis::RedisConnection::new(&settings.redis_url);
    redis_client.ping().await?;

    let pg_pool = backend::jobs::setup_pool(&settings.database_url).await?;
    let verification_email_storage =
        backend::jobs::setup_verification_email_storage(&pg_pool, &settings).await?;

    let storage = backend::utils::storage::setup_storage().await.map_err(|e| {
        std::io::Error::other(format!(
            "storage backend initialization failed (STORAGE_BACKEND / S3_* / LOCAL_UPLOAD_DIR): {e}"
        ))
    })?;

    let drive_config = backend::utils::drive::DriveConfig::from_env();

    // 起動時: システム上限を超過しているテナントを警告ログに出力
    if let Some(system_max) = drive_config.system_max_bytes_opt() {
        use backend::entities::tenants;
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        let violators = tenants::Entity::find()
            .filter(tenants::Column::DriveQuotaBytes.gt(system_max))
            .all(&db)
            .await?;
        for t in violators {
            tracing::warn!(
                tenant_id = %t.id,
                quota_bytes = ?t.drive_quota_bytes,
                system_max_bytes = system_max,
                "tenant drive quota exceeds system_max — update tenant quota or raise DRIVE_SYSTEM_MAX_QUOTA_MB"
            );
        }
    }

    // BOOTSTRAP_ADMIN_EMAIL: 管理者ゼロ時のみ対象ユーザーを昇格
    if let Some(ref email) = settings.bootstrap_admin_email {
        use backend::entities::{audit_logs, users};
        use sea_orm::{
            ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait,
            QueryFilter,
        };
        let admin_count = users::Entity::find()
            .filter(users::Column::IsAdmin.eq(true))
            .count(&db)
            .await?;
        if admin_count == 0 {
            if let Some(user) = users::Entity::find()
                .filter(users::Column::Email.eq(email.as_str()))
                .one(&db)
                .await?
            {
                let mut active: users::ActiveModel = user.clone().into();
                active.is_admin = Set(true);
                active.update(&db).await?;

                let log = audit_logs::ActiveModel {
                    id: Set(uuid::Uuid::new_v4()),
                    actor_id: Set(None),
                    actor_type: Set("system".to_string()),
                    action: Set("user.admin.grant".to_string()),
                    resource_type: Set("user".to_string()),
                    resource_id: Set(user.id.to_string()),
                    tenant_id: Set(None),
                    metadata: Set(Some(serde_json::json!({ "user_id": user.id.to_string() }))),
                    ip_address: Set(None),
                    user_agent: Set(None),
                    created_at: Set(chrono::Utc::now()),
                };
                log.insert(&db).await?;
                tracing::info!(user_id = %user.id, email = %email, "bootstrap: promoted to admin");
            } else {
                tracing::warn!(email = %email, "BOOTSTRAP_ADMIN_EMAIL set but no matching user found");
            }
        }
    }

    let state = AppState {
        settings,
        db,
        pg_pool,
        redis_client,
        smtp_client,
        verification_email_storage,
        storage,
        drive_config,
    };
    run(state).await?;

    Ok(())
}
