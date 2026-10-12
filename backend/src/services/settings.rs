use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::{error::Res, money::{rate_to_bps, Billing}};

pub fn get(conn: &Connection, key: &str) -> Res<Option<String>> {
    Ok(conn.query_row("SELECT value FROM settings WHERE key = ?", [key], |r| r.get(0)).optional()?)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Res<()> {
    conn.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
    Ok(())
}

pub fn get_bool(conn: &Connection, key: &str, default: bool) -> Res<bool> {
    Ok(get(conn, key)?.map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on")).unwrap_or(default))
}
fn get_f64(conn: &Connection, key: &str, default: f64) -> Res<f64> {
    Ok(get(conn, key)?.and_then(|v| v.parse().ok()).unwrap_or(default))
}

pub fn billing(conn: &Connection) -> Res<Billing> {
    Ok(Billing {
        apply_tax: get_bool(conn, "apply_tax", true)?,
        tax_bps: rate_to_bps(get_f64(conn, "tax_rate", 0.10)?),
        apply_service_dine_in: get_bool(conn, "apply_service_charge_dine_in", true)?,
        service_bps: rate_to_bps(get_f64(conn, "service_charge_rate", 0.05)?),
        apply_packaging: get_bool(conn, "apply_packaging_fee", true)?,
        packaging_fee: get_f64(conn, "packaging_fee", 2000.0)?.round() as i64,
    })
}

pub fn billing_json(conn: &Connection) -> Res<Value> {
    let b = billing(conn)?;
    Ok(json!({
        "tax_rate": b.tax_bps as f64 / 10_000.0,
        "apply_tax": b.apply_tax,
        "service_charge_rate": b.service_bps as f64 / 10_000.0,
        "apply_service_charge_dine_in": b.apply_service_dine_in,
        "packaging_fee": b.packaging_fee,
        "apply_packaging_fee": b.apply_packaging,
        "enable_split_bill": get_bool(conn, "enable_split_bill", true)?,
        "refund_requires_admin": get_bool(conn, "refund_requires_admin", true)?,
    }))
}

pub fn feature(conn: &Connection, feature: &str) -> Res<bool> {
    let Some(t) = get(conn, "business_type")? else { return Ok(true) };
    let t = t.as_str();
    Ok(match feature {
        "karaoke" | "table_management" | "customer_management" | "profit_reporting" | "expense_reporting" => matches!(t, "cafe" | "restoran"),
        "inventory_complex" => t == "restoran",
        _ => true,
    })
}

pub fn shop(conn: &Connection) -> Res<Value> {
    Ok(json!({
        "app_name": get(conn, "app_name")?.unwrap_or_else(|| "Vresto POS".into()),
        "name": get(conn, "restaurant_name")?.unwrap_or_else(|| "Vresto".into()),
        "address": get(conn, "restaurant_address")?.unwrap_or_default(),
        "phone": get(conn, "restaurant_phone")?.unwrap_or_default(),
        "business_type": get(conn, "business_type")?,
        "setup_locked": get_bool(conn, "setup_locked", false)?,
        "features": {
            "karaoke": feature(conn, "karaoke")?,
            "table_management": feature(conn, "table_management")?,
            "customer_management": feature(conn, "customer_management")?,
            "profit_reporting": feature(conn, "profit_reporting")?,
            "expense_reporting": feature(conn, "expense_reporting")?,
        },
    }))
}
