use rusqlite::{params, Connection, OptionalExtension, ToSql};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{catalog, customers, settings, shifts, stock};
use crate::{
    auth::AuthUser,
    db,
    error::{AppError, Res},
    money::{self, Discount, OrderType, Totals},
    state::Env,
};

fn one() -> i64 { 1 }

#[derive(Deserialize, Clone, Debug)]
pub struct ModifierIn { pub id: i64, #[serde(default = "one")] pub quantity: i64 }

#[derive(Deserialize, Clone, Debug)]
pub struct ItemIn {
    pub product_id: i64,
    pub variant_id: Option<i64>,
    #[serde(default = "one")]
    pub qty: i64,
    pub note: Option<String>,
    #[serde(default)]
    pub modifiers: Vec<ModifierIn>,
    #[serde(default)]
    pub takeaway: bool,
}

#[derive(Deserialize, Default, Clone, Debug)]
pub struct OrderIn {
    #[serde(default)]
    pub items: Vec<ItemIn>,
    pub order_type: Option<String>,
    pub table_id: Option<i64>,
    pub customer_id: Option<i64>,
    pub customer_name: Option<String>,
    pub guest_count: Option<i64>,

    pub discount_type: Option<String>,
    pub discount_value: Option<f64>,
    pub shipping_fee: Option<i64>,
    pub points_used: Option<i64>,
    pub notes: Option<String>,
    pub kitchen_notes: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct PayIn {

    pub client_uuid: Option<String>,
    pub order_id: Option<i64>,

    pub order: Option<OrderIn>,
    pub payment_method: String,
    pub amount_paid: i64,
    pub customer_id: Option<i64>,
    pub notes: Option<String>,
}

struct Priced {
    product_id: i64,
    variant_id: Option<i64>,
    name: String,
    sku: Option<String>,
    qty: i64,
    unit_price: i64,
    cost_price: i64,
    modifiers: Vec<(i64, String, i64, i64)>,
    modifiers_total: i64,
    note: Option<String>,
    takeaway: bool,
}

impl Priced {
    fn line_total(&self) -> i64 { (self.unit_price + self.modifiers_total) * self.qty }
}

fn price_items(conn: &Connection, items: &[ItemIn]) -> Res<Vec<Priced>> {
    if items.is_empty() { return Err(AppError::validation("Keranjang kosong.")); }
    let now = db::now_ts();
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        if it.qty < 1 || it.qty > 999 { return Err(AppError::validation("Jumlah item harus 1–999.")); }
        let p = conn
            .query_row(
                "SELECT name, sku, price, cost_price, discount_price, discount_start, discount_end, is_active FROM products WHERE id=?",
                [it.product_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?,
                        r.get::<_, Option<i64>>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, Option<String>>(6)?, r.get::<_, i64>(7)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::validation(format!("Produk #{} tidak ditemukan.", it.product_id)))?;
        if p.7 != 1 { return Err(AppError::business(format!("Produk \"{}\" sedang tidak tersedia.", p.0))); }

        let mut unit = catalog::effective_price(p.2, p.4, p.5.as_deref(), p.6.as_deref(), &now);
        let mut name = p.0.clone();
        if let Some(vid) = it.variant_id {
            let (vname, adj): (String, i64) = conn
                .query_row("SELECT name, price_adjustment FROM product_variants WHERE id=? AND product_id=? AND is_active=1", params![vid, it.product_id],
                    |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|_| AppError::validation(format!("Varian tidak valid untuk \"{}\".", p.0)))?;
            unit += adj;
            name = format!("{} ({})", p.0, vname);
        }
        let mut mods = vec![];
        let mut mods_total = 0;
        for m in &it.modifiers {
            let (mname, price, max): (String, i64, i64) = conn
                .query_row("SELECT name, price, max_quantity FROM product_modifiers WHERE id=? AND product_id=? AND is_active=1", params![m.id, it.product_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .map_err(|_| AppError::validation(format!("Modifier tidak valid untuk \"{}\".", p.0)))?;
            if m.quantity < 1 || m.quantity > max {
                return Err(AppError::validation(format!("Jumlah modifier \"{mname}\" maksimal {max}.")));
            }
            mods_total += price * m.quantity;
            mods.push((m.id, mname, m.quantity, price));
        }
        out.push(Priced {
            product_id: it.product_id, variant_id: it.variant_id, name, sku: p.1, qty: it.qty, unit_price: unit, cost_price: p.3,
            modifiers: mods, modifiers_total: mods_total, note: it.note.clone().filter(|n| !n.trim().is_empty()), takeaway: it.takeaway,
        });
    }
    Ok(out)
}

fn totals_for(conn: &Connection, input: &OrderIn, ot: OrderType, priced: &[Priced]) -> Res<(Totals, Discount, money::Billing)> {
    let billing = settings::billing(conn)?;
    let subtotal: i64 = priced.iter().map(Priced::line_total).sum();
    let discount = Discount::from_parts(input.discount_type.as_deref(), input.discount_value);
    let points = input.points_used.unwrap_or(0);
    if points < 0 { return Err(AppError::validation("Poin tidak boleh negatif.")); }
    if points > 0 {
        let cid = input.customer_id.ok_or_else(|| AppError::business("Pelanggan wajib dipilih untuk memakai poin."))?;
        let have: i64 = conn.query_row("SELECT points FROM customers WHERE id=?", [cid], |r| r.get(0))
            .map_err(|_| AppError::not_found("Pelanggan tidak ditemukan."))?;
        if have < points { return Err(AppError::business(format!("Poin tidak mencukupi (Butuh: {points}, Tersedia: {have})"))); }
    }
    Ok((money::calculate(&billing, subtotal, ot, discount, input.shipping_fee.unwrap_or(0), points), discount, billing))
}

fn parse_type(s: Option<&str>) -> Res<OrderType> {
    match s { None => Ok(OrderType::DineIn), Some(s) => OrderType::parse(s).ok_or_else(|| AppError::validation(format!("Tipe order tidak dikenal: {s}"))) }
}

fn next_seq(conn: &Connection, table: &str, col: &str, prefix: &str) -> Res<i64> {
    let last: Option<String> = conn
        .query_row(&format!("SELECT {col} FROM {table} WHERE {col} LIKE ? ORDER BY id DESC LIMIT 1"), [format!("{prefix}%")], |r| r.get(0))
        .optional()?;
    Ok(last.and_then(|n| n.rsplit('-').next().and_then(|s| s.parse::<i64>().ok())).unwrap_or(0) + 1)
}

fn next_queue_number(conn: &Connection, env: &Env) -> Res<i64> {
    let today = db::local_date(db::now(), env.tz_offset_min).to_string();
    conn.execute("INSERT INTO queue_counters(date,last_number) VALUES(?,1) ON CONFLICT(date) DO UPDATE SET last_number = last_number + 1", [&today])?;
    Ok(conn.query_row("SELECT last_number FROM queue_counters WHERE date=?", [&today], |r| r.get(0))?)
}

fn insert_items(conn: &Connection, order_id: i64, priced: &[Priced]) -> Res<()> {
    for it in priced {
        conn.execute(
            "INSERT INTO order_items (order_id, product_id, product_variant_id, product_name, product_sku, quantity, unit_price, cost_price,
                                      modifiers_total, total_price, note, is_takeaway) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            params![order_id, it.product_id, it.variant_id, it.name, it.sku, it.qty, it.unit_price, it.cost_price,
                    it.modifiers_total, it.line_total(), it.note, it.takeaway as i64])?;
        let item_id = conn.last_insert_rowid();
        for (mid, mname, q, price) in &it.modifiers {
            conn.execute("INSERT INTO order_item_modifiers (order_item_id, product_modifier_id, modifier_name, quantity, unit_price, total_price) VALUES (?,?,?,?,?,?)",
                params![item_id, mid, mname, q, price, price * q])?;
        }
    }
    Ok(())
}

pub fn create_order(conn: &Connection, env: &Env, user: &AuthUser, input: &OrderIn, status: &str) -> Res<i64> {
    let ot = parse_type(input.order_type.as_deref())?;
    let priced = price_items(conn, &input.items)?;
    let (t, discount, billing) = totals_for(conn, input, ot, &priced)?;

    let table_id = if matches!(ot, OrderType::DineIn | OrderType::Karaoke) { input.table_id } else { None };
    if let Some(tid) = table_id {
        let (name, st): (String, String) = conn
            .query_row("SELECT name, status FROM dining_tables WHERE id=? AND is_active=1", [tid], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|_| AppError::validation("Meja tidak ditemukan."))?;
        if st != "available" { return Err(AppError::business(format!("Meja {name} sedang terisi."))); }
    }

    let now = db::now();
    let date = db::local_date(now, env.tz_offset_min).format("%Y%m%d").to_string();
    let prefix = format!("{}-{date}-", ot.prefix());
    let number = format!("{prefix}{:04}", next_seq(conn, "orders", "order_number", &prefix)?);
    let queue = if ot == OrderType::TakeAway { Some(next_queue_number(conn, env)?) } else { None };
    let (dtype, dvalue) = match discount { Discount::None => (None, None), Discount::Fixed { value } => (Some("fixed"), Some(value as f64)), Discount::Percent { value } => (Some("percent"), Some(value)) };
    let shift_id = shifts::open_shift_id(conn, user.id)?;
    let ts = db::ts(now);

    conn.execute(
        "INSERT INTO orders (uuid, order_number, user_id, shift_id, dining_table_id, customer_id, customer_name, guest_count, order_type, queue_number,
            subtotal, discount_type, discount_value, discount_amount, tax_amount, tax_rate_bps, service_charge, service_charge_bps, packaging_fee, shipping_fee,
            points_used, total_amount, status, payment_status, notes, kitchen_notes, created_at, updated_at)
         VALUES (?,?,?,?,?,?,?,?,?,?, ?,?,?,?,?,?,?,?,?,?, ?,?,?,'unpaid',?,?,?,?)",
        params![uuid::Uuid::new_v4().to_string(), number, user.id, shift_id, table_id, input.customer_id,
                input.customer_name.as_deref().map(str::trim).filter(|s| !s.is_empty()), input.guest_count.unwrap_or(1).max(1), ot.as_str(), queue,
                t.subtotal, dtype, dvalue, t.discount_amount, t.tax_amount, if billing.apply_tax { billing.tax_bps } else { 0 },
                t.service_charge, if t.service_charge > 0 { billing.service_bps } else { 0 }, t.packaging_fee, t.shipping_fee,
                input.points_used.unwrap_or(0), t.total, status, input.notes, input.kitchen_notes, ts, ts],
    )?;
    let id = conn.last_insert_rowid();
    insert_items(conn, id, &priced)?;
    if let Some(tid) = table_id {
        conn.execute("UPDATE dining_tables SET status='occupied', current_order_id=? WHERE id=?", params![id, tid])?;
    }
    db::audit(conn, Some(user.id), "created", "order", id, json!({"order_number": number, "total": t.total}))?;
    Ok(id)
}

pub fn update_order(conn: &mut Connection, env: &Env, user: &AuthUser, id: i64, input: &OrderIn) -> Res<Value> {
    let tx = conn.transaction()?;
    let (status, pay, otype, cust): (String, String, String, Option<i64>) = tx
        .query_row("SELECT status, payment_status, order_type, customer_id FROM orders WHERE id=?", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(|_| AppError::not_found("Pesanan tidak ditemukan."))?;
    if matches!(status.as_str(), "completed" | "cancelled") { return Err(AppError::business("Pesanan sudah selesai/dibatalkan.")); }
    if pay != "unpaid" { return Err(AppError::business("Pesanan sudah dibayar; buat pesanan tambahan untuk item baru.")); }

    let ot = OrderType::parse(&otype).unwrap_or(OrderType::DineIn);
    let mut input = input.clone();
    input.customer_id = input.customer_id.or(cust);
    let priced = price_items(&tx, &input.items)?;
    let (t, discount, _) = totals_for(&tx, &input, ot, &priced)?;
    let (dtype, dvalue) = match discount { Discount::None => (None, None), Discount::Fixed { value } => (Some("fixed"), Some(value as f64)), Discount::Percent { value } => (Some("percent"), Some(value)) };

    tx.execute("DELETE FROM order_items WHERE id IN (SELECT id FROM order_items WHERE order_id=?)", [id])?;
    insert_items(&tx, id, &priced)?;
    tx.execute(
        "UPDATE orders SET subtotal=?, discount_type=?, discount_value=?, discount_amount=?, tax_amount=?, service_charge=?, packaging_fee=?, shipping_fee=?,
           points_used=?, total_amount=?, customer_id=?, customer_name=COALESCE(?, customer_name), guest_count=COALESCE(?, guest_count),
           notes=COALESCE(?, notes), kitchen_notes=COALESCE(?, kitchen_notes), updated_at=? WHERE id=?",
        params![t.subtotal, dtype, dvalue, t.discount_amount, t.tax_amount, t.service_charge, t.packaging_fee, t.shipping_fee,
                input.points_used.unwrap_or(0), t.total, input.customer_id, input.customer_name, input.guest_count, input.notes, input.kitchen_notes, db::now_ts(), id])?;
    db::audit(&tx, Some(user.id), "updated", "order", id, json!({"total": t.total}))?;
    let out = get_order(&tx, id)?;
    tx.commit()?;
    env.emit("order.updated", json!({"id": id}));
    Ok(out)
}

pub fn normalize_method(m: &str) -> Res<&'static str> {
    Ok(match m.trim().to_lowercase().as_str() {
        "cash" | "tunai" => "cash",
        "transfer" | "bank_transfer" | "transfer_bank" => "transfer",
        "qris" | "qr" => "qris",
        "debit" | "debit_card" | "kartu_debit" => "debit",
        "credit" | "credit_card" | "kartu_kredit" => "credit",
        "card" => "card",
        "hutang" | "debt" | "piutang" => "debt",
        "ewallet" | "e-wallet" | "gopay" | "ovo" | "dana" | "shopeepay" => "ewallet",
        "points" | "poin" | "loyalty" => "points",

        other => return Err(AppError::validation(format!("Metode pembayaran tidak dikenal: {other}"))),
    })
}

fn payment_result(conn: &Connection, trx_id: i64) -> Res<Value> {
    db::query_one(conn,
        "SELECT t.id AS transaction_id, t.trx_number, t.order_id, o.order_number, t.total_amount, t.amount_paid, t.change_amount,
                t.payment_method, t.status, t.paid_at, o.queue_number, o.order_type
         FROM transactions t LEFT JOIN orders o ON o.id=t.order_id WHERE t.id=?", &[&trx_id])?
        .ok_or_else(|| AppError::not_found("Transaksi tidak ditemukan."))
}

pub fn pay(conn: &mut Connection, env: &Env, user: &AuthUser, input: &PayIn) -> Res<Value> {

    if let Some(cu) = input.client_uuid.as_deref() {
        if let Some(tid) = conn.query_row("SELECT id FROM transactions WHERE client_uuid=?", [cu], |r| r.get::<_, i64>(0)).optional()? {
            return payment_result(conn, tid);
        }
    }
    let method = normalize_method(&input.payment_method)?;
    let shift_id = shifts::open_shift_id(conn, user.id)?
        .ok_or_else(|| AppError::conflict("Shift belum dibuka. Buka shift terlebih dahulu."))?;
    if input.amount_paid < 0 { return Err(AppError::validation("Jumlah bayar tidak valid.")); }

    let tx = conn.transaction()?;
    let order_id = match (input.order_id, &input.order) {
        (Some(id), _) => id,
        (None, Some(o)) => create_order(&tx, env, user, o, "preparing")?,
        (None, None) => return Err(AppError::validation("order_id atau order wajib diisi.")),
    };
    let (status, o_total, o_sub, o_tax, o_disc, o_svc, o_cust, o_points): (String, i64, i64, i64, i64, i64, Option<i64>, i64) = tx
        .query_row("SELECT status, total_amount, subtotal, tax_amount, discount_amount, service_charge, customer_id, points_used FROM orders WHERE id=?", [order_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)))
        .map_err(|_| AppError::not_found("Pesanan tidak ditemukan."))?;
    if matches!(status.as_str(), "completed" | "cancelled") { return Err(AppError::business("Pesanan sudah selesai/dibatalkan.")); }
    let already: i64 = tx.query_row("SELECT COUNT(*) FROM transactions WHERE order_id=? AND status IN ('confirmed','unpaid','partial_refund')", [order_id], |r| r.get(0))?;
    if already > 0 { return Err(AppError::business("Pesanan ini sudah dibayar.")); }

    let customer_id = o_cust.or(input.customer_id);
    let total = o_total;
    let (trx_status, paid, change, paid_at) = if method == "debt" {
        let cid = customer_id.ok_or_else(|| AppError::business("Customer wajib dipilih untuk pembayaran hutang."))?;
        let (debt, limit): (i64, i64) = tx.query_row("SELECT total_debt, credit_limit FROM customers WHERE id=?", [cid], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|_| AppError::not_found("Pelanggan tidak ditemukan."))?;
        if debt + total > limit { return Err(AppError::business("Limit hutang tidak mencukupi.")); }
        ("unpaid", 0, 0, None)
    } else if method == "points" {
        let cid = customer_id.ok_or_else(|| AppError::business("Customer wajib dipilih untuk pembayaran poin."))?;
        let need = (total + money::POINT_VALUE - 1) / money::POINT_VALUE;
        let have: i64 = tx.query_row("SELECT points FROM customers WHERE id=?", [cid], |r| r.get(0)).map_err(|_| AppError::not_found("Pelanggan tidak ditemukan."))?;
        if have < need { return Err(AppError::business(format!("Poin tidak mencukupi (Butuh: {need}, Tersedia: {have})"))); }
        tx.execute("UPDATE customers SET points = points - ? WHERE id=?", params![need, cid])?;
        ("confirmed", total, 0, Some(db::now_ts()))
    } else {
        if input.amount_paid < total { return Err(AppError::business(format!("Jumlah bayar kurang Rp{}.", total - input.amount_paid))); }
        ("confirmed", input.amount_paid, input.amount_paid - total, Some(db::now_ts()))
    };

    if o_points > 0 {
        let cid = customer_id.ok_or_else(|| AppError::business("Pelanggan wajib dipilih untuk memakai poin."))?;
        let have: i64 = tx.query_row("SELECT points FROM customers WHERE id=?", [cid], |r| r.get(0))?;
        if have < o_points { return Err(AppError::business(format!("Poin tidak mencukupi (Butuh: {o_points}, Tersedia: {have})"))); }
        tx.execute("UPDATE customers SET points = points - ? WHERE id=?", params![o_points, cid])?;
    }

    let now = db::now();
    let date = db::local_date(now, env.tz_offset_min).format("%Y%m%d").to_string();
    let prefix = format!("TRX-{date}-");
    let trx_no = format!("{prefix}{:04}", next_seq(&tx, "transactions", "trx_number", &prefix)?);
    tx.execute(
        "INSERT INTO transactions (uuid, client_uuid, trx_number, order_id, user_id, shift_id, customer_id, payment_method, subtotal, tax_amount, discount_amount,
            service_charge, total_amount, amount_paid, change_amount, remaining_refundable, status, notes, paid_at, created_at)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![uuid::Uuid::new_v4().to_string(), input.client_uuid, trx_no, order_id, user.id, shift_id, customer_id, method, o_sub, o_tax, o_disc,
                o_svc, total, paid, change, total, trx_status, input.notes, paid_at, db::ts(now)])?;
    let trx_id = tx.last_insert_rowid();

