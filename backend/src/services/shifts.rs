use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::{auth::AuthUser, db, error::{AppError, Res}, state::Env};

pub fn open_shift_id(conn: &Connection, user_id: i64) -> Res<Option<i64>> {
    Ok(conn.query_row("SELECT id FROM shifts WHERE user_id=? AND status='open'", [user_id], |r| r.get(0)).optional()?)
}

pub struct Sales {
    pub total_sales: i64, pub cash_sales: i64, pub digital_sales: i64,
    pub qris: i64, pub ewallet: i64, pub transfer: i64, pub card: i64,
    pub cash_refund: i64, pub refunds: i64, pub total_orders: i64, pub total_transactions: i64,
}

pub fn calculate_sales(conn: &Connection, shift_id: i64) -> Res<Sales> {

    let mut sales = conn.query_row(
        "SELECT
           COALESCE(SUM(CASE WHEN status IN ('confirmed','partial_refund','refunded') THEN total_amount END),0),
           COALESCE(SUM(CASE WHEN status IN ('confirmed','partial_refund','refunded') AND payment_method='cash' THEN total_amount END),0),
           COALESCE(SUM(CASE WHEN status IN ('confirmed','partial_refund','refunded') AND payment_method IN ('qris','ewallet','transfer','card','debit','credit') THEN total_amount END),0),
           COALESCE(SUM(CASE WHEN status IN ('confirmed','partial_refund','refunded') AND payment_method='qris' THEN total_amount END),0),
           COALESCE(SUM(CASE WHEN status IN ('confirmed','partial_refund','refunded') AND payment_method='ewallet' THEN total_amount END),0),
           COALESCE(SUM(CASE WHEN status IN ('confirmed','partial_refund','refunded') AND payment_method='transfer' THEN total_amount END),0),
           COALESCE(SUM(CASE WHEN status IN ('confirmed','partial_refund','refunded') AND payment_method IN ('card','debit','credit') THEN total_amount END),0),
           0,
           0,
           COUNT(DISTINCT CASE WHEN status IN ('confirmed','partial_refund','refunded') THEN order_id END),
           COUNT(CASE WHEN status IN ('confirmed','partial_refund','refunded') THEN 1 END)
         FROM transactions WHERE shift_id=?",
        [shift_id],
        |r| Ok(Sales {
            total_sales: r.get(0)?, cash_sales: r.get(1)?, digital_sales: r.get(2)?, qris: r.get(3)?, ewallet: r.get(4)?,
            transfer: r.get(5)?, card: r.get(6)?, cash_refund: r.get(7)?, refunds: r.get(8)?, total_orders: r.get(9)?, total_transactions: r.get(10)?,
        }),
    )?;

    let (cash_refund, refunds): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(CASE WHEN payment_method='cash' THEN amount END),0), COUNT(*) FROM refunds WHERE shift_id=?",
        [shift_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    sales.cash_refund = cash_refund;
    sales.refunds = refunds;
    Ok(sales)
}

fn expenses_total(conn: &Connection, shift_id: i64) -> Res<i64> {
    Ok(conn.query_row("SELECT COALESCE(SUM(amount),0) FROM expenses WHERE shift_id=?", [shift_id], |r| r.get(0))?)
}

