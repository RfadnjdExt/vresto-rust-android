use std::{collections::HashMap, sync::{Arc, Mutex}, time::Instant};

use rusqlite::Connection;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::{config::Config, error::{AppError, Res}};

#[derive(Clone)]
pub struct Env {
    pub tz_offset_min: i32,
    pub hub: broadcast::Sender<String>,
}

impl Env {

    pub fn emit(&self, event: &str, data: Value) {
        let _ = self.hub.send(json!({ "event": event, "data": data }).to_string());
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub cfg: Arc<Config>,
    pub env: Env,
    pub login_attempts: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}

impl AppState {
    pub fn new(conn: Connection, cfg: Config) -> Self {
        let (hub, _) = broadcast::channel(256);
        Self {
            env: Env { tz_offset_min: cfg.tz_offset_minutes, hub },
            db: Arc::new(Mutex::new(conn)),
            cfg: Arc::new(cfg),
            login_attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn db<T, F>(&self, f: F) -> Res<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection, &Env) -> Res<T> + Send + 'static,
    {
        let db = self.db.clone();
        let env = self.env.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = db.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut guard, &env)
        })
        .await
        .map_err(|e| AppError::Internal(format!("task gagal: {e}")))?
    }
}
