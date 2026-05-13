use deadpool_redis::{Config, Pool, Runtime};

#[derive(Clone)]
pub struct RedisConnection {
    pub conn: Pool,
}

impl RedisConnection {
    pub fn new(url: &str) -> Self {
        let cfg = Config::from_url(url);
        let pool = cfg.create_pool(Some(Runtime::Tokio1)).unwrap();
        Self { conn: pool }
    }
}