pub fn summary(conn: &Connection, shift_id: i64) -> Res<Value> {
    let sh = db::query_one(conn, "SELECT * FROM shifts WHERE id=?", &[&shift_id])?.ok_or_else(|| AppError::not_found("Shift tidak ditemukan."))?;
    let s = calculate_sales(conn, shift_id)?;
    let expenses = expenses_total(conn, shift_id)?;
    let g = |k: &str| sh[k].as_i64().unwrap_or(0);

    let (dc, dd, rc, rd): (i64, i64, i64, i64) = conn.query_row(
        "SELECT
           COALESCE((SELECT SUM(amount) FROM debt_payments WHERE shift_id=?1 AND payment_method='cash'),0),
           COALESCE((SELECT SUM(amount) FROM debt_payments WHERE shift_id=?1 AND payment_method<>'cash'),0),
           COALESCE((SELECT SUM(p.amount) FROM debt_payment_reversals r JOIN debt_payments p ON p.id=r.payment_id WHERE r.shift_id=?1 AND p.payment_method='cash'),0),
           COALESCE((SELECT SUM(p.amount) FROM debt_payment_reversals r JOIN debt_payments p ON p.id=r.payment_id WHERE r.shift_id=?1 AND p.payment_method<>'cash'),0)",
        [shift_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    let (debt_cash, debt_digital) = (dc - rc, dd - rd);
    let expected = g("opening_cash") + s.cash_sales + debt_cash + g("total_cash_additions") - g("total_cash_drops") - s.cash_refund - expenses;
    Ok(json!({
        "shift": sh,
        "sales": {
            "total_sales": s.total_sales, "cash_sales": s.cash_sales, "digital_sales": s.digital_sales,
            "qris_sales": s.qris, "ewallet_sales": s.ewallet, "transfer_sales": s.transfer, "card_sales": s.card,
            "total_refunds": s.refunds, "total_cash_refund": s.cash_refund,
            "total_orders": s.total_orders, "total_transactions": s.total_transactions,
        },
        "debt_collected": { "cash": debt_cash, "digital": debt_digital, "total": debt_cash + debt_digital },
        "debt_reversed": { "cash": rc, "digital": rd },
        "total_expenses": expenses,
        "expected_cash": expected,
    }))
}

pub fn current(conn: &Connection, user: &AuthUser) -> Res<Option<Value>> {
    match open_shift_id(conn, user.id)? { Some(id) => Ok(Some(summary(conn, id)?)), None => Ok(None) }
}

pub fn start(conn: &Connection, env: &Env, user: &AuthUser, opening_cash: i64, shift_type: Option<&str>, notes: Option<&str>) -> Res<Value> {
    if opening_cash < 0 { return Err(AppError::validation("Kas awal tidak boleh negatif.")); }
    if open_shift_id(conn, user.id)?.is_some() { return Err(AppError::conflict("Shift aktif sudah ada untuk kasir ini.")); }
    let now = db::now();
    let date = db::local_date(now, env.tz_offset_min).format("%Y%m%d").to_string();
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM shifts WHERE shift_number LIKE ?", [format!("SH-{date}-%")], |r| r.get(0))?;
    conn.execute(
        "INSERT INTO shifts (uuid, shift_number, user_id, opened_by, shift_type, status, opening_cash, note_open, opened_at)
         VALUES (?,?,?,?,?,'open',?,?,?)",
        params![uuid::Uuid::new_v4().to_string(), format!("SH-{date}-{:02}", n + 1), user.id, user.id,
                shift_type.unwrap_or("morning"), opening_cash, notes.map(str::trim).filter(|s| !s.is_empty()), db::ts(now)])?;
    let id = conn.last_insert_rowid();
    db::audit(conn, Some(user.id), "shift_opened", "shift", id, json!({"opening_cash": opening_cash}))?;
    summary(conn, id)
}

pub fn close(conn: &mut Connection, env: &Env, user: &AuthUser, shift_id: i64, actual_cash: i64, notes: Option<&str>) -> Res<Value> {
    if actual_cash < 0 { return Err(AppError::validation("Kas aktual tidak boleh negatif.")); }
    let tx = conn.transaction()?;
    let (owner, status): (i64, String) = tx.query_row("SELECT user_id, status FROM shifts WHERE id=?", [shift_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|_| AppError::not_found("Shift tidak ditemukan."))?;
    if owner != user.id && !user.is_admin() { return Err(AppError::Forbidden("Shift ini milik kasir lain.".into())); }
    if status == "closed" { return summary(&tx, shift_id); }

    let sum = summary(&tx, shift_id)?;
    let expected = sum["expected_cash"].as_i64().unwrap_or(0);
    let diff = actual_cash - expected;
    let notes = notes.map(str::trim).filter(|s| !s.is_empty());

    if sum["sales"]["total_orders"].as_i64() == Some(0) && sum["sales"]["total_sales"].as_i64().unwrap_or(0) > 0 {
        return Err(AppError::validation("Total transaksi tidak boleh 0 jika ada penjualan. Cek sinkronisasi transaksi shift."));
    }
    if diff != 0 && notes.is_none() {
        return Err(AppError::validation("Alasan/Catatan wajib diisi saat selisih kas tidak sama dengan 0."));
    }
    tx.execute(
        "UPDATE shifts SET status='closed', closed_at=?, closed_by=?, closing_cash=?, expected_cash=?, cash_difference=?, note_close=?,
           total_sales=?, cash_sales=?, digital_sales=?, total_orders=?, total_expenses=? WHERE id=?",
        params![db::now_ts(), user.id, actual_cash, expected, diff, notes,
                sum["sales"]["total_sales"].as_i64(), sum["sales"]["cash_sales"].as_i64(), sum["sales"]["digital_sales"].as_i64(),
                sum["sales"]["total_orders"].as_i64(), sum["total_expenses"].as_i64(), shift_id])?;
    db::audit(&tx, Some(user.id), "shift_closed", "shift", shift_id, json!({"expected": expected, "actual": actual_cash, "difference": diff}))?;
    let out = summary(&tx, shift_id)?;
    tx.commit()?;
    env.emit("stats.updated", json!({}));
    Ok(out)
}

pub fn cash_movement(conn: &mut Connection, user: &AuthUser, kind: &str, amount: i64, description: &str) -> Res<Value> {
    let column = match kind { "add_cash" => "total_cash_additions", "drop_cash" => "total_cash_drops", _ => return Err(AppError::validation("Jenis mutasi kas tidak valid.")) };
    if amount <= 0 { return Err(AppError::validation("Nominal harus lebih dari 0.")); }
    let shift_id = open_shift_id(conn, user.id)?.ok_or_else(|| AppError::conflict("Shift belum dibuka."))?;
    let tx = conn.transaction()?;
    tx.execute("INSERT INTO shift_cash_logs (shift_id,user_id,type,amount,description,created_at) VALUES (?,?,?,?,?,?)",
        params![shift_id, user.id, kind, amount, description, db::now_ts()])?;
    tx.execute(&format!("UPDATE shifts SET {column} = {column} + ? WHERE id=?"), params![amount, shift_id])?;
    db::audit(&tx, Some(user.id), kind, "shift", shift_id, json!({"amount": amount, "description": description}))?;
    let out = summary(&tx, shift_id)?;
    tx.commit()?;
    Ok(out)
}

pub fn history(conn: &Connection, user: &AuthUser, limit: i64) -> Res<Vec<Value>> {
    let limit = limit.clamp(1, 100);
    if user.is_admin() {
        db::query_rows(conn, "SELECT s.*, u.name AS cashier FROM shifts s JOIN users u ON u.id=s.user_id ORDER BY s.id DESC LIMIT ?", &[&limit])
    } else {
        db::query_rows(conn, "SELECT s.*, u.name AS cashier FROM shifts s JOIN users u ON u.id=s.user_id WHERE s.user_id=? ORDER BY s.id DESC LIMIT ?", &[&user.id, &limit])
    }
}
