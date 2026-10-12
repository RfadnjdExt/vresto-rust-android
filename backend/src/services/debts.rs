use rusqlite::{params, Connection, OptionalExtension, ToSql};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{orders::normalize_method, shifts};
use crate::{auth::AuthUser, db, error::{AppError, Res}, state::Env};

pub enum Scope { Debt(i64), Customer(i64) }

#[derive(Deserialize, Debug)]
pub struct RepayIn {

    pub client_uuid: Option<String>,
    pub amount: i64,
    pub payment_method: String,
    pub notes: Option<String>,
}

const DEBT_SELECT: &str =
    "SELECT d.*, c.name AS customer_name, o.order_number,
            (d.remaining_amount > 0 AND d.due_date IS NOT NULL AND d.due_date < ?) AS is_overdue
     FROM debts d JOIN customers c ON c.id = d.customer_id LEFT JOIN orders o ON o.id = d.order_id";

pub fn list(conn: &Connection, env: &Env, status: &str, customer_id: Option<i64>) -> Res<Vec<Value>> {
    let today = db::local_date(db::now(), env.tz_offset_min).to_string();
    let mut sql = format!("{DEBT_SELECT} WHERE 1=1");
    let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(today)];
    match status {
        "open" => sql.push_str(" AND d.remaining_amount > 0"),
        "paid" => sql.push_str(" AND d.status = 'PAID'"),
        "all" => {}
        _ => return Err(AppError::validation("status harus open, paid, atau all.")),
    }
    if let Some(c) = customer_id { sql.push_str(" AND d.customer_id = ?"); args.push(Box::new(c)); }
    sql.push_str(" ORDER BY d.due_date IS NULL, d.due_date, d.id LIMIT 500");
    let refs: Vec<&dyn ToSql> = args.iter().map(|b| b.as_ref()).collect();
    db::query_rows(conn, &sql, &refs)
}

