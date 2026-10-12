use rusqlite::{params, Connection};

use crate::{db, error::Res};

pub fn deduct_for_order(conn: &Connection, order_id: i64, user_id: i64) -> Res<()> {
    let items: Vec<(i64, i64)> = {
        let mut st = conn.prepare("SELECT product_id, quantity FROM order_items WHERE order_id = ?")?;
        let rows = st.query_map([order_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let note = conn.query_row("SELECT order_number FROM orders WHERE id = ?", [order_id], |r| r.get::<_, String>(0))?;
    for (pid, qty) in items {
        deduct_product(conn, pid, qty, order_id, user_id, &note, 0)?;
    }
    Ok(())
}

fn deduct_product(conn: &Connection, product_id: i64, qty: i64, order_id: i64, user_id: i64, order_no: &str, depth: u8) -> Res<()> {
    if depth > 5 { return Ok(()); }
    let (ptype, managed): (String, i64) =
        conn.query_row("SELECT type, is_stock_managed FROM products WHERE id = ?", [product_id], |r| Ok((r.get(0)?, r.get(1)?)))?;

    if ptype == "bundle" {
        let kids: Vec<(i64, i64)> = {
            let mut st = conn.prepare("SELECT product_id, quantity FROM product_bundles WHERE parent_id = ?")?;
            let rows = st.query_map([product_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        for (child, q) in kids {
            deduct_product(conn, child, q * qty, order_id, user_id, order_no, depth + 1)?;
        }
        return Ok(());
    }

    if managed == 1 {
        conn.execute("UPDATE products SET stock_quantity = stock_quantity - ? WHERE id = ?", params![qty, product_id])?;
        let after: i64 = conn.query_row("SELECT stock_quantity FROM products WHERE id = ?", [product_id], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO inventory_transactions (product_id, order_id, user_id, type, quantity, stock_after, notes, created_at)
             VALUES (?,?,?,?,?,?,?,?)",
            params![product_id, order_id, user_id, "sales", -(qty as f64), after as f64, format!("Penjualan Order #{order_no}"), db::now_ts()],
        )?;
    }

    let recipe: Vec<(i64, f64)> = {
        let mut st = conn.prepare(
            "SELECT pi.ingredient_id, pi.quantity FROM product_ingredients pi
             JOIN ingredients i ON i.id = pi.ingredient_id WHERE pi.product_id = ? AND i.use_stock = 1")?;
        let rows = st.query_map([product_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    for (ing, per_unit) in recipe {
        let used = per_unit * qty as f64;
        conn.execute("UPDATE ingredients SET current_stock = current_stock - ? WHERE id = ?", params![used, ing])?;
        let after: f64 = conn.query_row("SELECT current_stock FROM ingredients WHERE id = ?", [ing], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO inventory_transactions (ingredient_id, order_id, user_id, type, quantity, stock_after, notes, created_at)
             VALUES (?,?,?,?,?,?,?,?)",
            params![ing, order_id, user_id, "sales", -used, after, format!("Bahan untuk Order #{order_no}"), db::now_ts()],
        )?;
    }
    Ok(())
}
