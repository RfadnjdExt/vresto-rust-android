use std::time::{Duration, Instant};

use axum::{extract::FromRequestParts, http::request::Parts};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    config::Config,
    db,
    error::{AppError, Res},
    state::AppState,
};

#[derive(Debug, Serialize, Deserialize)]
struct Claims { sub: i64, exp: usize }

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub name: String,
    pub role: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool { self.role == "admin" }
    pub fn require_admin(&self) -> Res<()> {
        if self.is_admin() { Ok(()) } else { Err(AppError::Forbidden("Hanya admin yang boleh melakukan ini.".into())) }
    }
}

const MAX_ATTEMPTS: u32 = 5;
const WINDOW: Duration = Duration::from_secs(60);

pub fn issue_token(cfg: &Config, user_id: i64) -> Res<String> {
    let exp = (db::now() + chrono::Duration::hours(cfg.token_ttl_hours)).timestamp() as usize;
    encode(&Header::default(), &Claims { sub: user_id, exp }, &EncodingKey::from_secret(cfg.jwt_secret.as_bytes()))
        .map_err(|e| AppError::Internal(e.to_string()))
}

fn user_from_token(conn: &Connection, cfg: &Config, token: &str) -> Res<AuthUser> {
    let claims = decode::<Claims>(token, &DecodingKey::from_secret(cfg.jwt_secret.as_bytes()), &Validation::default())
        .map_err(|_| AppError::Unauthorized("Sesi tidak valid atau kedaluwarsa. Silakan login ulang.".into()))?
        .claims;

    let row = conn.query_row("SELECT id, name, role, is_active FROM users WHERE id = ?", [claims.sub], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
    });
    match row {
        Ok((id, name, role, 1)) => Ok(AuthUser { id, name, role }),
        _ => Err(AppError::Unauthorized("Akun tidak aktif atau tidak ditemukan.".into())),
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let from_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::to_owned);

        let from_query = parts.uri.query().and_then(|q| {
            q.split('&').find_map(|kv| kv.strip_prefix("token=").map(str::to_owned))
        });
        let token = from_header.or(from_query).ok_or_else(|| AppError::Unauthorized("Token tidak ada.".into()))?;
        let st = state.clone();
        state.db(move |conn, _| user_from_token(conn, &st.cfg, &token)).await
    }
}

pub fn login(conn: &Connection, st: &AppState, login: &str, password: &str, client: &str) -> Res<Value> {
    if login.trim().is_empty() { return Err(AppError::validation("Username wajib diisi.")); }
    if password.len() < 6 { return Err(AppError::validation("Password minimal 6 karakter.")); }

    let key = format!("{}|{}", login.to_lowercase(), client);
    {
        let m = st.login_attempts.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((n, since)) = m.get(&key) {
            if since.elapsed() < WINDOW && *n >= MAX_ATTEMPTS {
                let left = WINDOW.saturating_sub(since.elapsed()).as_secs();
                return Err(AppError::validation(format!("Terlalu banyak percobaan. Coba lagi dalam {left} detik.")));
            }
        }
    }
    let bad = |st: &AppState| {
        let mut m = st.login_attempts.lock().unwrap_or_else(|e| e.into_inner());
        let e = m.entry(key.clone()).or_insert((0, Instant::now()));
        if e.1.elapsed() >= WINDOW { *e = (0, Instant::now()); }
        e.0 += 1;
        AppError::validation("Username atau password salah.")
    };

    let field = if login.contains('@') { "email" } else { "username" };
    let row = conn.query_row(
        &format!("SELECT id, name, role, password_hash, is_active FROM users WHERE {field} = ?"),
        [login],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?)),
    );
    let Ok((id, name, role, hash, active)) = row else { return Err(bad(st)) };
    if active != 1 { return Err(AppError::validation("Akun tidak aktif. Hubungi administrator.")); }

    if !bcrypt::verify(password, &hash).unwrap_or(false) { return Err(bad(st)); }

    st.login_attempts.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
    conn.execute("UPDATE users SET last_login_at = ? WHERE id = ?", params![db::now_ts(), id])?;
    Ok(json!({
        "token": issue_token(&st.cfg, id)?,
        "token_type": "Bearer",
        "expires_in": st.cfg.token_ttl_hours * 3600,
        "user": { "id": id, "name": name, "role": role },
    }))
}
