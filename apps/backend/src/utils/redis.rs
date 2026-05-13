use redis_pool::SingleRedisPool;

#[derive(Clone)]
pub struct RedisConnection {
    pub conn: SingleRedisPool,
}

impl RedisConnection {
    pub fn new(url: &str) -> Self {
        let client = redis::Client::open(url).unwrap();
        let pool = SingleRedisPool::from(client);
        Self { conn: pool }
    }
}