pub fn get(conn: &Connection, env: &Env, id: i64) -> Res<Value> {
    let today = db::local_date(db::now(), env.tz_offset_min).to_string();
    let mut d = db::query_one(conn, &format!("{DEBT_SELECT} WHERE d.id = ?"), &[&today, &id])?
        .ok_or_else(|| AppError::not_found("Hutang tidak ditemukan."))?;
    let payments = db::query_rows(conn,
        "SELECT p.id, p.amount, p.payment_method, p.notes, p.created_at, p.user_id, p.shift_id, u.name AS cashier,
                (r.id IS NOT NULL) AS is_reversed, r.reason AS reversal_reason, r.created_at AS reversed_at
         FROM debt_payments p JOIN users u ON u.id = p.user_id LEFT JOIN debt_payment_reversals r ON r.payment_id = p.id
         WHERE p.debt_id = ? ORDER BY p.id", &[&id])?;
    d.as_object_mut().unwrap().insert("payments".into(), json!(payments));
    Ok(d)
}

fn build_result(conn: &Connection, ids: &[i64]) -> Res<Value> {
    let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let payments = db::query_rows(conn, &format!(
        "SELECT p.id, p.debt_id, p.amount, p.payment_method, p.customer_id, p.shift_id,
                d.remaining_amount AS remaining_after, d.status AS debt_status
         FROM debt_payments p JOIN debts d ON d.id = p.debt_id WHERE p.id IN ({list}) ORDER BY p.id"), &[])?;
    let applied: i64 = payments.iter().filter_map(|p| p["amount"].as_i64()).sum();
    let cid = payments.first().and_then(|p| p["customer_id"].as_i64()).unwrap_or_default();
    let customer = db::query_one(conn, "SELECT id, name, total_debt, credit_limit FROM customers WHERE id=?", &[&cid])?.unwrap_or(Value::Null);
    Ok(json!({ "applied": applied, "payments": payments, "customer": customer }))
}

pub fn repay(conn: &mut Connection, env: &Env, user: &AuthUser, scope: Scope, input: &RepayIn) -> Res<Value> {

    if let Some(cu) = input.client_uuid.as_deref() {
        let ids: Vec<i64> = {
            let mut st = conn.prepare("SELECT id FROM debt_payments WHERE client_uuid = ? ORDER BY id")?;
            let rows = st.query_map([cu], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        if !ids.is_empty() { return build_result(conn, &ids); }
    }

    let method = normalize_method(&input.payment_method)?;
    if method == "debt" || method == "points" {
        return Err(AppError::validation("Metode ini tidak bisa dipakai untuk melunasi hutang."));
    }
    if input.amount <= 0 { return Err(AppError::validation("Jumlah pembayaran harus lebih dari 0.")); }
    let shift_id = shifts::open_shift_id(conn, user.id)?
        .ok_or_else(|| AppError::conflict("Shift belum dibuka. Buka shift untuk menerima pembayaran hutang."))?;

    let tx = conn.transaction()?;

    let (customer_id, targets): (i64, Vec<(i64, i64)>) = match scope {
        Scope::Debt(id) => {
            let (rem, cust, st): (i64, i64, String) = tx
                .query_row("SELECT remaining_amount, customer_id, status FROM debts WHERE id = ?", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .map_err(|_| AppError::not_found("Hutang tidak ditemukan."))?;
            if st == "VOID" { return Err(AppError::business("Hutang ini sudah dibatalkan (void).")); }
            if rem == 0 { return Err(AppError::business("Hutang ini sudah lunas.")); }
            if input.amount > rem { return Err(AppError::business(format!("Jumlah melebihi sisa hutang (Rp{rem})."))); }
            (cust, vec![(id, rem)])
        }
        Scope::Customer(cid) => {
            tx.query_row("SELECT 1 FROM customers WHERE id = ?", [cid], |_| Ok(())).map_err(|_| AppError::not_found("Pelanggan tidak ditemukan."))?;

            let rows: Vec<(i64, i64)> = {
                let mut st = tx.prepare("SELECT id, remaining_amount FROM debts WHERE customer_id = ? AND remaining_amount > 0 ORDER BY due_date IS NULL, due_date, id")?;
                let r = st.query_map([cid], |r| Ok((r.get(0)?, r.get(1)?)))?;
                r.collect::<Result<_, _>>()?
            };
            let total: i64 = rows.iter().map(|r| r.1).sum();
            if total == 0 { return Err(AppError::business("Pelanggan tidak memiliki hutang.")); }
            if input.amount > total { return Err(AppError::business(format!("Jumlah melebihi total hutang pelanggan (Rp{total})."))); }
            (cid, rows)
        }
    };

    let mut left = input.amount;
    let mut ids = vec![];
    let now = db::now_ts();
    for (debt_id, rem) in targets {
        if left == 0 { break; }
        let a = left.min(rem);
        tx.execute(
            "UPDATE debts SET remaining_amount = remaining_amount - ?1,
               status = CASE WHEN remaining_amount - ?1 = 0 THEN 'PAID' ELSE 'PARTIAL' END WHERE id = ?2",
            params![a, debt_id])?;
        tx.execute(
            "INSERT INTO debt_payments (uuid, client_uuid, debt_id, customer_id, shift_id, user_id, amount, payment_method, notes, created_at)
             VALUES (?,?,?,?,?,?,?,?,?,?)",
            params![uuid::Uuid::new_v4().to_string(), input.client_uuid, debt_id, customer_id, shift_id, user.id, a, method,
                    input.notes.as_deref().map(str::trim).filter(|s| !s.is_empty()), now])?;
        ids.push(tx.last_insert_rowid());
        left -= a;
    }
    debug_assert_eq!(left, 0);

    tx.execute("UPDATE customers SET total_debt = (SELECT COALESCE(SUM(remaining_amount),0) FROM debts WHERE customer_id = ?1) WHERE id = ?1", [customer_id])?;
    db::audit(&tx, Some(user.id), "debt_paid", "customer", customer_id, json!({"amount": input.amount, "method": method, "payments": ids}))?;
    let mut out = build_result(&tx, &ids)?;
    out.as_object_mut().unwrap().insert("payment_method".into(), json!(method));
    tx.commit()?;

    env.emit("debt.updated", json!({"customer_id": customer_id}));
    env.emit("stats.updated", json!({}));
    Ok(out)
}

#[derive(Deserialize, Debug)]
pub struct ReverseIn { pub reason: String }

fn reversal_result(conn: &Connection, payment_id: i64) -> Res<Value> {
    let r = db::query_one(conn,
        "SELECT p.id AS payment_id, p.amount, p.payment_method, r.shift_id, r.reason, r.created_at AS reversed_at,
                d.id AS debt_id, d.amount AS debt_amount, d.remaining_amount, d.status AS debt_status,
                c.id AS customer_id, c.name AS customer_name, c.total_debt
         FROM debt_payment_reversals r JOIN debt_payments p ON p.id = r.payment_id JOIN debts d ON d.id = p.debt_id JOIN customers c ON c.id = p.customer_id
         WHERE r.payment_id = ?", &[&payment_id])?
        .ok_or_else(|| AppError::not_found("Pembatalan tidak ditemukan."))?;
    Ok(r)
}

pub fn reverse_payment(conn: &mut Connection, env: &Env, user: &AuthUser, payment_id: i64, input: &ReverseIn) -> Res<Value> {
    let reason = input.reason.trim();
    if reason.is_empty() { return Err(AppError::validation("Alasan pembatalan pelunasan wajib diisi.")); }
    let tx = conn.transaction()?;
    let (debt_id, customer_id, pshift, puser, amount, method): (i64, i64, Option<i64>, i64, i64, String) = tx
        .query_row("SELECT debt_id, customer_id, shift_id, user_id, amount, payment_method FROM debt_payments WHERE id = ?", [payment_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
        .map_err(|_| AppError::not_found("Pembayaran tidak ditemukan."))?;

    if tx.query_row("SELECT 1 FROM debt_payment_reversals WHERE payment_id = ?", [payment_id], |_| Ok(())).optional()?.is_some() {
        return reversal_result(&tx, payment_id);
    }
    let pshift_open = match pshift {
        Some(s) => tx.query_row("SELECT status FROM shifts WHERE id = ?", [s], |r| r.get::<_, String>(0))? == "open",
        None => false,
    };
    if !(user.is_admin() || (pshift_open && puser == user.id)) {
        return Err(AppError::Forbidden("Hanya admin, atau kasir penerima selama shift-nya masih terbuka, yang boleh membatalkan pelunasan.".into()));
    }
    let rev_shift = if pshift_open { pshift } else { shifts::open_shift_id(&tx, user.id)? };
    if rev_shift.is_none() && method == "cash" {
        return Err(AppError::conflict("Shift belum dibuka. Buka shift untuk membatalkan pelunasan tunai (uang dikembalikan dari laci)."));
    }

    tx.execute(
        "UPDATE debts SET remaining_amount = remaining_amount + ?1,
           status = CASE WHEN remaining_amount + ?1 = amount THEN 'OPEN' ELSE 'PARTIAL' END WHERE id = ?2",
        params![amount, debt_id])?;
    tx.execute("INSERT INTO debt_payment_reversals (payment_id, shift_id, user_id, reason, created_at) VALUES (?,?,?,?,?)",
        params![payment_id, rev_shift, user.id, reason, db::now_ts()])?;
    tx.execute("UPDATE customers SET total_debt = (SELECT COALESCE(SUM(remaining_amount),0) FROM debts WHERE customer_id = ?1) WHERE id = ?1", [customer_id])?;
    db::audit(&tx, Some(user.id), "debt_payment_reversed", "debt_payment", payment_id, json!({"amount": amount, "method": method, "reason": reason}))?;
    let out = reversal_result(&tx, payment_id)?;
    tx.commit()?;

    env.emit("debt.updated", json!({"customer_id": customer_id}));
    env.emit("stats.updated", json!({}));
    Ok(out)
}
