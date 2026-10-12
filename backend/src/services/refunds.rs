use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{customers, orders, settings, shifts};
use crate::{auth::AuthUser, db, error::{AppError, Res}, money::POINT_VALUE, state::Env};

#[derive(Deserialize, Debug)]
pub struct VoidIn { pub reason: String }

#[derive(Deserialize, Debug)]
pub struct RefundIn {

    pub client_uuid: Option<String>,
    pub amount: i64,
    pub reason: String,

    #[serde(default)]
    pub restock: bool,

    pub shift_id: Option<i64>,
}

struct Trx {
    status: String, shift_id: Option<i64>, order_id: Option<i64>, user_id: i64,
    customer_id: Option<i64>, method: String, total: i64, refunded: i64,
}

fn load_trx(conn: &Connection, id: i64) -> Res<Trx> {
    conn.query_row(
        "SELECT status, shift_id, order_id, user_id, customer_id, payment_method, total_amount, refunded_amount FROM transactions WHERE id=?",
        [id], |r| Ok(Trx { status: r.get(0)?, shift_id: r.get(1)?, order_id: r.get(2)?, user_id: r.get(3)?, customer_id: r.get(4)?,
                          method: r.get(5)?, total: r.get(6)?, refunded: r.get(7)? }))
        .map_err(|_| AppError::not_found("Transaksi tidak ditemukan."))
}

fn need_reason(s: &str, what: &str) -> Res<String> {
    let r = s.trim();
    if r.is_empty() { return Err(AppError::validation(format!("Alasan {what} wajib diisi."))); }
    Ok(r.to_owned())
}

