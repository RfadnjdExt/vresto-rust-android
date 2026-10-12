use axum::{body::Body, http::{Request, StatusCode}, Router};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use vresto::{api, config::Config, db, seed, state::AppState};

struct App { router: Router, state: AppState }

fn app() -> App {
    let conn = db::open(":memory:").unwrap();
    assert!(seed::demo(&conn).unwrap());
    let state = AppState::new(conn, Config::test());
    App { router: api::router(state.clone()), state }
}

impl App {
    async fn call(&self, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
        let mut b = Request::builder().method(method).uri(path);
        if let Some(t) = token { b = b.header("authorization", format!("Bearer {t}")); }
        let req = match body {
            Some(v) => b.header("content-type", "application/json").body(Body::from(v.to_string())).unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let st = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (st, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }
    async fn login(&self, user: &str, pw: &str) -> String {
        let (s, v) = self.call("POST", "/api/v1/auth/login", None, Some(json!({"username": user, "password": pw}))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v["data"]["token"].as_str().unwrap().to_owned()
    }
    fn q<T: rusqlite::types::FromSql>(&self, sql: &str) -> T {
        self.state.db.lock().unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
    }
}

fn i(v: &Value, path: &[&str]) -> i64 {
    let mut cur = v;
    for p in path { cur = &cur[*p]; }
    cur.as_i64().unwrap_or_else(|| panic!("bukan integer di {path:?}: {v}"))
}

#[tokio::test]
async fn auth_rules() {
    let a = app();

    let (s, _) = a.call("POST", "/api/v1/auth/login", None, Some(json!({"username":"kasir","password":"salahsalah"}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(a.call("GET", "/api/v1/pos/init", None, None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(a.call("GET", "/api/v1/pos/init", Some("sampah"), None).await.0, StatusCode::UNAUTHORIZED);

    let (s, _) = a.call("POST", "/api/v1/auth/login", None, Some(json!({"username":"kasir@vresto.com","password":"warmangji.ksr"}))).await;
    assert_eq!(s, StatusCode::OK);

    let h = bcrypt::hash("rahasia123", 4).unwrap().replacen("$2b$", "$2y$", 1);
    assert!(h.starts_with("$2y$"));
    a.state.db.lock().unwrap().execute("UPDATE users SET password_hash=? WHERE username='kasir'", [h]).unwrap();
    a.login("kasir", "rahasia123").await;

    let t = a.login("kasir", "rahasia123").await;
    a.state.db.lock().unwrap().execute("UPDATE users SET is_active=0 WHERE username='kasir'", []).unwrap();
    assert_eq!(a.call("GET", "/api/v1/auth/me", Some(&t), None).await.0, StatusCode::UNAUTHORIZED);

    for _ in 0..5 { a.call("POST", "/api/v1/auth/login", None, Some(json!({"username":"admin","password":"salahsalah"}))).await; }
    let (s, v) = a.call("POST", "/api/v1/auth/login", None, Some(json!({"username":"admin","password":"warmangji.adm"}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["message"].as_str().unwrap().contains("Terlalu banyak"), "{v}");
}

#[tokio::test]
async fn full_pos_day() {
    let a = app();
    let k = a.login("kasir", "warmangji.ksr").await;
    let mut events = a.state.env.hub.subscribe();

    let order = json!({"items":[{"product_id":4,"qty":1}], "order_type":"take_away"});
    let (s, _) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(json!({"order": order, "payment_method":"cash","amount_paid":50000}))).await;
    assert_eq!(s, StatusCode::CONFLICT);

    let (s, v) = a.call("POST", "/api/v1/pos/shifts/start", Some(&k), Some(json!({"opening_cash":100000}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");

    assert_eq!(a.call("POST", "/api/v1/pos/shifts/start", Some(&k), Some(json!({"opening_cash":1}))).await.0, StatusCode::CONFLICT);

    let dine = json!({
        "items":[
            {"product_id":1,"qty":2,"modifiers":[{"id":1,"quantity":2}]},
            {"product_id":6,"variant_id":2,"qty":1}
        ],
        "order_type":"dine_in","table_id":1,"discount_type":"percent","discount_value":10
    });
    let pay = json!({"client_uuid":"11111111-aaaa","order":dine,"payment_method":"cash","amount_paid":100000});
    let (s, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(pay.clone())).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(i(&v, &["data","total_amount"]), 82800);
    assert_eq!(i(&v, &["data","change_amount"]), 17200);
    let order1 = i(&v, &["data","order_id"]);
    let trx1 = i(&v, &["data","transaction_id"]);
    assert_eq!(a.q::<String>("SELECT status FROM dining_tables WHERE id=1"), "occupied");
    assert_eq!(a.q::<String>(&format!("SELECT status||'/'||payment_status FROM orders WHERE id={order1}")), "preparing/paid");
    assert!(events.try_recv().is_ok(), "event realtime harus terkirim");

    let (s, v2) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(pay)).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(i(&v2, &["data","transaction_id"]), trx1);
    assert_eq!(a.q::<i64>("SELECT COUNT(*) FROM transactions"), 1);

    assert_eq!(a.q::<i64>("SELECT stock_quantity FROM products WHERE id=6"), 24);
    let (s, _) = a.call("POST", &format!("/api/v1/pos/orders/{order1}/complete"), Some(&k), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(a.q::<i64>("SELECT stock_quantity FROM products WHERE id=6"), 23);
    assert_eq!(a.q::<f64>("SELECT current_stock FROM ingredients WHERE id=1"), 20000.0 - 400.0);
    assert_eq!(a.q::<f64>("SELECT current_stock FROM ingredients WHERE id=2"), 60.0 - 2.0);
    assert_eq!(a.q::<String>("SELECT status FROM dining_tables WHERE id=1"), "available");

    assert_eq!(a.call("POST", &format!("/api/v1/pos/orders/{order1}/complete"), Some(&k), None).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(a.q::<i64>("SELECT stock_quantity FROM products WHERE id=6"), 23);

    let (s, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k),
        Some(json!({"order":{"items":[{"product_id":9,"qty":1}],"order_type":"takeaway"},"payment_method":"tunai","amount_paid":50000}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(i(&v, &["data","total_amount"]), 32800);
    assert_eq!(i(&v, &["data","queue_number"]), 1);
    let order2 = i(&v, &["data","order_id"]);
    a.call("POST", &format!("/api/v1/pos/orders/{order2}/complete"), Some(&k), None).await;
    assert_eq!(a.q::<f64>("SELECT current_stock FROM ingredients WHERE id=1"), 20000.0 - 400.0 - 200.0);

    let o = json!({"items":[{"product_id":3,"qty":1}],"order_type":"dine_in","table_id":2});
    let (s, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(json!({"order":o,"payment_method":"cash","amount_paid":1000}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert_eq!(a.q::<String>("SELECT status FROM dining_tables WHERE id=2"), "available", "rollback: meja tidak boleh terkunci");
    let (s, _) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(json!({"order":o,"payment_method":"koin-ajaib","amount_paid":99999}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(json!({"order":{"items":[{"product_id":999,"qty":1}]},"payment_method":"cash","amount_paid":1}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    let (s, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k),
        Some(json!({"order":o,"payment_method":"hutang","amount_paid":0,"customer_id":1}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(a.q::<i64>("SELECT total_debt FROM customers WHERE id=1"), 34500);
    assert_eq!(a.q::<String>("SELECT status FROM transactions ORDER BY id DESC LIMIT 1"), "unpaid");

    let big = json!({"items":[{"product_id":3,"qty":6}],"order_type":"dine_in","table_id":3});
    let (s, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(json!({"order":big,"payment_method":"debt","amount_paid":0,"customer_id":1}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["message"].as_str().unwrap().contains("Limit hutang"));

    let (s, v) = a.call("POST", "/api/v1/pos/orders", Some(&k), Some(json!({"items":[{"product_id":4,"qty":2}],"table_id":4}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let saved = i(&v, &["data","id"]);
    assert_eq!(a.call("POST", "/api/v1/pos/orders", Some(&k), Some(json!({"items":[{"product_id":4,"qty":1}],"table_id":4}))).await.0, StatusCode::UNPROCESSABLE_ENTITY);

    let (s, v) = a.call("PUT", &format!("/api/v1/pos/orders/{saved}"), Some(&k), Some(json!({"items":[{"product_id":4,"qty":3}]}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(i(&v, &["data","total_amount"]), 17250);
    assert_eq!(a.call("POST", &format!("/api/v1/pos/orders/{saved}/cancel"), Some(&k), None).await.0, StatusCode::OK);
    assert_eq!(a.q::<String>("SELECT status FROM dining_tables WHERE id=4"), "available");

    assert_eq!(a.call("PUT", &format!("/api/v1/pos/orders/{order1}"), Some(&k), Some(json!({"items":[{"product_id":4,"qty":1}]}))).await.0, StatusCode::UNPROCESSABLE_ENTITY);

    assert_eq!(a.call("POST", "/api/v1/pos/expenses", Some(&k), Some(json!({"category":"Gas","amount":15000}))).await.0, StatusCode::CREATED);

    let (_, v) = a.call("GET", "/api/v1/pos/shifts/current", Some(&k), None).await;
    assert_eq!(i(&v, &["data","expected_cash"]), 200600);
    assert_eq!(i(&v, &["data","sales","total_sales"]), 115600);
    let (s, _) = a.call("POST", "/api/v1/pos/shifts/close", Some(&k), Some(json!({"actual_cash":200000}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "selisih tanpa catatan harus ditolak");
    let (s, v) = a.call("POST", "/api/v1/pos/shifts/close", Some(&k), Some(json!({"actual_cash":200000,"notes":"kembalian kurang"}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(i(&v, &["data","shift","cash_difference"]), -600);

    assert_eq!(a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(json!({"order":order,"payment_method":"cash","amount_paid":50000}))).await.0, StatusCode::CONFLICT);

    let (_, v) = a.call("GET", "/api/v1/pos/reports/daily", Some(&k), None).await;
    assert_eq!(i(&v, &["data","total_sales"]), 115600);
    assert_eq!(i(&v, &["data","total_transactions"]), 2);
    assert_eq!(i(&v, &["data","new_receivables"]), 34500);
    let (_, v) = a.call("GET", "/api/v1/pos/transactions", Some(&k), None).await;
    assert_eq!(i(&v, &["summary","total_sales"]), 115600);
}

#[tokio::test]
async fn roles_and_admin() {
    let a = app();
    let k = a.login("kasir", "warmangji.ksr").await;
    let ad = a.login("admin", "warmangji.adm").await;
    let p = json!({"name":"Soto Ayam","price":22000,"category_id":1,"variants":[{"name":"Jumbo","price_adjustment":5000}]});
    assert_eq!(a.call("POST", "/api/v1/admin/products", Some(&k), Some(p.clone())).await.0, StatusCode::FORBIDDEN);
    let (s, v) = a.call("POST", "/api/v1/admin/products", Some(&ad), Some(p)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, v) = a.call("GET", "/api/v1/pos/products?q=soto", Some(&k), None).await;
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert_eq!(v["data"][0]["variants"][0]["name"], "Jumbo");

    assert_eq!(a.call("POST", "/api/v1/admin/products", Some(&ad), Some(json!({"name":"X","price":-5}))).await.0, StatusCode::UNPROCESSABLE_ENTITY);

    let (_, v) = a.call("GET", "/api/v1/pos/init", Some(&k), None).await;
    for key in ["products","categories","tables","billing_settings","payment_methods","shop_settings","stats"] { assert!(!v["data"][key].is_null(), "init tanpa {key}"); }

    let (s, v) = a.call("POST", "/api/v1/pos/shifts/start", Some(&k), Some(json!({"opening_cash":"banyak"}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["success"], false);
}

fn debt_invariant(a: &App) {

    let bad: i64 = a.q("SELECT COUNT(*) FROM customers c WHERE c.total_debt != (SELECT COALESCE(SUM(remaining_amount),0) FROM debts WHERE customer_id=c.id)");
    assert_eq!(bad, 0, "total_debt pelanggan tidak sinkron dengan tabel debts");
}

#[tokio::test]
async fn debt_repayment() {
    let a = app();
    let k = a.login("kasir", "warmangji.ksr").await;
    let ad = a.login("admin", "warmangji.adm").await;
    let mut events = a.state.env.hub.subscribe();
    assert_eq!(a.call("POST", "/api/v1/pos/shifts/start", Some(&k), Some(json!({"opening_cash":100000}))).await.0, StatusCode::CREATED);

    for order in [
        json!({"items":[{"product_id":3,"qty":1}],"order_type":"dine_in","table_id":1}),
        json!({"items":[{"product_id":1,"qty":1}],"order_type":"take_away"}),
    ] {
        let (s, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k),
            Some(json!({"order":order,"payment_method":"hutang","amount_paid":0,"customer_id":1}))).await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
    }
    assert_eq!(a.q::<i64>("SELECT total_debt FROM customers WHERE id=1"), 64000);
    debt_invariant(&a);

    let (s, _) = a.call("POST", "/api/v1/pos/debts/1/payments", Some(&ad), Some(json!({"amount":1000,"payment_method":"cash"}))).await;
    assert_eq!(s, StatusCode::CONFLICT);

    let (_, v) = a.call("GET", "/api/v1/pos/debts?status=open", Some(&k), None).await;
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
    assert!(v["data"].as_array().unwrap().iter().all(|d| d["is_overdue"] == false));
    a.state.db.lock().unwrap().execute("UPDATE debts SET due_date='2000-01-01' WHERE id=1", []).unwrap();
    let (_, v) = a.call("GET", "/api/v1/pos/debts?status=open", Some(&k), None).await;
    assert_eq!(v["data"][0]["id"], 1, "yang jatuh tempo lebih dulu tampil pertama");
    assert_eq!(v["data"][0]["is_overdue"], true);
    assert_eq!(v["data"][1]["is_overdue"], false);
    assert_eq!(v["data"][0]["customer_name"], "Budi Santoso");

    let (s, v) = a.call("POST", "/api/v1/pos/debts/1/payments", Some(&k), Some(json!({"amount":10000,"payment_method":"cash"}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["data"]["payments"][0]["remaining_after"], 24500);
    assert_eq!(v["data"]["payments"][0]["debt_status"], "PARTIAL");
    assert_eq!(i(&v, &["data","customer","total_debt"]), 54000);
    debt_invariant(&a);

    for (body, needle) in [
        (json!({"amount":30000,"payment_method":"cash"}), "melebihi sisa"),
        (json!({"amount":0,"payment_method":"cash"}), "lebih dari 0"),
        (json!({"amount":-5,"payment_method":"cash"}), "lebih dari 0"),
        (json!({"amount":1000,"payment_method":"hutang"}), "tidak bisa dipakai"),
        (json!({"amount":1000,"payment_method":"points"}), "tidak bisa dipakai"),
        (json!({"amount":1000,"payment_method":"koin-ajaib"}), "tidak dikenal"),
    ] {
        let (s, v) = a.call("POST", "/api/v1/pos/debts/1/payments", Some(&k), Some(body.clone())).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert!(v["message"].as_str().unwrap().contains(needle), "{body} → {v}");
    }
    assert_eq!(a.q::<i64>("SELECT total_debt FROM customers WHERE id=1"), 54000);
    assert_eq!(a.q::<i64>("SELECT COUNT(*) FROM debt_payments"), 1);

    let body = json!({"client_uuid":"rep-1","amount":40000,"payment_method":"qris","notes":"transfer Budi"});
    let (s, v) = a.call("POST", "/api/v1/pos/customers/1/debt-payments", Some(&k), Some(body.clone())).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let ps = v["data"]["payments"].as_array().unwrap();
    assert_eq!(ps.len(), 2);
    assert_eq!((ps[0]["debt_id"].as_i64(), ps[0]["amount"].as_i64(), ps[0]["debt_status"].as_str()), (Some(1), Some(24500), Some("PAID")));
    assert_eq!((ps[1]["debt_id"].as_i64(), ps[1]["amount"].as_i64(), ps[1]["remaining_after"].as_i64()), (Some(2), Some(15500), Some(14000)));
    assert_eq!(i(&v, &["data","applied"]), 40000);
    assert_eq!(i(&v, &["data","customer","total_debt"]), 14000);
    debt_invariant(&a);

    let (s, v2) = a.call("POST", "/api/v1/pos/customers/1/debt-payments", Some(&k), Some(body)).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(v2["data"]["payments"][0]["id"], v["data"]["payments"][0]["id"]);
    assert_eq!(a.q::<i64>("SELECT COUNT(*) FROM debt_payments"), 3);
    assert_eq!(a.q::<i64>("SELECT total_debt FROM customers WHERE id=1"), 14000);

    let (s, v) = a.call("POST", "/api/v1/pos/customers/1/debt-payments", Some(&k), Some(json!({"amount":20000,"payment_method":"qris"}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["message"].as_str().unwrap().contains("melebihi total hutang"), "{v}");

    let (_, v) = a.call("GET", "/api/v1/pos/shifts/current", Some(&k), None).await;
    assert_eq!(i(&v, &["data","expected_cash"]), 110000);
    assert_eq!(i(&v, &["data","debt_collected","cash"]), 10000);
    assert_eq!(i(&v, &["data","debt_collected","digital"]), 40000);
    assert_eq!(i(&v, &["data","sales","total_sales"]), 0, "pelunasan hutang bukan penjualan");

    let (s, v) = a.call("POST", "/api/v1/pos/debts/2/payments", Some(&k), Some(json!({"amount":14000,"payment_method":"cash"}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["data"]["payments"][0]["debt_status"], "PAID");
    assert_eq!(i(&v, &["data","customer","total_debt"]), 0);
    debt_invariant(&a);
    let (s, v) = a.call("POST", "/api/v1/pos/debts/2/payments", Some(&k), Some(json!({"amount":1,"payment_method":"cash"}))).await;
    assert_eq!((s, v["message"].as_str().unwrap().contains("sudah lunas")), (StatusCode::UNPROCESSABLE_ENTITY, true));
    let (s, v) = a.call("POST", "/api/v1/pos/customers/1/debt-payments", Some(&k), Some(json!({"amount":1,"payment_method":"cash"}))).await;
    assert_eq!((s, v["message"].as_str().unwrap().contains("tidak memiliki hutang")), (StatusCode::UNPROCESSABLE_ENTITY, true));

    {
        let db = a.state.db.lock().unwrap();
        assert!(db.execute("UPDATE debts SET remaining_amount = -1 WHERE id=1", []).is_err());
        assert!(db.execute("UPDATE debts SET remaining_amount = amount + 1 WHERE id=1", []).is_err());
    }

    let (_, v) = a.call("GET", "/api/v1/pos/debts?status=open", Some(&k), None).await;
    assert_eq!(v["data"].as_array().unwrap().len(), 0);
    let (_, v) = a.call("GET", "/api/v1/pos/debts?status=paid&customer_id=1", Some(&k), None).await;
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
    assert_eq!(a.call("GET", "/api/v1/pos/debts?status=ngawur", Some(&k), None).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, v) = a.call("GET", "/api/v1/pos/debts/1", Some(&k), None).await;
    let pays = v["data"]["payments"].as_array().unwrap();
    assert_eq!(pays.len(), 2);
    assert_eq!((pays[0]["payment_method"].as_str(), pays[0]["amount"].as_i64()), (Some("cash"), Some(10000)));
    assert_eq!((pays[1]["payment_method"].as_str(), pays[1]["amount"].as_i64()), (Some("qris"), Some(24500)));
    assert_eq!(a.call("GET", "/api/v1/pos/debts/999", Some(&k), None).await.0, StatusCode::NOT_FOUND);

    let (_, v) = a.call("GET", "/api/v1/pos/reports/daily", Some(&k), None).await;
    assert_eq!(i(&v, &["data","debt_collected"]), 64000);
    assert_eq!(i(&v, &["data","total_sales"]), 0);
    assert_eq!(i(&v, &["data","total_cash_in"]), 64000);
    let by: std::collections::HashMap<String, i64> = v["data"]["debt_collected_by_method"].as_array().unwrap().iter()
        .map(|m| (m["method"].as_str().unwrap().to_owned(), m["total"].as_i64().unwrap())).collect();
    assert_eq!((by["cash"], by["qris"]), (24000, 40000));
    let (_, v) = a.call("GET", "/api/v1/pos/dashboard/stats", Some(&k), None).await;
    assert_eq!(i(&v, &["data","receivables_total"]), 0);

    let (s, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(json!({
        "order":{"items":[{"product_id":3,"qty":1}],"order_type":"dine_in","table_id":2},"payment_method":"debt","amount_paid":0,"customer_id":1}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(a.q::<i64>("SELECT total_debt FROM customers WHERE id=1"), 34500);
    debt_invariant(&a);

    let (_, v) = a.call("GET", "/api/v1/pos/shifts/current", Some(&k), None).await;
    assert_eq!(i(&v, &["data","expected_cash"]), 124000);
    let (s, v) = a.call("POST", "/api/v1/pos/shifts/close", Some(&k), Some(json!({"actual_cash":124000}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(i(&v, &["data","shift","cash_difference"]), 0);

    assert_eq!(a.call("POST", "/api/v1/pos/customers/1/debt-payments", Some(&k), Some(json!({"amount":1000,"payment_method":"cash"}))).await.0, StatusCode::CONFLICT);

    let mut names = vec![];
    while let Ok(m) = events.try_recv() { names.push(serde_json::from_str::<Value>(&m).unwrap()["event"].as_str().unwrap().to_owned()); }
    assert!(names.iter().any(|n| n == "debt.updated"), "event debt.updated tidak terkirim: {names:?}");
}

#[tokio::test]
async fn migrates_existing_v2_database() {

    let path = std::env::temp_dir().join(format!("vresto-mig-{}.sqlite", uuid::Uuid::new_v4()));
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch(include_str!("../migrations/0001_init.sql")).unwrap();
        c.execute_batch(include_str!("../migrations/0002_debt_payments.sql")).unwrap();
        c.execute_batch("PRAGMA user_version = 2;").unwrap();
        c.execute("INSERT INTO users (name,username,password_hash,role,created_at) VALUES ('Lama','lama','x','cashier','2026-01-01T00:00:00Z')", []).unwrap();
        c.execute("INSERT INTO customers (uuid,name,created_at,total_debt) VALUES ('u','Pelanggan Lama','2026-01-01T00:00:00Z',3000)", []).unwrap();
        c.execute("INSERT INTO debts (customer_id,amount,remaining_amount,status,created_at) VALUES (1,5000,3000,'PARTIAL','2026-01-01T00:00:00Z')", []).unwrap();
        c.execute("INSERT INTO debt_payments (uuid,debt_id,customer_id,user_id,amount,payment_method,created_at) VALUES ('p',1,1,1,2000,'cash','2026-01-02T00:00:00Z')", []).unwrap();
    }
    let latest: i64 = 3;
    let conn = db::open(path.to_str().unwrap()).unwrap();
    assert_eq!(conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), latest);
    assert_eq!(conn.query_row("SELECT remaining_amount FROM debts", [], |r| r.get::<_, i64>(0)).unwrap(), 3000);
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM debt_payments", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    for t in ["refunds", "transaction_voids", "debt_payment_reversals"] {
        conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get::<_, i64>(0)).unwrap_or_else(|e| panic!("tabel {t} tidak ada: {e}"));
    }

    conn.execute("INSERT INTO debt_payment_reversals (payment_id,user_id,reason,created_at) VALUES (1,1,'x','2026-01-03T00:00:00Z')", []).unwrap();
    assert!(conn.execute("INSERT INTO debt_payment_reversals (payment_id,user_id,reason,created_at) VALUES (1,1,'lagi','2026-01-03T00:00:00Z')", []).is_err(), "pembatalan ganda harus ditolak DB");
    drop(conn);
    let conn = db::open(path.to_str().unwrap()).unwrap();
    assert_eq!(conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), latest);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn void_and_refund() {
    let a = app();
    let k = a.login("kasir", "warmangji.ksr").await;
    let ad = a.login("admin", "warmangji.adm").await;
    assert_eq!(a.call("POST", "/api/v1/pos/shifts/start", Some(&k), Some(json!({"opening_cash":100000}))).await.0, StatusCode::CREATED);
    let pay = |order: Value, method: &str, paid: i64| json!({"order": order, "payment_method": method, "amount_paid": paid, "customer_id": 1});
    let points = |a: &App| a.q::<i64>("SELECT points FROM customers WHERE id=1");

    let (s, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k),
        Some(pay(json!({"items":[{"product_id":4,"qty":2}],"order_type":"dine_in","table_id":1}), "cash", 20000))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(i(&v, &["data","total_amount"]), 11500);
    let (trx1, ord1) = (i(&v, &["data","transaction_id"]), i(&v, &["data","order_id"]));
    assert_eq!((points(&a), a.q::<i64>("SELECT total_visits FROM customers WHERE id=1"), a.q::<i64>("SELECT total_spent FROM customers WHERE id=1")), (51, 1, 11500));

    let void = |id: i64, reason: &str| (format!("/api/v1/pos/transactions/{id}/void"), json!({"reason": reason}));
    let (p, b) = void(trx1, "  ");
    assert_eq!(a.call("POST", &p, Some(&k), Some(b)).await.0, StatusCode::UNPROCESSABLE_ENTITY, "alasan wajib");
    let (p, b) = void(trx1, "salah input menu");
    let (s, v) = a.call("POST", &p, Some(&k), Some(b.clone())).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["data"]["status"], "voided");
    assert_eq!(v["data"]["void"]["reason"], "salah input menu");
    assert_eq!(a.q::<String>(&format!("SELECT status||'/'||payment_status FROM orders WHERE id={ord1}")), "cancelled/refunded");
    assert_eq!(a.q::<String>("SELECT status FROM dining_tables WHERE id=1"), "available", "meja harus bebas");
    assert_eq!((points(&a), a.q::<i64>("SELECT total_visits FROM customers WHERE id=1"), a.q::<i64>("SELECT total_spent FROM customers WHERE id=1")), (50, 0, 0), "poin/kunjungan/belanja dikembalikan");
    let (_, v) = a.call("GET", "/api/v1/pos/shifts/current", Some(&k), None).await;
    assert_eq!((i(&v, &["data","expected_cash"]), i(&v, &["data","sales","total_sales"]), i(&v, &["data","sales","total_orders"])), (100000, 0, 0), "void = seolah tak pernah terjadi");

    assert_eq!(a.call("POST", &p, Some(&k), Some(b)).await.0, StatusCode::OK);
    assert_eq!(a.q::<i64>("SELECT COUNT(*) FROM transaction_voids"), 1);

    let (s, v) = a.call("POST", &format!("/api/v1/pos/transactions/{trx1}/refund"), Some(&ad), Some(json!({"amount":100,"reason":"x"}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["message"].as_str().unwrap().contains("di-void"), "{v}");

    let (s, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(pay(json!({
        "items":[{"product_id":1,"qty":1,"modifiers":[{"id":1,"quantity":2}]},{"product_id":6,"variant_id":1,"qty":2}],
        "order_type":"dine_in","table_id":1}), "cash", 70000))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!((i(&v, &["data","total_amount"]), i(&v, &["data","change_amount"])), (65550, 4450));
    let (trx2, ord2) = (i(&v, &["data","transaction_id"]), i(&v, &["data","order_id"]));
    assert_eq!(points(&a), 56);
    assert_eq!(a.call("POST", &format!("/api/v1/pos/orders/{ord2}/complete"), Some(&k), None).await.0, StatusCode::OK);
    assert_eq!(a.q::<i64>("SELECT stock_quantity FROM products WHERE id=6"), 22);
    assert_eq!(a.q::<f64>("SELECT current_stock FROM ingredients WHERE id=1"), 19800.0);
    assert_eq!(a.q::<f64>("SELECT current_stock FROM ingredients WHERE id=2"), 59.0);

    let (p, b) = void(trx2, "ups");
    let (s, v) = a.call("POST", &p, Some(&k), Some(b)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["message"].as_str().unwrap().contains("refund"), "{v}");

    let rf = format!("/api/v1/pos/transactions/{trx2}/refund");
    assert_eq!(a.call("POST", &rf, Some(&k), Some(json!({"amount":10000,"reason":"komplain"}))).await.0, StatusCode::FORBIDDEN);
    for (body, needle) in [
        (json!({"amount":10000,"reason":" "}), "Alasan"),
        (json!({"amount":0,"reason":"x"}), "lebih dari 0"),
        (json!({"amount":70000,"reason":"x"}), "melebihi sisa"),
        (json!({"amount":10000,"reason":"x","restock":true}), "restock"),
        (json!({"amount":10000,"reason":"x","shift_id":999}), "shift"),
    ] {
        let (s, v) = a.call("POST", &rf, Some(&ad), Some(body.clone())).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{body} → {v}");
        assert!(v["message"].as_str().unwrap().contains(needle), "{body} → {v}");
    }
    assert_eq!(a.q::<i64>("SELECT COUNT(*) FROM refunds"), 0, "penolakan tidak boleh meninggalkan jejak");

    let (s, v) = a.call("POST", &rf, Some(&ad), Some(json!({"amount":10000,"reason":"menu kurang"}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["data"]["refund"]["transaction_status"], "partial_refund");
    assert_eq!((i(&v, &["data","refund","refunded_amount"]), i(&v, &["data","refund","remaining_refundable"])), (10000, 55550));
    assert_eq!(v["data"]["cash_out"], true);
    assert_eq!(i(&v, &["data","refund","shift_id"]), a.q::<i64>("SELECT id FROM shifts WHERE user_id=2"));
    assert_eq!((points(&a), a.q::<i64>("SELECT total_spent FROM customers WHERE id=1"), a.q::<i64>("SELECT total_visits FROM customers WHERE id=1")), (56, 55550, 1));

    let full = json!({"client_uuid":"rf-1","amount":55550,"reason":"pelanggan batal","restock":true});
    let (s, v) = a.call("POST", &rf, Some(&ad), Some(full.clone())).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["data"]["refund"]["transaction_status"], "refunded");
    let rid = v["data"]["refund"]["id"].clone();
    assert_eq!((points(&a), a.q::<i64>("SELECT total_spent FROM customers WHERE id=1"), a.q::<i64>("SELECT total_visits FROM customers WHERE id=1")), (50, 0, 0), "poin yang dulu didapat ditarik kembali PERSIS");
    assert_eq!(a.q::<i64>("SELECT stock_quantity FROM products WHERE id=6"), 24, "stok produk kembali");
    assert_eq!(a.q::<f64>("SELECT current_stock FROM ingredients WHERE id=1"), 20000.0, "bahan baku kembali");
    assert_eq!(a.q::<f64>("SELECT current_stock FROM ingredients WHERE id=2"), 60.0);
    assert_eq!(a.q::<i64>("SELECT COUNT(*) FROM inventory_transactions WHERE order_id=2 AND type='return'"), 3);
    assert_eq!(a.q::<String>(&format!("SELECT status||'/'||payment_status FROM orders WHERE id={ord2}")), "completed/refunded");
    let (s, v2) = a.call("POST", &rf, Some(&ad), Some(full)).await;
    assert_eq!((s, &v2["data"]["refund"]["id"]), (StatusCode::CREATED, &rid));
    assert_eq!(a.q::<i64>("SELECT COUNT(*) FROM refunds"), 2);
    assert_eq!(a.q::<i64>("SELECT stock_quantity FROM products WHERE id=6"), 24, "replay tidak boleh restock dua kali");
    let (s, v) = a.call("POST", &rf, Some(&ad), Some(json!({"amount":1,"reason":"lagi"}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["message"].as_str().unwrap().contains("penuh"), "{v}");

    let (_, v) = a.call("GET", "/api/v1/pos/shifts/current", Some(&k), None).await;
    assert_eq!(i(&v, &["data","sales","total_sales"]), 65550, "penjualan bruto tetap tercatat di shift asal");
    assert_eq!((i(&v, &["data","sales","total_cash_refund"]), i(&v, &["data","sales","total_refunds"])), (65550, 2));
    assert_eq!(i(&v, &["data","expected_cash"]), 100000 + 65550 - 65550);
    let (_, v) = a.call("GET", "/api/v1/pos/reports/daily", Some(&k), None).await;
    assert_eq!((i(&v, &["data","gross_sales"]), i(&v, &["data","total_refunds"]), i(&v, &["data","total_sales"]), i(&v, &["data","refund_count"])), (65550, 65550, 0, 2));
    assert_eq!(v["data"]["by_payment_method"][0]["total"], 0);

    assert!(a.state.db.lock().unwrap().execute("UPDATE transactions SET refunded_amount = total_amount + 1 WHERE id=2", []).is_err());

    let (s, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(json!({
        "order":{"items":[{"product_id":4,"qty":2}],"order_type":"dine_in","table_id":2},"payment_method":"cash","amount_paid":11500}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let (trx3, ord3) = (i(&v, &["data","transaction_id"]), i(&v, &["data","order_id"]));
    let rf3 = format!("/api/v1/pos/transactions/{trx3}/refund");
    assert_eq!(a.call("PUT", "/api/v1/admin/settings", Some(&k), Some(json!({"refund_requires_admin":false}))).await.0, StatusCode::FORBIDDEN);

    let (s, _) = a.call("PUT", "/api/v1/admin/settings", Some(&ad), Some(json!({"tax_rate":0.2,"apply_foo":true}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(a.call("PUT", "/api/v1/admin/settings", Some(&ad), Some(json!({"tax_rate":1.5}))).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, v) = a.call("GET", "/api/v1/pos/billing-settings", Some(&k), None).await;
    assert_eq!(v["data"]["tax_rate"], 0.1, "setting tidak boleh berubah setelah permintaan ditolak");
    assert_eq!(v["data"]["refund_requires_admin"], true, "kebijakan refund harus terlihat oleh klien");
    let (s, v) = a.call("PUT", "/api/v1/admin/settings", Some(&ad), Some(json!({"refund_requires_admin":false}))).await;
    assert_eq!((s, &v["data"]["refund_requires_admin"]), (StatusCode::OK, &json!(false)));
    let (s, v) = a.call("POST", &rf3, Some(&k), Some(json!({"amount":500,"reason":"kembalian"}))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(a.q::<i64>("SELECT shift_id FROM refunds ORDER BY id DESC LIMIT 1"), 1, "kasir: dari laci shift sendiri");

    let (_, v) = a.call("GET", "/api/v1/pos/shifts/current", Some(&k), None).await;
    assert_eq!(i(&v, &["data","expected_cash"]), 111000);
    let (s, v) = a.call("POST", "/api/v1/pos/shifts/close", Some(&k), Some(json!({"actual_cash":111000}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (p, b) = void(trx3, "terlambat");
    assert_eq!(a.call("POST", &p, Some(&k), Some(b)).await.0, StatusCode::UNPROCESSABLE_ENTITY, "shift ditutup → tak bisa void");

    let rest = json!({"amount":11000,"reason":"sisa dikembalikan"});
    assert_eq!(a.call("POST", &rf3, Some(&ad), Some(rest.clone())).await.0, StatusCode::CONFLICT);
    assert_eq!(a.call("POST", "/api/v1/pos/shifts/start", Some(&ad), Some(json!({"opening_cash":0}))).await.0, StatusCode::CREATED);
    let (s, v) = a.call("POST", &rf3, Some(&ad), Some(rest)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["data"]["refund"]["transaction_status"], "refunded");
    assert_eq!(a.q::<String>(&format!("SELECT status FROM orders WHERE id={ord3}")), "cancelled", "refund penuh pada pesanan belum selesai membatalkan pesanan");
    assert_eq!(a.q::<String>("SELECT status FROM dining_tables WHERE id=2"), "available");

    let (_, v) = a.call("GET", "/api/v1/pos/shifts/current", Some(&ad), None).await;
    assert_eq!(i(&v, &["data","expected_cash"]), -11000);
    let (_, v) = a.call("GET", "/api/v1/pos/shifts/1/summary", Some(&k), None).await;
    assert_eq!((i(&v, &["data","expected_cash"]), i(&v, &["data","sales","total_sales"]), i(&v, &["data","sales","total_cash_refund"])), (111000, 77050, 66050), "shift tertutup harus beku");

    let (_, v) = a.call("GET", "/api/v1/pos/transactions?status=all", Some(&k), None).await;
    assert!(v["data"].as_array().unwrap().iter().all(|t| t["order_status"].is_string()), "daftar transaksi memuat order_status");
    let (_, v) = a.call("GET", &format!("/api/v1/pos/transactions/{trx2}"), Some(&k), None).await;
    assert_eq!(v["data"]["status"], "refunded");
    assert_eq!(v["data"]["refunds"].as_array().unwrap().len(), 2);
    assert_eq!(v["data"]["refunds"][1]["is_restocked"], true);
}

#[tokio::test]
async fn debt_payment_reversal() {
    let a = app();
    let k = a.login("kasir", "warmangji.ksr").await;
    let ad = a.login("admin", "warmangji.adm").await;
    assert_eq!(a.call("POST", "/api/v1/pos/shifts/start", Some(&k), Some(json!({"opening_cash":100000}))).await.0, StatusCode::CREATED);
    let debt_sale = |order: Value| json!({"order": order, "payment_method":"hutang","amount_paid":0,"customer_id":1});
    let repay = |debt: i64, amt: i64| (format!("/api/v1/pos/debts/{debt}/payments"), json!({"amount": amt, "payment_method":"cash"}));
    let reverse = |pid: i64| format!("/api/v1/pos/debt-payments/{pid}/reverse");

    let (_, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(debt_sale(json!({"items":[{"product_id":3,"qty":1}],"order_type":"dine_in","table_id":1})))).await;
    let trx1 = i(&v, &["data","transaction_id"]);
    assert_eq!(a.q::<i64>("SELECT total_debt FROM customers WHERE id=1"), 34500);
    let (p, b) = repay(1, 10000);
    let (s, _) = a.call("POST", &p, Some(&k), Some(b)).await;
    assert_eq!(s, StatusCode::CREATED);

    let void1 = format!("/api/v1/pos/transactions/{trx1}/void");
    let (s, v) = a.call("POST", &void1, Some(&k), Some(json!({"reason":"salah pelanggan"}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["message"].as_str().unwrap().contains("Batalkan pelunasan"), "{v}");

    assert_eq!(a.call("POST", &reverse(1), Some(&k), Some(json!({"reason":""}))).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, v) = a.call("POST", &reverse(1), Some(&k), Some(json!({"reason":"salah nominal"}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!((i(&v, &["data","remaining_amount"]), v["data"]["debt_status"].as_str()), (34500, Some("OPEN")));
    assert_eq!(i(&v, &["data","total_debt"]), 34500);
    debt_invariant(&a);
    let (_, v) = a.call("GET", "/api/v1/pos/shifts/current", Some(&k), None).await;
    assert_eq!((i(&v, &["data","expected_cash"]), i(&v, &["data","debt_collected","cash"]), i(&v, &["data","debt_reversed","cash"])), (100000, 0, 10000));

    assert_eq!(a.call("POST", &reverse(1), Some(&k), Some(json!({"reason":"lagi"}))).await.0, StatusCode::OK);
    assert_eq!(a.q::<i64>("SELECT COUNT(*) FROM debt_payment_reversals"), 1);
    assert_eq!(a.q::<i64>("SELECT remaining_amount FROM debts WHERE id=1"), 34500, "pembatalan ulang tidak boleh menambah sisa lagi");
    let (_, v) = a.call("GET", "/api/v1/pos/debts/1", Some(&k), None).await;
    assert_eq!(v["data"]["payments"][0]["is_reversed"], true);
    assert_eq!((v["data"]["payments"][0]["user_id"].as_i64(), v["data"]["payments"][0]["shift_id"].as_i64()), (Some(2), Some(1)), "klien butuh user_id/shift_id untuk aturan pembatalan");
    assert_eq!(v["data"]["payments"][0]["reversal_reason"], "salah nominal");

    let (s, v) = a.call("POST", &void1, Some(&k), Some(json!({"reason":"salah pelanggan"}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!((a.q::<String>("SELECT status FROM debts WHERE id=1"), a.q::<i64>("SELECT remaining_amount FROM debts WHERE id=1")), ("VOID".to_string(), 0));
    assert_eq!(a.q::<i64>("SELECT total_debt FROM customers WHERE id=1"), 0);
    assert_eq!((a.q::<i64>("SELECT points FROM customers WHERE id=1"), a.q::<i64>("SELECT total_visits FROM customers WHERE id=1"), a.q::<i64>("SELECT total_spent FROM customers WHERE id=1")), (50, 0, 0));
    assert_eq!(a.q::<String>("SELECT status FROM dining_tables WHERE id=1"), "available");
    debt_invariant(&a);
    let (p, b) = repay(1, 100);
    let (s, v) = a.call("POST", &p, Some(&k), Some(b)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["message"].as_str().unwrap().contains("dibatalkan"), "{v}");
    let (_, v) = a.call("GET", "/api/v1/pos/debts?status=paid", Some(&k), None).await;
    assert_eq!(v["data"].as_array().unwrap().len(), 0, "hutang VOID bukan hutang lunas");
    let (_, v) = a.call("GET", "/api/v1/pos/debts?status=all", Some(&k), None).await;
    assert_eq!(v["data"][0]["status"], "VOID");

    let (_, v) = a.call("POST", "/api/v1/pos/transactions", Some(&k), Some(debt_sale(json!({"items":[{"product_id":1,"qty":1}],"order_type":"take_away"})))).await;
    assert_eq!(i(&v, &["data","total_amount"]), 29500);
    let (p, b) = repay(2, 20000);
    let (s, v) = a.call("POST", &p, Some(&k), Some(b)).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let pay_id = v["data"]["payments"][0]["id"].as_i64().unwrap();
    let (_, v) = a.call("GET", "/api/v1/pos/shifts/current", Some(&k), None).await;
    assert_eq!(i(&v, &["data","expected_cash"]), 120000);
    assert_eq!(a.call("POST", "/api/v1/pos/shifts/close", Some(&k), Some(json!({"actual_cash":120000}))).await.0, StatusCode::OK);
    assert_eq!(a.call("POST", "/api/v1/pos/shifts/start", Some(&k), Some(json!({"opening_cash":50000}))).await.0, StatusCode::CREATED);

    assert_eq!(a.call("POST", &reverse(pay_id), Some(&k), Some(json!({"reason":"koreksi"}))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(a.call("POST", &reverse(pay_id), Some(&ad), Some(json!({"reason":"koreksi"}))).await.0, StatusCode::CONFLICT);
    assert_eq!(a.q::<i64>("SELECT COUNT(*) FROM debt_payment_reversals"), 1, "penolakan tak meninggalkan jejak");
    assert_eq!(a.call("POST", "/api/v1/pos/shifts/start", Some(&ad), Some(json!({"opening_cash":0}))).await.0, StatusCode::CREATED);
    let (s, v) = a.call("POST", &reverse(pay_id), Some(&ad), Some(json!({"reason":"koreksi"}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!((i(&v, &["data","remaining_amount"]), v["data"]["debt_status"].as_str(), i(&v, &["data","total_debt"])), (29500, Some("OPEN"), 29500));
    debt_invariant(&a);

    let (_, v) = a.call("GET", "/api/v1/pos/shifts/current", Some(&ad), None).await;
    assert_eq!((i(&v, &["data","debt_collected","cash"]), i(&v, &["data","expected_cash"])), (-20000, -20000));
    let (_, v) = a.call("GET", "/api/v1/pos/shifts/2/summary", Some(&k), None).await;
    assert_eq!(i(&v, &["data","shift","id"]), 2);
    let (_, v) = a.call("GET", "/api/v1/pos/shifts/1/summary", Some(&k), None).await;
    assert_eq!((i(&v, &["data","debt_collected","cash"]), i(&v, &["data","expected_cash"])), (20000, 120000), "shift tertutup harus beku");

    let (_, v) = a.call("GET", "/api/v1/pos/reports/daily", Some(&k), None).await;
    assert_eq!((i(&v, &["data","debt_collected"]), i(&v, &["data","debt_reversed"])), (0, 30000));

    let (p, b) = repay(2, 29500);
    assert_eq!(a.call("POST", &p, Some(&k), Some(b)).await.0, StatusCode::CREATED);
    assert_eq!(a.q::<i64>("SELECT total_debt FROM customers WHERE id=1"), 0);
    debt_invariant(&a);
}

async fn raw_call(a: &App, token: &str, method: &str, path: &str, content_type: bool, body: &str) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(path).header("authorization", format!("Bearer {token}"));
    if content_type { b = b.header("content-type", "application/json"); }
    let resp = a.router.clone().oneshot(b.body(Body::from(body.to_owned())).unwrap()).await.unwrap();
    let st = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (st, serde_json::from_slice(&bytes).unwrap_or_else(|_| panic!("respons BUKAN JSON: {:?}", String::from_utf8_lossy(&bytes))))
}

async fn new_saved_order(a: &App, token: &str) -> i64 {
    let (_, v) = a.call("POST", "/api/v1/pos/orders", Some(token), Some(json!({"items":[{"product_id":4,"qty":1}]}))).await;
    v["data"]["id"].as_i64().unwrap()
}

#[tokio::test]
async fn optional_body_and_json_errors() {

    let a = app();
    let k = a.login("kasir", "warmangji.ksr").await;
    assert_eq!(a.call("POST", "/api/v1/pos/shifts/start", Some(&k), Some(json!({"opening_cash":1000}))).await.0, StatusCode::CREATED);

    let id = new_saved_order(&a, &k).await;
    let (s, v) = raw_call(&a, &k, "POST", &format!("/api/v1/pos/orders/{id}/cancel"), true, "").await;
    assert_eq!(s, StatusCode::OK, "{v}");

    let id = new_saved_order(&a, &k).await;
    assert_eq!(raw_call(&a, &k, "POST", &format!("/api/v1/pos/orders/{id}/cancel"), false, "").await.0, StatusCode::OK);

    let id = new_saved_order(&a, &k).await;
    assert_eq!(raw_call(&a, &k, "POST", &format!("/api/v1/pos/orders/{id}/cancel"), true, r#"{"reason":"pelanggan pergi"}"#).await.0, StatusCode::OK);
    assert_eq!(a.q::<String>(&format!("SELECT cancel_reason FROM orders WHERE id={id}")), "pelanggan pergi");

    let id = new_saved_order(&a, &k).await;
    let (s, v) = raw_call(&a, &k, "POST", &format!("/api/v1/pos/orders/{id}/cancel"), true, "{rusak").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!((v["success"].clone(), v["code"].clone()), (json!(false), json!("validation")));

    let (s, v) = raw_call(&a, &k, "POST", "/api/v1/pos/shifts/start", true, "").await;
    assert_eq!((s, v["success"].clone()), (StatusCode::UNPROCESSABLE_ENTITY, json!(false)));
}