    if method == "debt" {
        let days: i64 = tx.query_row("SELECT default_due_days FROM customers WHERE id=?", [customer_id.unwrap()], |r| r.get(0))?;
        let due = db::local_date(now + chrono::Duration::days(days), env.tz_offset_min).to_string();
        tx.execute("INSERT INTO debts (customer_id, order_id, transaction_id, amount, remaining_amount, due_date, status, notes, created_at) VALUES (?,?,?,?,?,?,'OPEN',?,?)",
            params![customer_id, order_id, trx_id, total, total, due, input.notes, db::ts(now)])?;
        tx.execute("UPDATE customers SET total_debt = total_debt + ? WHERE id=?", params![total, customer_id])?;
    }
    if let Some(cid) = customer_id { if method != "points" { customers::record_visit(&tx, cid, total)?; } }

    tx.execute(
        "UPDATE orders SET payment_status='paid', customer_id=?, shift_id=COALESCE(shift_id, ?),
            status = CASE WHEN status IN ('draft','pending','confirmed') THEN 'preparing' ELSE status END, updated_at=? WHERE id=?",
        params![customer_id, shift_id, db::now_ts(), order_id])?;
    db::audit(&tx, Some(user.id), "paid", "transaction", trx_id, json!({"order_id": order_id, "method": method, "total": total}))?;
    let out = payment_result(&tx, trx_id)?;
    let table: Option<i64> = tx.query_row("SELECT dining_table_id FROM orders WHERE id=?", [order_id], |r| r.get(0))?;
    tx.commit()?;

