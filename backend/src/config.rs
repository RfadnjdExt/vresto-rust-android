#[derive(Clone, Debug)]
pub struct Config {
    pub database_path: String,
    pub bind: String,
    pub jwt_secret: String,

    pub tz_offset_minutes: i32,
    pub token_ttl_hours: i64,
}

impl Config {
    pub fn from_env() -> Self {
        let jwt_secret = std::env::var("VRESTO_JWT_SECRET").unwrap_or_else(|_| {
            tracing::warn!("VRESTO_JWT_SECRET tidak diset — memakai secret acak; semua token hangus saat restart.");
            format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
        });
        Self {
            database_path: std::env::var("VRESTO_DB").unwrap_or_else(|_| "vresto.sqlite".into()),
            bind: std::env::var("VRESTO_BIND").unwrap_or_else(|_| "0.0.0.0:8000".into()),
            jwt_secret,
            tz_offset_minutes: std::env::var("VRESTO_TZ_OFFSET_MIN").ok().and_then(|v| v.parse().ok()).unwrap_or(420),
            token_ttl_hours: std::env::var("VRESTO_TOKEN_TTL_HOURS").ok().and_then(|v| v.parse().ok()).unwrap_or(12),
        }
    }

    pub fn test() -> Self {
        Self {
            database_path: ":memory:".into(),
            bind: "127.0.0.1:0".into(),
            jwt_secret: "test-secret-test-secret-test-secret".into(),
            tz_offset_minutes: 420,
            token_ttl_hours: 1,
        }
    }
}
