use rusqlite::{params, Connection};

use crate::{db, error::Res, services::settings};

pub fn demo(conn: &Connection) -> Res<bool> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
    if n > 0 { return Ok(false); }
    let now = db::now_ts();
    for (name, user, email, role, pw) in [
        ("Administrator", "admin", "admin@vresto.com", "admin", "warmangji.adm"),
        ("Kasir", "kasir", "kasir@vresto.com", "cashier", "warmangji.ksr"),
    ] {
        conn.execute("INSERT INTO users (name,username,email,password_hash,role,created_at) VALUES (?,?,?,?,?,?)",
            params![name, user, email, bcrypt::hash(pw, 10).map_err(|e| crate::error::AppError::Internal(e.to_string()))?, role, now])?;
    }
    for (k, v) in [("restaurant_name", "Warung Demo"), ("restaurant_address", "Jl. Contoh No. 1"), ("restaurant_phone", "0812-0000-0000"),
                   ("business_type", "restoran"), ("tax_rate", "0.10"), ("service_charge_rate", "0.05"), ("packaging_fee", "2000")] {
        settings::set(conn, k, v)?;
    }
    for (i, (name, icon)) in [("Makanan", "restaurant"), ("Minuman", "local_cafe"), ("Snack", "cookie")].iter().enumerate() {
        conn.execute("INSERT INTO categories (name, icon, sort_order) VALUES (?,?,?)", params![name, icon, i as i64])?;
    }

    let products: [(i64, &str, i64, i64, Option<i64>); 8] = [
        (1, "Nasi Goreng Spesial", 25_000, 12_000, None),
        (1, "Mie Ayam", 20_000, 9_000, None),
        (1, "Ayam Bakar", 30_000, 15_000, None),
        (2, "Es Teh Manis", 5_000, 1_500, None),
        (2, "Kopi Susu", 18_000, 7_000, None),
        (2, "Frisian Flag", 12_000, 8_000, Some(24)),
        (3, "Kerupuk", 3_000, 1_000, Some(100)),
        (3, "Pisang Goreng", 10_000, 4_000, None),
    ];
    for (i, (cat, name, price, cost, stock)) in products.iter().enumerate() {
        conn.execute(
            "INSERT INTO products (uuid,category_id,name,sku,price,cost_price,is_stock_managed,stock_quantity,sort_order,created_at) VALUES (?,?,?,?,?,?,?,?,?,?)",
            params![uuid::Uuid::new_v4().to_string(), cat, name, format!("SKU-{:03}", i + 1), price, cost, stock.is_some() as i64, stock.unwrap_or(0), i as i64, now])?;
    }

    conn.execute("INSERT INTO product_variants (product_id,name,price_adjustment) VALUES (6,'Panas',0),(6,'Dingin',2000)", [])?;
    conn.execute("INSERT INTO product_modifiers (product_id,name,type,price,max_quantity) VALUES (1,'Telur Mata Sapi','topping',4000,2),(1,'Extra Pedas','level',0,1)", [])?;

    conn.execute("INSERT INTO ingredients (name,unit,current_stock,alert_limit) VALUES ('Beras','gram',20000,2000),('Telur','butir',60,12)", [])?;
    conn.execute("INSERT INTO product_ingredients (product_id,ingredient_id,quantity) VALUES (1,1,200),(1,2,1)", [])?;
    conn.execute("INSERT INTO products (uuid,category_id,name,sku,type,price,sort_order,created_at) VALUES (?,1,'Paket Hemat','SKU-BNDL','bundle',28000,99,?)", params![uuid::Uuid::new_v4().to_string(), now])?;
    let bundle_id = conn.last_insert_rowid();
    conn.execute("INSERT INTO product_bundles (parent_id,product_id,quantity) VALUES (?,1,1),(?,4,1)", params![bundle_id, bundle_id])?;
    for (i, name) in ["T1", "T2", "T3", "T4", "T5", "T6"].iter().enumerate() {
        conn.execute("INSERT INTO dining_tables (name,code,section,capacity) VALUES (?,?,?,?)", params![name, name, if i < 4 { "Indoor" } else { "Outdoor" }, if i % 2 == 0 { 4 } else { 2 }])?;
    }
    for (slug, name, icon, color) in [("cash", "Tunai", "payments", "#43A047"), ("qris", "QRIS", "qr_code", "#7B1FA2"), ("transfer", "Transfer Bank", "account_balance", "#1976D2"), ("debit", "Kartu Debit", "credit_card", "#F57C00"), ("debt", "Hutang", "receipt_long", "#5D4037")] {
        conn.execute("INSERT INTO payment_methods (slug,name,icon,color) VALUES (?,?,?,?)", params![slug, name, icon, color])?;
    }
    conn.execute("INSERT INTO customers (uuid,name,phone,credit_limit,points,created_at) VALUES (?,?,?,?,?,?)", params![uuid::Uuid::new_v4().to_string(), "Budi Santoso", "0811111111", 200_000, 50, now])?;
    Ok(true)
}