    if let Some(t) = table { env.emit("table.updated", json!({"id": t, "status": "occupied", "current_order_id": order_id})); }
    env.emit("order.updated", json!({"id": order_id}));
    env.emit("stats.updated", json!({}));
    Ok(out)
}

pub fn free_table(conn: &Connection, env: &Env, order_id: i64) -> Res<()> {
    let ids: Vec<i64> = {
        let mut st = conn.prepare("SELECT id FROM dining_tables WHERE current_order_id=?")?;
        let rows = st.query_map([order_id], |r| r.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    conn.execute("UPDATE dining_tables SET status='available', current_order_id=NULL WHERE current_order_id=?", [order_id])?;
    for id in ids { env.emit("table.updated", json!({"id": id, "status": "available"})); }
    Ok(())
}

pub fn complete(conn: &mut Connection, env: &Env, user: &AuthUser, id: i64) -> Res<Value> {
    let tx = conn.transaction()?;
    let (status, pay): (String, String) = tx.query_row("SELECT status, payment_status FROM orders WHERE id=?", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|_| AppError::not_found("Pesanan tidak ditemukan."))?;
    if !matches!(status.as_str(), "preparing" | "confirmed" | "ready" | "served") {
        return Err(AppError::business("Order tidak dalam status yang valid untuk diselesaikan"));
    }
    if pay != "paid" { return Err(AppError::business("Pesanan belum dibayar.")); }
    tx.execute("UPDATE orders SET status='completed', completed_at=?, completed_by=?, updated_at=? WHERE id=?", params![db::now_ts(), user.id, db::now_ts(), id])?;
    stock::deduct_for_order(&tx, id, user.id)?;
    free_table(&tx, env, id)?;
    db::audit(&tx, Some(user.id), "completed", "order", id, json!({}))?;
    let out = get_order(&tx, id)?;
    tx.commit()?;
    env.emit("order.updated", json!({"id": id}));
    env.emit("stats.updated", json!({}));
    Ok(out)
}

pub fn cancel(conn: &mut Connection, env: &Env, user: &AuthUser, id: i64, reason: Option<&str>) -> Res<Value> {
    let tx = conn.transaction()?;
    let (status, pay): (String, String) = tx.query_row("SELECT status, payment_status FROM orders WHERE id=?", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|_| AppError::not_found("Pesanan tidak ditemukan."))?;
    if matches!(status.as_str(), "completed" | "cancelled") { return Err(AppError::business("Pesanan sudah selesai/dibatalkan.")); }
    if pay != "unpaid" {
        return Err(AppError::business("Pesanan sudah dibayar. Gunakan void (di shift yang sama, sebelum selesai) atau refund."));
    }
    tx.execute("UPDATE orders SET status='cancelled', cancelled_at=?, cancel_reason=?, updated_at=? WHERE id=?", params![db::now_ts(), reason, db::now_ts(), id])?;
    free_table(&tx, env, id)?;
    db::audit(&tx, Some(user.id), "cancelled", "order", id, json!({"reason": reason}))?;
    let out = get_order(&tx, id)?;
    tx.commit()?;
    env.emit("order.updated", json!({"id": id}));
    Ok(out)
}

pub fn set_status(conn: &mut Connection, env: &Env, user: &AuthUser, id: i64, new: &str) -> Res<Value> {
    let allowed_from: &[&str] = match new {
        "confirmed" => &["pending", "draft"],
        "preparing" => &["confirmed", "pending"],
        "ready" => &["preparing"],
        "served" => &["ready", "preparing"],
        _ => return Err(AppError::validation("Status tidak valid (gunakan endpoint complete/cancel untuk status akhir).")),
    };
    let tx = conn.transaction()?;
    let cur: String = tx.query_row("SELECT status FROM orders WHERE id=?", [id], |r| r.get(0)).map_err(|_| AppError::not_found("Pesanan tidak ditemukan."))?;
    if !allowed_from.contains(&cur.as_str()) { return Err(AppError::business(format!("Tidak bisa mengubah status dari {cur} ke {new}."))); }
    tx.execute("UPDATE orders SET status=?, updated_at=? WHERE id=?", params![new, db::now_ts(), id])?;
    db::audit(&tx, Some(user.id), "status", "order", id, json!({"from": cur, "to": new}))?;
    let out = get_order(&tx, id)?;
    tx.commit()?;
    env.emit("order.updated", json!({"id": id}));
    Ok(out)
}

const ORDER_SELECT: &str =
    "SELECT o.*, t.name AS table_name, COALESCE(o.customer_name, c.name) AS customer_display
     FROM orders o LEFT JOIN dining_tables t ON t.id=o.dining_table_id LEFT JOIN customers c ON c.id=o.customer_id";

fn attach_items(conn: &Connection, orders: &mut [Value]) -> Res<()> {
    for o in orders.iter_mut() {
        let id = o["id"].as_i64().unwrap_or_default();
        let mut items = db::query_rows(conn, "SELECT * FROM order_items WHERE order_id=? ORDER BY id", &[&id])?;
        for it in items.iter_mut() {
            let iid = it["id"].as_i64().unwrap_or_default();
            let mods = db::query_rows(conn, "SELECT modifier_name, quantity, unit_price, total_price FROM order_item_modifiers WHERE order_item_id=?", &[&iid])?;
            it.as_object_mut().unwrap().insert("modifiers".into(), json!(mods));
        }
        o.as_object_mut().unwrap().insert("items".into(), json!(items));
    }
    Ok(())
}

pub fn get_order(conn: &Connection, id: i64) -> Res<Value> {
    let mut rows = db::query_rows(conn, &format!("{ORDER_SELECT} WHERE o.id=?"), &[&id])?;
    if rows.is_empty() { return Err(AppError::not_found("Pesanan tidak ditemukan.")); }
    attach_items(conn, &mut rows)?;
    Ok(rows.remove(0))
}

pub fn list_orders(conn: &Connection, env: &Env, status: &str, date: Option<chrono::NaiveDate>) -> Res<Vec<Value>> {
    let mut sql = format!("{ORDER_SELECT} WHERE 1=1");
    let mut args: Vec<Box<dyn ToSql>> = vec![];
    match status {
        "active" => sql.push_str(" AND o.status IN ('draft','pending','confirmed','preparing','ready','served')"),
        "all" => {}
        s => { sql.push_str(" AND o.status = ?"); args.push(Box::new(s.to_owned())); }
    }
    if status != "active" {
        let d = date.unwrap_or_else(|| db::local_date(db::now(), env.tz_offset_min));
        let (a, b) = db::day_range_utc(d, env.tz_offset_min);
        sql.push_str(" AND o.created_at >= ? AND o.created_at < ?");
        args.push(Box::new(a)); args.push(Box::new(b));
    }
    sql.push_str(" ORDER BY o.id DESC LIMIT 500");
    let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let mut rows = db::query_rows(conn, &sql, &refs)?;
    attach_items(conn, &mut rows)?;
    Ok(rows)
}

pub struct TrxQuery { pub from: chrono::NaiveDate, pub to: chrono::NaiveDate, pub status: Option<String>, pub method: Option<String>, pub search: Option<String> }

pub fn list_transactions(conn: &Connection, env: &Env, q: &TrxQuery) -> Res<Value> {
    let (start, _) = db::day_range_utc(q.from, env.tz_offset_min);
    let (_, end) = db::day_range_utc(q.to, env.tz_offset_min);
    let mut sql = String::from(
        "SELECT t.*, o.order_number, o.order_type, o.queue_number, o.status AS order_status, COALESCE(o.customer_name, c.name, 'Guest') AS customer_name, u.name AS cashier
         FROM transactions t LEFT JOIN orders o ON o.id=t.order_id LEFT JOIN customers c ON c.id=t.customer_id JOIN users u ON u.id=t.user_id
         WHERE COALESCE(t.paid_at, t.created_at) >= ? AND COALESCE(t.paid_at, t.created_at) < ?");
    let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(start), Box::new(end)];
    match q.status.as_deref() {
        None | Some("all") => {}
        Some("completed") => sql.push_str(" AND t.status IN ('confirmed','partial_refund')"),
        Some("cancelled") => sql.push_str(" AND t.status IN ('voided','refunded','cancelled')"),
        Some(s) => { sql.push_str(" AND t.status = ?"); args.push(Box::new(s.to_owned())); }
    }
    if let Some(m) = q.method.as_deref().filter(|m| *m != "all") { sql.push_str(" AND t.payment_method = ?"); args.push(Box::new(m.to_owned())); }
    if let Some(s) = q.search.as_deref().filter(|s| !s.trim().is_empty()) {
        sql.push_str(" AND (t.trx_number LIKE ? OR o.customer_name LIKE ? OR o.order_number LIKE ?)");
        let like = format!("%{}%", s.trim());
        for _ in 0..3 { args.push(Box::new(like.clone())); }
    }
    sql.push_str(" ORDER BY t.id DESC LIMIT 1000");
    let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = db::query_rows(conn, &sql, &refs)?;

    let done: Vec<&Value> = rows.iter().filter(|r| matches!(r["status"].as_str(), Some("confirmed" | "partial_refund"))).collect();
    let net: i64 = done.iter().map(|r| r["total_amount"].as_i64().unwrap_or(0) - r["refunded_amount"].as_i64().unwrap_or(0)).sum();
    let cancelled = rows.iter().filter(|r| matches!(r["status"].as_str(), Some("voided" | "refunded" | "cancelled"))).count();
    Ok(json!({
        "data": rows,
        "summary": {
            "total_sales": net,
            "total_transactions": done.len(),
            "average_transaction": if done.is_empty() { 0 } else { (net as f64 / done.len() as f64).round() as i64 },
            "cancelled_count": cancelled,
        }
    }))
}
