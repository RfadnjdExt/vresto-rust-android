use rusqlite::{params, Connection, ToSql};
use serde_json::{json, Value};

use crate::{db, error::{AppError, Res}};

pub fn list(conn: &Connection, q: Option<&str>, limit: i64) -> Res<Vec<Value>> {
    let limit = limit.clamp(1, 200);
    match q.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => {
            let like = format!("%{s}%");
            db::query_rows(conn, "SELECT * FROM customers WHERE is_active=1 AND (name LIKE ?1 OR phone LIKE ?1 OR email LIKE ?1) ORDER BY name LIMIT ?2", &[&like, &limit])
        }
        None => db::query_rows(conn, "SELECT * FROM customers WHERE is_active=1 ORDER BY name LIMIT ?1", &[&limit]),
    }
}

pub fn save(conn: &Connection, id: Option<i64>, body: &Value) -> Res<Value> {
    let name = body["name"].as_str().map(str::trim).unwrap_or("");
    if name.is_empty() { return Err(AppError::validation("Nama pelanggan wajib diisi.")); }
    let s = |k: &str| body[k].as_str().map(str::to_owned);
    let credit = body["credit_limit"].as_i64().unwrap_or(0);
    if credit < 0 { return Err(AppError::validation("Limit hutang tidak boleh negatif.")); }
    let id = match id {
        Some(id) => {
            let n = conn.execute(
                "UPDATE customers SET name=?, phone=?, email=?, address=?, credit_limit=?, default_due_days=COALESCE(?, default_due_days) WHERE id=?",
                params![name, s("phone"), s("email"), s("address"), credit, body["default_due_days"].as_i64(), id])?;
            if n == 0 { return Err(AppError::not_found("Pelanggan tidak ditemukan.")); }
            id
        }
        None => {
            conn.execute(
                "INSERT INTO customers (uuid,name,phone,email,address,credit_limit,created_at) VALUES (?,?,?,?,?,?,?)",
                params![uuid::Uuid::new_v4().to_string(), name, s("phone"), s("email"), s("address"), credit, db::now_ts()])?;
            conn.last_insert_rowid()
        }
    };
    Ok(db::query_one(conn, "SELECT * FROM customers WHERE id=?", &[&id])?.unwrap_or(Value::Null))
}

pub fn delete(conn: &Connection, id: i64) -> Res<()> {
    let debt: i64 = conn.query_row("SELECT total_debt FROM customers WHERE id=?", [id], |r| r.get(0))
        .map_err(|_| AppError::not_found("Pelanggan tidak ditemukan."))?;
    if debt > 0 { return Err(AppError::business("Pelanggan masih memiliki hutang.")); }
    conn.execute("UPDATE customers SET is_active=0 WHERE id=?", [id])?;
    Ok(())
}

pub fn history(conn: &Connection, id: i64) -> Res<Vec<Value>> {
    db::query_rows(conn,
        "SELECT t.id, t.trx_number, t.total_amount, t.payment_method, t.status, t.paid_at, o.order_number
         FROM transactions t LEFT JOIN orders o ON o.id=t.order_id WHERE t.customer_id=? ORDER BY t.id DESC LIMIT 100", &[&id as &dyn ToSql])
}

pub fn record_visit(conn: &Connection, id: i64, amount: i64) -> Res<()> {
    conn.execute("UPDATE customers SET total_visits=total_visits+1, total_spent=total_spent+?1, points=points+?2 WHERE id=?3",
        params![amount, amount / 10_000, id])?;
    recompute_level(conn, id)
}

fn recompute_level(conn: &Connection, id: i64) -> Res<()> {
    let spent: i64 = conn.query_row("SELECT total_spent FROM customers WHERE id=?", [id], |r| r.get(0))?;
    let level = match spent { s if s >= 10_000_000 => "platinum", s if s >= 5_000_000 => "gold", s if s >= 1_000_000 => "silver", _ => "regular" };
    conn.execute("UPDATE customers SET membership_level=? WHERE id=?", params![level, id])?;
    Ok(())
}

pub fn reverse_visit(conn: &Connection, id: i64, spent: i64, points_earned: i64, drop_visit: bool) -> Res<()> {
    conn.execute(
        "UPDATE customers SET total_spent = MAX(0, total_spent - ?1), points = MAX(0, points - ?2), total_visits = MAX(0, total_visits - ?3) WHERE id = ?4",
        params![spent, points_earned, drop_visit as i64, id])?;
    recompute_level(conn, id)
}

pub fn restore_points(conn: &Connection, id: i64, points: i64) -> Res<()> {
    if points > 0 { conn.execute("UPDATE customers SET points = points + ? WHERE id = ?", params![points, id])?; }
    Ok(())
}

pub fn summary(c: &Value) -> Value { json!({ "id": c["id"], "name": c["name"] }) }
