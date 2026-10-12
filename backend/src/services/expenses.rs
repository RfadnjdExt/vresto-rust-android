use rusqlite::{params, Connection};
use serde_json::Value;

use crate::{auth::AuthUser, db, error::{AppError, Res}, state::Env};

pub fn list(conn: &Connection, user: &AuthUser, all: bool) -> Res<Vec<Value>> {
    if all {
        user.require_admin()?;
        return db::query_rows(conn, "SELECT e.*, u.name AS created_by_name FROM expenses e JOIN users u ON u.id=e.created_by ORDER BY e.id DESC LIMIT 500", &[]);
    }
    match super::shifts::open_shift_id(conn, user.id)? {
        Some(sid) => db::query_rows(conn, "SELECT * FROM expenses WHERE shift_id=? ORDER BY id DESC", &[&sid]),
        None => Ok(vec![]),
    }
}

pub fn create(conn: &Connection, env: &Env, user: &AuthUser, category: &str, description: Option<&str>, amount: i64) -> Res<Value> {
    if category.trim().is_empty() { return Err(AppError::validation("Kategori pengeluaran wajib diisi.")); }
    if amount <= 0 { return Err(AppError::validation("Nominal harus lebih dari 0.")); }
    let shift_id = super::shifts::open_shift_id(conn, user.id)?.ok_or_else(|| AppError::conflict("Shift belum dibuka."))?;
    let date = db::local_date(db::now(), env.tz_offset_min).to_string();
    conn.execute("INSERT INTO expenses (shift_id,category,description,amount,expense_date,created_by,created_at) VALUES (?,?,?,?,?,?,?)",
        params![shift_id, category.trim(), description, amount, date, user.id, db::now_ts()])?;
    let id = conn.last_insert_rowid();
    db::audit(conn, Some(user.id), "expense_created", "expense", id, serde_json::json!({"amount": amount, "category": category}))?;
    Ok(db::query_one(conn, "SELECT * FROM expenses WHERE id=?", &[&id])?.unwrap_or(Value::Null))
}

pub fn delete(conn: &Connection, user: &AuthUser, id: i64) -> Res<()> {
    let (created_by, status): (i64, String) = conn.query_row(
        "SELECT e.created_by, COALESCE(s.status,'closed') FROM expenses e LEFT JOIN shifts s ON s.id=e.shift_id WHERE e.id=?",
        [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if created_by != user.id && !user.is_admin() { return Err(AppError::Forbidden("Pengeluaran milik kasir lain.".into())); }
    if status != "open" { return Err(AppError::business("Shift sudah ditutup; pengeluaran tidak bisa diubah.")); }
    conn.execute("DELETE FROM expenses WHERE id=?", [id])?;
    db::audit(conn, Some(user.id), "expense_deleted", "expense", id, serde_json::json!({}))?;
    Ok(())
}
