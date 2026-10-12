use chrono::NaiveDate;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::{db, error::Res, state::Env};

const SOLD: &str = "t.status IN ('confirmed','partial_refund','refunded')";

fn net_by_method(plus: Vec<Value>, minus: Vec<Value>) -> Vec<Value> {
    use std::collections::BTreeMap;
    let mut m: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for r in plus { let e = m.entry(r["method"].as_str().unwrap_or("?").to_owned()).or_default(); e.0 += r["count"].as_i64().unwrap_or(0); e.1 += r["total"].as_i64().unwrap_or(0); }
    for r in minus { m.entry(r["method"].as_str().unwrap_or("?").to_owned()).or_default().1 -= r["total"].as_i64().unwrap_or(0); }
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));
    v.into_iter().map(|(k, (c, t))| json!({"method": k, "count": c, "total": t})).collect()
}

pub fn daily(conn: &Connection, env: &Env, date: NaiveDate) -> Res<Value> {
    let (a, b) = db::day_range_utc(date, env.tz_offset_min);
    let range = "COALESCE(t.paid_at, t.created_at) >= ?1 AND COALESCE(t.paid_at, t.created_at) < ?2";

    let (n, gross): (i64, i64) = conn.query_row(
        &format!("SELECT COUNT(*), COALESCE(SUM(t.total_amount),0) FROM transactions t WHERE {SOLD} AND {range}"), [&a, &b], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let (refund_n, refunds): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(amount),0) FROM refunds WHERE created_at >= ?1 AND created_at < ?2", [&a, &b], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let net = gross - refunds;

    let gross_m = db::query_rows(conn,
        &format!("SELECT t.payment_method AS method, COUNT(*) AS count, SUM(t.total_amount) AS total FROM transactions t WHERE {SOLD} AND {range} GROUP BY t.payment_method"), &[&a, &b])?;
    let refund_m = db::query_rows(conn,
        "SELECT payment_method AS method, COUNT(*) AS count, SUM(amount) AS total FROM refunds WHERE created_at >= ?1 AND created_at < ?2 GROUP BY payment_method", &[&a, &b])?;
    let by_type = db::query_rows(conn,
        &format!("SELECT o.order_type AS order_type, COUNT(*) AS count, SUM(t.total_amount) AS total FROM transactions t JOIN orders o ON o.id=t.order_id WHERE {SOLD} AND {range} GROUP BY o.order_type ORDER BY total DESC"), &[&a, &b])?;
    let top = db::query_rows(conn,
        &format!("SELECT oi.product_name AS name, SUM(oi.quantity) AS qty, SUM(oi.total_price) AS revenue
                  FROM order_items oi JOIN transactions t ON t.order_id=oi.order_id WHERE {SOLD} AND {range}
                  GROUP BY oi.product_name ORDER BY qty DESC LIMIT 10"), &[&a, &b])?;

    let debt_paid = db::query_rows(conn,
        "SELECT payment_method AS method, COUNT(*) AS count, SUM(amount) AS total FROM debt_payments WHERE created_at >= ?1 AND created_at < ?2 GROUP BY payment_method", &[&a, &b])?;
    let debt_rev = db::query_rows(conn,
        "SELECT p.payment_method AS method, COUNT(*) AS count, SUM(p.amount) AS total FROM debt_payment_reversals r JOIN debt_payments p ON p.id=r.payment_id
         WHERE r.created_at >= ?1 AND r.created_at < ?2 GROUP BY p.payment_method", &[&a, &b])?;
    let debt_reversed: i64 = debt_rev.iter().filter_map(|r| r["total"].as_i64()).sum();
    let debt_gross: i64 = debt_paid.iter().filter_map(|r| r["total"].as_i64()).sum();
    let debt_net = debt_gross - debt_reversed;

    let d = date.to_string();
    let expenses: i64 = conn.query_row("SELECT COALESCE(SUM(amount),0) FROM expenses WHERE expense_date=?", [&d], |r| r.get(0))?;
    let receivable: i64 = conn.query_row(
        "SELECT COALESCE(SUM(t.total_amount),0) FROM transactions t WHERE t.status='unpaid' AND t.created_at >= ?1 AND t.created_at < ?2", [&a, &b], |r| r.get(0))?;
    Ok(json!({
        "date": d,
        "gross_sales": gross,
        "total_refunds": refunds,
        "refund_count": refund_n,
        "total_sales": net,
        "total_transactions": n,
        "average_transaction": if n == 0 { 0 } else { (gross as f64 / n as f64).round() as i64 },
        "total_expenses": expenses,
        "new_receivables": receivable,
        "debt_collected": debt_net,
        "debt_reversed": debt_reversed,
        "debt_collected_by_method": net_by_method(debt_paid, debt_rev),
        "total_cash_in": net + debt_net,
        "by_payment_method": net_by_method(gross_m, refund_m),
        "by_order_type": by_type,
        "top_products": top,
    }))
}

pub fn stats(conn: &Connection, env: &Env) -> Res<Value> {
    let today = db::local_date(db::now(), env.tz_offset_min);
    let d = daily(conn, env, today)?;
    let active: i64 = conn.query_row("SELECT COUNT(*) FROM orders WHERE status IN ('pending','confirmed','preparing','ready','served')", [], |r| r.get(0))?;
    let occupied: i64 = conn.query_row("SELECT COUNT(*) FROM dining_tables WHERE is_active=1 AND status='occupied'", [], |r| r.get(0))?;
    let receivables: i64 = conn.query_row("SELECT COALESCE(SUM(remaining_amount),0) FROM debts", [], |r| r.get(0))?;
    Ok(json!({
        "receivables_total": receivables,
        "today_sales": d["total_sales"], "today_transactions": d["total_transactions"], "average_transaction": d["average_transaction"],
        "active_orders": active, "occupied_tables": occupied,
    }))
}
