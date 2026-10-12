use chrono::{DateTime, FixedOffset, NaiveDate, SecondsFormat, Utc};
use rusqlite::{types::ValueRef, Connection, ToSql};
use serde_json::{Map, Value};

use crate::error::Res;

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_debt_payments.sql"),
    include_str!("../migrations/0003_refunds_voids.sql"),
];

pub fn open(path: &str) -> Res<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000; PRAGMA synchronous = NORMAL;")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Res<()> {
    let current: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        conn.execute_batch(&format!("BEGIN; {sql}; PRAGMA user_version = {}; COMMIT;", i + 1))?;
    }
    Ok(())
}

pub fn now() -> DateTime<Utc> { Utc::now() }
pub fn ts(t: DateTime<Utc>) -> String { t.to_rfc3339_opts(SecondsFormat::Secs, true) }
pub fn now_ts() -> String { ts(now()) }

pub fn local_date(t: DateTime<Utc>, tz_min: i32) -> NaiveDate {
    let off = FixedOffset::east_opt(tz_min * 60).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    t.with_timezone(&off).date_naive()
}

pub fn day_range_utc(date: NaiveDate, tz_min: i32) -> (String, String) {
    let start_local = date.and_hms_opt(0, 0, 0).unwrap();
    let start = start_local.and_utc() - chrono::Duration::minutes(tz_min as i64);
    (ts(start), ts(start + chrono::Duration::days(1)))
}

pub fn query_rows(conn: &Connection, sql: &str, params: &[&dyn ToSql]) -> Res<Vec<Value>> {
    let mut stmt = conn.prepare(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt.query_map(params, |r| {
        let mut m = Map::new();
        for (i, n) in names.iter().enumerate() {
            let v = match r.get_ref(i)? {
                ValueRef::Null | ValueRef::Blob(_) => Value::Null,
                ValueRef::Integer(x) => if n.starts_with("is_") { Value::Bool(x != 0) } else { Value::from(x) },
                ValueRef::Real(f) => Value::from(f),
                ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
            };
            m.insert(n.clone(), v);
        }
        Ok(Value::Object(m))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn query_one(conn: &Connection, sql: &str, params: &[&dyn ToSql]) -> Res<Option<Value>> {
    Ok(query_rows(conn, sql, params)?.into_iter().next())
}

pub fn audit(conn: &Connection, user_id: Option<i64>, event: &str, entity: &str, entity_id: i64, data: Value) -> Res<()> {
    conn.execute(
        "INSERT INTO audit_log (user_id, event, entity, entity_id, data, created_at) VALUES (?,?,?,?,?,?)",
        rusqlite::params![user_id, event, entity, entity_id, data.to_string(), now_ts()],
    )?;
    Ok(())
}
