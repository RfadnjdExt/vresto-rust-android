use rusqlite::{params, Connection, ToSql};
use serde_json::{json, Value};

use crate::{auth::AuthUser, db, error::{AppError, Res}};

#[derive(Clone, Copy)]
enum K { S, I, B }

fn bind(kind: K, v: &Value) -> Option<Box<dyn ToSql>> {
    if v.is_null() { return Some(Box::new(Option::<i64>::None)); }
    match kind {
        K::S => v.as_str().map(|s| Box::new(s.trim().to_owned()) as Box<dyn ToSql>),
        K::I => v.as_i64().map(|n| Box::new(n) as Box<dyn ToSql>),
        K::B => v.as_bool().map(|b| Box::new(b as i64) as Box<dyn ToSql>),
    }
}

fn save(conn: &Connection, table: &str, id: Option<i64>, body: &Value, spec: &[(&str, K)], required: &[&str], extra_insert: Vec<(&str, Box<dyn ToSql>)>) -> Res<i64> {
    for r in required {
        let missing = match &body[*r] { Value::Null => true, Value::String(s) => s.trim().is_empty(), _ => false };
        if id.is_none() && missing { return Err(AppError::validation(format!("Field \"{r}\" wajib diisi."))); }
    }
    let mut cols: Vec<&str> = vec![];
    let mut vals: Vec<Box<dyn ToSql>> = vec![];
    for (name, kind) in spec {
        if let Some(v) = body.get(*name) {
            let b = bind(*kind, v).ok_or_else(|| AppError::validation(format!("Tipe data field \"{name}\" tidak valid.")))?;
            cols.push(name); vals.push(b);
        }
    }
    match id {
        Some(id) => {
            if cols.is_empty() { return Ok(id); }
            let set = cols.iter().map(|c| format!("{c}=?")).collect::<Vec<_>>().join(",");
            vals.push(Box::new(id));
            let refs: Vec<&dyn ToSql> = vals.iter().map(|b| b.as_ref()).collect();
            if conn.execute(&format!("UPDATE {table} SET {set} WHERE id=?"), refs.as_slice())? == 0 { return Err(AppError::not_found("Data tidak ditemukan.")); }
            Ok(id)
        }
        None => {
            for (c, v) in extra_insert { cols.push(c); vals.push(v); }
            let ph = vec!["?"; cols.len()].join(",");
            let refs: Vec<&dyn ToSql> = vals.iter().map(|b| b.as_ref()).collect();
            conn.execute(&format!("INSERT INTO {table} ({}) VALUES ({ph})", cols.join(",")), refs.as_slice())?;
            Ok(conn.last_insert_rowid())
        }
    }
}

fn nonneg(body: &Value, keys: &[&str]) -> Res<()> {
    for k in keys { if body[*k].as_i64().map_or(false, |v| v < 0) { return Err(AppError::validation(format!("\"{k}\" tidak boleh negatif."))); } }
    Ok(())
}

pub fn save_category(conn: &Connection, user: &AuthUser, id: Option<i64>, body: &Value) -> Res<Value> {
    user.require_admin()?;
    let id = save(conn, "categories", id, body,
        &[("name", K::S), ("parent_id", K::I), ("icon", K::S), ("color", K::S), ("sort_order", K::I), ("is_active", K::B), ("show_in_pos", K::B)], &["name"], vec![])?;
    Ok(db::query_one(conn, "SELECT * FROM categories WHERE id=?", &[&id])?.unwrap_or(Value::Null))
}

pub fn save_table(conn: &Connection, user: &AuthUser, id: Option<i64>, body: &Value) -> Res<Value> {
    user.require_admin()?;
    nonneg(body, &["capacity"])?;
    let id = save(conn, "dining_tables", id, body,
        &[("name", K::S), ("code", K::S), ("section", K::S), ("floor", K::S), ("capacity", K::I), ("is_active", K::B)], &["name"], vec![])?;
    Ok(db::query_one(conn, "SELECT * FROM dining_tables WHERE id=?", &[&id])?.unwrap_or(Value::Null))
}

pub fn set_table_status(conn: &Connection, id: i64, status: &str) -> Res<()> {
    if !matches!(status, "available" | "occupied" | "reserved") { return Err(AppError::validation("Status meja tidak valid.")); }
    let occupied_by: Option<i64> = conn.query_row("SELECT current_order_id FROM dining_tables WHERE id=?", [id], |r| r.get(0))
        .map_err(|_| AppError::not_found("Meja tidak ditemukan."))?;
    if status == "available" && occupied_by.is_some() {
        return Err(AppError::business("Meja masih dipakai pesanan aktif. Selesaikan atau batalkan pesanannya dulu."));
    }
    conn.execute("UPDATE dining_tables SET status=? WHERE id=?", params![status, id])?;
    Ok(())
}