pub fn get_transaction(conn: &Connection, id: i64) -> Res<Value> {
    let mut t = db::query_one(conn,
        "SELECT t.*, o.order_number, o.status AS order_status, o.order_type, c.name AS customer_name, u.name AS cashier
         FROM transactions t LEFT JOIN orders o ON o.id=t.order_id LEFT JOIN customers c ON c.id=t.customer_id JOIN users u ON u.id=t.user_id
         WHERE t.id=?", &[&id])?.ok_or_else(|| AppError::not_found("Transaksi tidak ditemukan."))?;
    let refunds = db::query_rows(conn,
        "SELECT r.id, r.amount, r.payment_method, r.reason, r.is_restocked, r.shift_id, r.created_at, u.name AS user
         FROM refunds r JOIN users u ON u.id=r.user_id WHERE r.transaction_id=? ORDER BY r.id", &[&id])?;
    let void = db::query_one(conn,
        "SELECT v.reason, v.created_at, u.name AS user FROM transaction_voids v JOIN users u ON u.id=v.user_id WHERE v.transaction_id=?", &[&id])?;
    let o = t.as_object_mut().unwrap();
    o.insert("refunds".into(), json!(refunds));
    o.insert("void".into(), void.unwrap_or(Value::Null));
    Ok(t)
}

pub fn void_transaction(conn: &mut Connection, env: &Env, user: &AuthUser, id: i64, input: &VoidIn) -> Res<Value> {
    let reason = need_reason(&input.reason, "void")?;
    let tx = conn.transaction()?;
    let t = load_trx(&tx, id)?;
    if t.status == "voided" { return get_transaction(&tx, id); }
    if !matches!(t.status.as_str(), "confirmed" | "unpaid") {
        return Err(AppError::business(format!("Transaksi berstatus {} tidak bisa di-void.", t.status)));
    }
    if t.refunded > 0 { return Err(AppError::business("Transaksi sudah pernah direfund; tidak bisa di-void.")); }
    let is_debt = t.status == "unpaid";
    let order_id = t.order_id.ok_or_else(|| AppError::business("Transaksi tanpa pesanan tidak bisa di-void."))?;
    let (ostatus, points_used): (String, i64) = tx.query_row("SELECT status, points_used FROM orders WHERE id=?", [order_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    match ostatus.as_str() {
        "completed" => return Err(AppError::business("Pesanan sudah selesai (stok sudah berkurang). Gunakan refund.")),
        "cancelled" => return Err(AppError::business("Pesanan sudah dibatalkan.")),
        _ => {}
    }

    let mut void_shift = None;
    if is_debt {
        if !user.is_admin() && t.user_id != user.id { return Err(AppError::Forbidden("Hanya kasir pembuat transaksi atau admin yang boleh void.".into())); }
    } else {
        let sid = t.shift_id.ok_or_else(|| AppError::business("Transaksi tidak terikat shift; gunakan refund."))?;
        let (owner, st): (i64, String) = tx.query_row("SELECT user_id, status FROM shifts WHERE id=?", [sid], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if st != "open" { return Err(AppError::business("Shift transaksi ini sudah ditutup. Gunakan refund.")); }
        if owner != user.id && !user.is_admin() { return Err(AppError::Forbidden("Hanya kasir shift ini atau admin yang boleh void.".into())); }
        void_shift = Some(sid);
    }

    let mut touched_debt_customer = None;
    if is_debt {
        let debt: Option<i64> = tx.query_row("SELECT id FROM debts WHERE transaction_id=?", [id], |r| r.get(0)).optional()?;
        if let Some(did) = debt {
            let active: i64 = tx.query_row(
                "SELECT COUNT(*) FROM debt_payments p WHERE p.debt_id=? AND NOT EXISTS (SELECT 1 FROM debt_payment_reversals r WHERE r.payment_id=p.id)", [did], |r| r.get(0))?;
            if active > 0 { return Err(AppError::business("Hutang ini sudah memiliki pembayaran. Batalkan pelunasannya dulu.")); }
            tx.execute("UPDATE debts SET remaining_amount=0, status='VOID' WHERE id=?", [did])?;
            touched_debt_customer = t.customer_id;
        }
    }

    let now = db::now_ts();
    tx.execute("UPDATE transactions SET status='voided', remaining_refundable=0 WHERE id=?", [id])?;
    tx.execute("INSERT INTO transaction_voids (transaction_id, order_id, shift_id, user_id, reason, created_at) VALUES (?,?,?,?,?,?)",
        params![id, order_id, void_shift, user.id, reason, now])?;
    tx.execute("UPDATE orders SET status='cancelled', payment_status=?, cancelled_at=?, cancel_reason=?, updated_at=? WHERE id=?",
        params![if is_debt { "unpaid" } else { "refunded" }, now, format!("Void: {reason}"), now, order_id])?;
    orders::free_table(&tx, env, order_id)?;

    if let Some(cid) = t.customer_id {
        if t.method == "points" {
            customers::restore_points(&tx, cid, (t.total + POINT_VALUE - 1) / POINT_VALUE)?;
        } else {
            customers::reverse_visit(&tx, cid, t.total, t.total / 10_000, true)?;
        }
        customers::restore_points(&tx, cid, points_used)?;
    }
    if let Some(cid) = touched_debt_customer {
        tx.execute("UPDATE customers SET total_debt = (SELECT COALESCE(SUM(remaining_amount),0) FROM debts WHERE customer_id=?1) WHERE id=?1", [cid])?;
    }
    db::audit(&tx, Some(user.id), "voided", "transaction", id, json!({"reason": reason, "total": t.total, "method": t.method}))?;
    let out = get_transaction(&tx, id)?;
    tx.commit()?;

    env.emit("order.updated", json!({"id": order_id}));
    env.emit("stats.updated", json!({}));
    if let Some(cid) = touched_debt_customer { env.emit("debt.updated", json!({"customer_id": cid})); }
    Ok(out)
}

fn resolve_refund_shift(conn: &Connection, user: &AuthUser, explicit: Option<i64>, method: &str) -> Res<Option<i64>> {
    let found = if let Some(s) = explicit {
        user.require_admin()?;
        let st: Option<String> = conn.query_row("SELECT status FROM shifts WHERE id=?", [s], |r| r.get(0)).optional()?;
        if st.as_deref() != Some("open") { return Err(AppError::business("shift_id tidak ditemukan atau sudah ditutup.")); }
        Some(s)
    } else if let Some(s) = shifts::open_shift_id(conn, user.id)? {
        Some(s)
    } else if user.is_admin() {

        conn.query_row("SELECT id FROM shifts WHERE status='open' ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).optional()?
    } else { None };
    if found.is_none() && method == "cash" {
        return Err(AppError::conflict("Tidak ada shift terbuka. Buka shift untuk memproses refund tunai."));
    }
    Ok(found)
}

fn restock_order(conn: &Connection, order_id: i64, user_id: i64) -> Res<()> {
    let done: i64 = conn.query_row("SELECT COUNT(*) FROM inventory_transactions WHERE order_id=? AND type='return'", [order_id], |r| r.get(0))?;
    if done > 0 { return Err(AppError::business("Stok pesanan ini sudah pernah dikembalikan.")); }
    let rows: Vec<(Option<i64>, Option<i64>, f64)> = {
        let mut st = conn.prepare("SELECT product_id, ingredient_id, quantity FROM inventory_transactions WHERE order_id=? AND type='sales' ORDER BY id")?;
        let r = st.query_map([order_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        r.collect::<Result<_, _>>()?
    };
    let now = db::now_ts();
    for (pid, iid, q) in rows {
        let back = -q;
        let after = if let Some(p) = pid {
            conn.execute("UPDATE products SET stock_quantity = stock_quantity + ? WHERE id=?", params![back.round() as i64, p])?;
            conn.query_row("SELECT stock_quantity FROM products WHERE id=?", [p], |r| r.get::<_, i64>(0))? as f64
        } else if let Some(i) = iid {
            conn.execute("UPDATE ingredients SET current_stock = current_stock + ? WHERE id=?", params![back, i])?;
            conn.query_row("SELECT current_stock FROM ingredients WHERE id=?", [i], |r| r.get::<_, f64>(0))?
        } else { continue };
        conn.execute(
            "INSERT INTO inventory_transactions (product_id, ingredient_id, order_id, user_id, type, quantity, stock_after, notes, created_at) VALUES (?,?,?,?,?,?,?,?,?)",
            params![pid, iid, order_id, user_id, "return", back, after, "Stok kembali karena refund", now])?;
    }
    Ok(())
}

fn refund_result(conn: &Connection, refund_id: i64) -> Res<Value> {
    let r = db::query_one(conn,
        "SELECT r.id, r.transaction_id, r.amount, r.payment_method, r.shift_id, r.reason, r.is_restocked, r.created_at,
                t.status AS transaction_status, t.total_amount, t.refunded_amount, t.remaining_refundable
         FROM refunds r JOIN transactions t ON t.id=r.transaction_id WHERE r.id=?", &[&refund_id])?
        .ok_or_else(|| AppError::not_found("Refund tidak ditemukan."))?;
    let cash_out = r["payment_method"] == "cash";
    Ok(json!({ "refund": r, "cash_out": cash_out }))
}

pub fn refund(conn: &mut Connection, env: &Env, user: &AuthUser, trx_id: i64, input: &RefundIn) -> Res<Value> {
    if let Some(cu) = input.client_uuid.as_deref() {
        if let Some(rid) = conn.query_row("SELECT id FROM refunds WHERE client_uuid=?", [cu], |r| r.get::<_, i64>(0)).optional()? {
            return refund_result(conn, rid);
        }
    }
    if settings::get_bool(conn, "refund_requires_admin", true)? { user.require_admin()?; }
    let reason = need_reason(&input.reason, "refund")?;
    if input.amount <= 0 { return Err(AppError::validation("Jumlah refund harus lebih dari 0.")); }

    let tx = conn.transaction()?;
    let t = load_trx(&tx, trx_id)?;
    match t.status.as_str() {
        "confirmed" | "partial_refund" => {}
        "unpaid" => return Err(AppError::business("Transaksi hutang tidak bisa direfund. Gunakan void, atau batalkan pelunasannya.")),
        "voided" => return Err(AppError::business("Transaksi sudah di-void.")),
        "refunded" => return Err(AppError::business("Transaksi sudah direfund penuh.")),
        s => return Err(AppError::business(format!("Transaksi berstatus {s} tidak bisa direfund."))),
    }
    let remaining = t.total - t.refunded;
    if input.amount > remaining { return Err(AppError::business(format!("Jumlah melebihi sisa yang bisa direfund (Rp{remaining})."))); }
    let after = t.refunded + input.amount;
    let full = after == t.total;
    if t.method == "points" && !(t.refunded == 0 && full) {
        return Err(AppError::business("Transaksi yang dibayar dengan poin hanya bisa direfund penuh."));
    }
    let order_id = t.order_id.ok_or_else(|| AppError::business("Transaksi tanpa pesanan tidak bisa direfund."))?;
    let (ostatus, points_used): (String, i64) = tx.query_row("SELECT status, points_used FROM orders WHERE id=?", [order_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if input.restock && !(full && ostatus == "completed") {
        return Err(AppError::validation("restock hanya untuk refund yang membuat total refund penuh pada pesanan yang sudah selesai."));
    }
    let shift_id = resolve_refund_shift(&tx, user, input.shift_id, &t.method)?;

    let now = db::now_ts();
    tx.execute(
        "INSERT INTO refunds (uuid, client_uuid, transaction_id, order_id, shift_id, user_id, amount, payment_method, reason, is_restocked, created_at)
         VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        params![uuid::Uuid::new_v4().to_string(), input.client_uuid, trx_id, order_id, shift_id, user.id, input.amount, t.method, reason, input.restock as i64, now])?;
    let refund_id = tx.last_insert_rowid();
    tx.execute("UPDATE transactions SET refunded_amount=?1, remaining_refundable=total_amount-?1, status=?2 WHERE id=?3",
        params![after, if full { "refunded" } else { "partial_refund" }, trx_id])?;

    if full {
        tx.execute("UPDATE orders SET payment_status='refunded', updated_at=? WHERE id=?", params![now, order_id])?;
        if !matches!(ostatus.as_str(), "completed" | "cancelled") {
            tx.execute("UPDATE orders SET status='cancelled', cancelled_at=?1, cancel_reason=?2 WHERE id=?3", params![now, format!("Refund penuh: {reason}"), order_id])?;
            orders::free_table(&tx, env, order_id)?;
        }
    }
    if input.restock { restock_order(&tx, order_id, user.id)?; }

    if let Some(cid) = t.customer_id {
        if t.method == "points" {
            customers::restore_points(&tx, cid, (t.total + POINT_VALUE - 1) / POINT_VALUE)?;
        } else if t.total > 0 {

            let earned = t.total / 10_000;
            let cum = |r: i64| (earned as i128 * r as i128 / t.total as i128) as i64;
            customers::reverse_visit(&tx, cid, input.amount, cum(after) - cum(t.refunded), full)?;
        }
        if full { customers::restore_points(&tx, cid, points_used)?; }
    }
    db::audit(&tx, Some(user.id), "refunded", "transaction", trx_id, json!({"amount": input.amount, "reason": reason, "full": full, "restock": input.restock}))?;
    let out = refund_result(&tx, refund_id)?;
    tx.commit()?;

    env.emit("order.updated", json!({"id": order_id}));
    env.emit("stats.updated", json!({}));
    Ok(out)
}
