use std::collections::HashMap;

use rusqlite::{params, Connection, ToSql};
use serde_json::{json, Value};

use crate::{auth::AuthUser, db, error::{AppError, Res}, state::Env};

#[derive(Default)]
pub struct ProductQuery { pub q: Option<String>, pub category_id: Option<i64>, pub barcode: Option<String> }

pub fn effective_price(price: i64, discount_price: Option<i64>, start: Option<&str>, end: Option<&str>, now: &str) -> i64 {
    match discount_price {
        Some(dp) if start.map_or(true, |s| s <= now) && end.map_or(true, |e| now <= e) => dp,
        _ => price,
    }
}

fn group_by_product(rows: Vec<Value>) -> HashMap<i64, Vec<Value>> {
    let mut m: HashMap<i64, Vec<Value>> = HashMap::new();
    for r in rows {
        if let Some(pid) = r.get("product_id").and_then(Value::as_i64) { m.entry(pid).or_default().push(r); }
    }
    m
}

pub fn products(conn: &Connection, q: &ProductQuery) -> Res<Vec<Value>> {
    let mut sql = String::from(
        "SELECT p.*, c.name AS category_name FROM products p LEFT JOIN categories c ON c.id = p.category_id WHERE p.is_active = 1");
    let mut args: Vec<Box<dyn ToSql>> = vec![];
    if let Some(c) = q.category_id { sql.push_str(" AND p.category_id = ?"); args.push(Box::new(c)); }
    if let Some(b) = &q.barcode { sql.push_str(" AND p.barcode = ?"); args.push(Box::new(b.clone())); }
    if let Some(s) = q.q.as_ref().filter(|s| !s.trim().is_empty()) {
        sql.push_str(" AND (p.name LIKE ? OR p.sku LIKE ? OR p.barcode LIKE ?)");
        let like = format!("%{}%", s.trim());
        for _ in 0..3 { args.push(Box::new(like.clone())); }
    }
    sql.push_str(" ORDER BY p.sort_order, p.name");
    let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let mut rows = db::query_rows(conn, &sql, &refs)?;

    let mut variants = group_by_product(db::query_rows(conn,
        "SELECT id, product_id, name, price_adjustment, is_active FROM product_variants WHERE is_active=1 ORDER BY sort_order, id", &[])?);
    let mut modifiers = group_by_product(db::query_rows(conn,
        "SELECT id, product_id, name, type, price, max_quantity, is_required, is_active FROM product_modifiers WHERE is_active=1 ORDER BY sort_order, id", &[])?);

    let now = db::now_ts();
    for p in rows.iter_mut() {
        let id = p["id"].as_i64().unwrap_or_default();
        let price = p["price"].as_i64().unwrap_or(0);
        let eff = effective_price(price, p["discount_price"].as_i64(), p["discount_start"].as_str(), p["discount_end"].as_str(), &now);
        let managed = p["is_stock_managed"].as_bool().unwrap_or(false);
        let stock = p["stock_quantity"].as_i64().unwrap_or(0);
        let min = p["min_stock"].as_i64().unwrap_or(5);
        let o = p.as_object_mut().unwrap();
        o.insert("effective_price".into(), json!(eff));
        o.insert("is_low_stock".into(), json!(managed && stock <= min));
        o.insert("variants".into(), json!(variants.remove(&id).unwrap_or_default()));
        o.insert("modifiers".into(), json!(modifiers.remove(&id).unwrap_or_default()));
    }
    Ok(rows)
}

pub fn categories(conn: &Connection) -> Res<Vec<Value>> {
    db::query_rows(conn, "SELECT id, parent_id, name, icon, color, sort_order, is_active FROM categories WHERE is_active=1 AND show_in_pos=1 ORDER BY sort_order, name", &[])
}

pub fn tables(conn: &Connection) -> Res<Vec<Value>> {
    db::query_rows(conn,
        "SELECT t.id, t.name, t.code, t.section, t.floor, t.capacity, t.status, t.current_order_id, t.is_active,
                o.order_number AS current_order_number
         FROM dining_tables t LEFT JOIN orders o ON o.id = t.current_order_id
         WHERE t.is_active = 1 ORDER BY t.section, t.name", &[])
}

pub fn payment_methods(conn: &Connection) -> Res<Vec<Value>> {
    let rows = db::query_rows(conn, "SELECT slug AS id, name, icon, color, description FROM payment_methods WHERE is_active=1 ORDER BY name", &[])?;
    if !rows.is_empty() { return Ok(rows); }
    Ok(vec![
        json!({"id":"cash","name":"Tunai","icon":"payments","color":"#43A047"}),
        json!({"id":"qris","name":"QRIS","icon":"qr_code","color":"#7B1FA2"}),
        json!({"id":"transfer","name":"Transfer Bank","icon":"account_balance","color":"#1976D2"}),
        json!({"id":"debit","name":"Kartu Debit","icon":"credit_card","color":"#F57C00"}),
        json!({"id":"debt","name":"Hutang","icon":"receipt_long","color":"#5D4037","description":"Catat sebagai piutang"}),
    ])
}

fn set_table(conn: &Connection, table_id: i64, status: &str, order_id: Option<i64>) -> Res<()> {
    conn.execute("UPDATE dining_tables SET status=?, current_order_id=? WHERE id=?", params![status, order_id, table_id])?;
    Ok(())
}

pub fn move_order_table(conn: &mut Connection, env: &Env, user: &AuthUser, order_id: i64, to_table: i64) -> Res<()> {
    let tx = conn.transaction()?;
    let (status, ot, from): (String, String, Option<i64>) = tx.query_row(
        "SELECT status, order_type, dining_table_id FROM orders WHERE id=?", [order_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    if matches!(status.as_str(), "completed" | "cancelled") { return Err(AppError::business("Pesanan sudah selesai/dibatalkan.")); }
    if ot != "dine_in" && ot != "karaoke" { return Err(AppError::business("Hanya pesanan dine-in yang bisa dipindah meja.")); }
    let (tname, tstatus): (String, String) = tx.query_row(
        "SELECT name, status FROM dining_tables WHERE id=? AND is_active=1", [to_table], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|_| AppError::not_found("Meja tujuan tidak ditemukan."))?;
    if tstatus != "available" { return Err(AppError::business(format!("Meja {tname} sedang terisi."))); }
    if let Some(f) = from { set_table(&tx, f, "available", None)?; }
    set_table(&tx, to_table, "occupied", Some(order_id))?;
    tx.execute("UPDATE orders SET dining_table_id=?, updated_at=? WHERE id=?", params![to_table, db::now_ts(), order_id])?;
    db::audit(&tx, Some(user.id), "table_moved", "order", order_id, json!({"from": from, "to": to_table}))?;
    tx.commit()?;
    if let Some(f) = from { env.emit("table.updated", json!({"id": f, "status": "available"})); }
    env.emit("table.updated", json!({"id": to_table, "status": "occupied", "current_order_id": order_id}));
    env.emit("order.updated", json!({"id": order_id}));
    Ok(())
}