pub fn save_product(conn: &mut Connection, user: &AuthUser, id: Option<i64>, body: &Value) -> Res<Value> {
    user.require_admin()?;
    nonneg(body, &["price", "cost_price", "stock_quantity"])?;
    let tx = conn.transaction()?;
    let id = save(&tx, "products", id, body,
        &[("category_id", K::I), ("name", K::S), ("sku", K::S), ("barcode", K::S), ("type", K::S), ("price", K::I), ("cost_price", K::I),
          ("discount_price", K::I), ("discount_start", K::S), ("discount_end", K::S), ("is_stock_managed", K::B), ("stock_quantity", K::I),
          ("min_stock", K::I), ("image", K::S), ("station", K::S), ("is_active", K::B), ("sort_order", K::I)],
        &["name", "price"],
        vec![("uuid", Box::new(uuid::Uuid::new_v4().to_string())), ("created_at", Box::new(db::now_ts()))])?;

    if let Some(vs) = body["variants"].as_array() {
        tx.execute("DELETE FROM product_variants WHERE product_id=?", [id])?;
        for (i, v) in vs.iter().enumerate() {
            let name = v["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| AppError::validation("Nama varian wajib diisi."))?;
            tx.execute("INSERT INTO product_variants (product_id,name,price_adjustment,sort_order) VALUES (?,?,?,?)",
                params![id, name, v["price_adjustment"].as_i64().unwrap_or(0), i as i64])?;
        }
    }
    if let Some(ms) = body["modifiers"].as_array() {
        tx.execute("DELETE FROM product_modifiers WHERE product_id=?", [id])?;
        for (i, m) in ms.iter().enumerate() {
            let name = m["name"].as_str().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| AppError::validation("Nama modifier wajib diisi."))?;
            tx.execute("INSERT INTO product_modifiers (product_id,name,type,price,max_quantity,is_required,sort_order) VALUES (?,?,?,?,?,?,?)",
                params![id, name, m["type"].as_str(), m["price"].as_i64().unwrap_or(0), m["max_quantity"].as_i64().unwrap_or(1).max(1),
                        m["is_required"].as_bool().unwrap_or(false) as i64, i as i64])?;
        }
    }
    db::audit(&tx, Some(user.id), "saved", "product", id, json!({"name": body["name"]}))?;
    let out = db::query_one(&tx, "SELECT * FROM products WHERE id=?", &[&id])?.unwrap_or(Value::Null);
    tx.commit()?;
    Ok(out)
}

pub fn deactivate_product(conn: &Connection, user: &AuthUser, id: i64) -> Res<()> {
    user.require_admin()?;
    if conn.execute("UPDATE products SET is_active=0 WHERE id=?", [id])? == 0 { return Err(AppError::not_found("Produk tidak ditemukan.")); }
    Ok(())
}

pub fn adjust_stock(conn: &mut Connection, user: &AuthUser, product_id: i64, delta: i64, notes: Option<&str>) -> Res<Value> {
    if delta == 0 { return Err(AppError::validation("Perubahan stok tidak boleh 0.")); }
    let tx = conn.transaction()?;
    let managed: i64 = tx.query_row("SELECT is_stock_managed FROM products WHERE id=?", [product_id], |r| r.get(0)).map_err(|_| AppError::not_found("Produk tidak ditemukan."))?;
    if managed != 1 { return Err(AppError::business("Produk ini tidak memakai manajemen stok.")); }
    tx.execute("UPDATE products SET stock_quantity = stock_quantity + ? WHERE id=?", params![delta, product_id])?;
    let after: i64 = tx.query_row("SELECT stock_quantity FROM products WHERE id=?", [product_id], |r| r.get(0))?;
    tx.execute("INSERT INTO inventory_transactions (product_id,user_id,type,quantity,stock_after,notes,created_at) VALUES (?,?,?,?,?,?,?)",
        params![product_id, user.id, "adjustment", delta as f64, after as f64, notes.unwrap_or("Penyesuaian stok"), db::now_ts()])?;
    db::audit(&tx, Some(user.id), "stock_adjusted", "product", product_id, json!({"delta": delta, "after": after}))?;
    tx.commit()?;
    Ok(json!({"product_id": product_id, "stock_quantity": after}))
}

pub fn stock_history(conn: &Connection, product_id: i64) -> Res<Vec<Value>> {
    db::query_rows(conn, "SELECT * FROM inventory_transactions WHERE product_id=? ORDER BY id DESC LIMIT 200", &[&product_id])
}

pub fn list_users(conn: &Connection, user: &AuthUser) -> Res<Vec<Value>> {
    user.require_admin()?;
    db::query_rows(conn, "SELECT id, name, username, email, role, phone, is_active, last_login_at FROM users ORDER BY name", &[])
}

pub fn save_user(conn: &Connection, user: &AuthUser, id: Option<i64>, body: &Value) -> Res<Value> {
    user.require_admin()?;
    if let Some(r) = body["role"].as_str() { if !matches!(r, "admin" | "cashier") { return Err(AppError::validation("Role harus admin atau cashier.")); } }
    if id == Some(user.id) && body["is_active"].as_bool() == Some(false) { return Err(AppError::business("Tidak bisa menonaktifkan akun sendiri.")); }
    let pw = body["password"].as_str();
    if let Some(p) = pw { if p.len() < 6 { return Err(AppError::validation("Password minimal 6 karakter.")); } }
    if id.is_none() && pw.is_none() { return Err(AppError::validation("Field \"password\" wajib diisi.")); }
    let mut extra: Vec<(&str, Box<dyn ToSql>)> = vec![("created_at", Box::new(db::now_ts()))];
    let hash = pw.map(|p| bcrypt::hash(p, 10)).transpose().map_err(|e| AppError::Internal(e.to_string()))?;
    if let (None, Some(h)) = (id, &hash) { extra.push(("password_hash", Box::new(h.clone()))); }
    let id = save(conn, "users", id, body,
        &[("name", K::S), ("username", K::S), ("email", K::S), ("role", K::S), ("phone", K::S), ("is_active", K::B)],
        &["name", "username", "role"], extra)?;
    if let Some(h) = hash { conn.execute("UPDATE users SET password_hash=? WHERE id=?", params![h, id])?; }
    Ok(db::query_one(conn, "SELECT id, name, username, email, role, phone, is_active FROM users WHERE id=?", &[&id])?.unwrap_or(Value::Null))
}

pub fn save_setup(conn: &Connection, user: &AuthUser, body: &Value) -> Res<Value> {
    user.require_admin()?;
    use super::settings::{get, set};
    if get(conn, "setup_locked")?.as_deref() == Some("true") { return Err(AppError::conflict("Setup sudah dikunci.")); }
    let bt = body["business_type"].as_str().unwrap_or("");
    if !matches!(bt, "warung" | "cafe" | "restoran") { return Err(AppError::validation("business_type harus warung, cafe, atau restoran.")); }
    set(conn, "business_type", bt)?;
    for (k, dst) in [("name", "restaurant_name"), ("address", "restaurant_address"), ("phone", "restaurant_phone")] {
        if let Some(v) = body[k].as_str() { set(conn, dst, v.trim())?; }
    }
    for k in ["tax_rate", "service_charge_rate"] {
        if let Some(v) = body[k].as_f64() {
            if !(0.0..=1.0).contains(&v) { return Err(AppError::validation(format!("\"{k}\" harus pecahan 0–1, mis. 0.10."))); }
            set(conn, k, &v.to_string())?;
        }
    }
    set(conn, "setup_locked", "true")?;
    super::settings::shop(conn)
}

pub fn save_settings(conn: &Connection, user: &AuthUser, body: &Value) -> Res<Value> {
    user.require_admin()?;
    use super::settings::{get_bool, set};
    let obj = body.as_object().filter(|o| !o.is_empty()).ok_or_else(|| AppError::validation("Kirim objek JSON berisi pengaturan yang diubah."))?;
    let mut writes: Vec<(&str, String)> = vec![];
    for (k, v) in obj {
        let (key, val) = match k.as_str() {
            "apply_tax" | "apply_service_charge_dine_in" | "apply_packaging_fee" | "enable_split_bill" | "refund_requires_admin" => {
                let b = v.as_bool().ok_or_else(|| AppError::validation(format!("\"{k}\" harus true/false.")))?;
                (k.as_str(), b.to_string())
            }
            "tax_rate" | "service_charge_rate" => {
                let f = v.as_f64().filter(|f| (0.0..=1.0).contains(f)).ok_or_else(|| AppError::validation(format!("\"{k}\" harus pecahan 0–1, mis. 0.10.")))?;
                (k.as_str(), f.to_string())
            }
            "packaging_fee" => {
                let n = v.as_i64().filter(|n| *n >= 0).ok_or_else(|| AppError::validation("\"packaging_fee\" harus bilangan bulat ≥ 0."))?;
                (k.as_str(), n.to_string())
            }
            other => return Err(AppError::validation(format!("Pengaturan tidak dikenal: {other}"))),
        };
        writes.push((key, val));
    }
    for (k, v) in &writes { set(conn, k, v)?; }
    db::audit(conn, Some(user.id), "settings_changed", "settings", 0, body.clone())?;
    Ok(json!({ "billing": super::settings::billing_json(conn)?, "refund_requires_admin": get_bool(conn, "refund_requires_admin", true)? }))
}
