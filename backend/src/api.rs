use axum::{
    extract::{FromRequest, Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    routing::{get, patch, post, put},
    Json, Router,
};
use chrono::NaiveDate;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::trace::TraceLayer;

use crate::{
    auth::{self, AuthUser},
    db,
    error::{AppError, Res},
    services::{admin, catalog, customers, debts, expenses, orders, refunds, reports, settings, shifts},
    state::AppState,
    ws,
};

type Out = Res<Json<Value>>;

fn ok<T: Serialize>(data: T) -> Out { Ok(Json(json!({ "success": true, "data": data }))) }
fn ok_msg<T: Serialize>(data: T, message: &str) -> Out { Ok(Json(json!({ "success": true, "message": message, "data": data }))) }

pub struct ApiJson<T>(pub T);
impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = AppError;
    async fn from_request(req: Request, state: &S) -> Result<Self, AppError> {
        Json::<T>::from_request(req, state).await.map(|Json(v)| ApiJson(v))
            .map_err(|e| AppError::validation(format!("Body JSON tidak valid: {}", e.body_text())))
    }
}

pub struct OptJson<T>(pub Option<T>);
impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for OptJson<T> {
    type Rejection = AppError;
    async fn from_request(req: Request, state: &S) -> Result<Self, AppError> {
        let bytes = axum::body::Bytes::from_request(req, state).await
            .map_err(|e| AppError::validation(format!("Body tidak terbaca: {e}")))?;
        if bytes.iter().all(u8::is_ascii_whitespace) { return Ok(OptJson(None)); }
        serde_json::from_slice(&bytes).map(|v| OptJson(Some(v))).map_err(|e| AppError::validation(format!("Body JSON tidak valid: {e}")))
    }
}

fn parse_date(s: Option<&str>) -> Res<Option<NaiveDate>> {
    s.map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| AppError::validation("Format tanggal harus YYYY-MM-DD.")))
        .transpose()
}

pub fn router(state: AppState) -> Router {
    let pos = Router::new()
        .route("/init", get(init))
        .route("/products", get(products))
        .route("/categories", get(categories))
        .route("/tables", get(tables))
        .route("/tables/{id}/status", patch(table_status))
        .route("/tables/move", post(table_move))
        .route("/payment-methods", get(payment_methods))
        .route("/billing-settings", get(billing_settings))
        .route("/dashboard/stats", get(stats))
        .route("/orders", get(list_orders).post(create_order))
        .route("/orders/{id}", get(get_order).put(update_order).patch(update_order))
        .route("/orders/{id}/status", patch(order_status))
        .route("/orders/{id}/complete", post(complete_order))
        .route("/orders/{id}/cancel", post(cancel_order))
        .route("/transactions", get(list_transactions).post(pay))
        .route("/transactions/{id}", get(get_transaction))
        .route("/transactions/{id}/void", post(void_transaction))
        .route("/transactions/{id}/refund", post(refund_transaction))
        .route("/debt-payments/{id}/reverse", post(reverse_debt_payment))
        .route("/customers", get(list_customers).post(create_customer))
        .route("/customers/{id}", put(update_customer).delete(delete_customer))
        .route("/customers/{id}/history", get(customer_history))
        .route("/debts", get(list_debts))
        .route("/debts/{id}", get(get_debt))
        .route("/debts/{id}/payments", post(pay_debt))
        .route("/customers/{id}/debt-payments", post(pay_customer_debts))
        .route("/shifts/current", get(shift_current))
        .route("/shifts/history", get(shift_history))
        .route("/shifts/start", post(shift_start))
        .route("/shifts/close", post(shift_close))
        .route("/shifts/cash-movement", post(shift_cash))
        .route("/shifts/{id}/summary", get(shift_summary))
        .route("/expenses", get(list_expenses).post(create_expense))
        .route("/expenses/{id}", axum::routing::delete(delete_expense))
        .route("/reports/daily", get(report_daily));

    let adm = Router::new()
        .route("/categories", post(admin_category))
        .route("/categories/{id}", put(admin_category_update))
        .route("/products", post(admin_product))
        .route("/products/{id}", put(admin_product_update).delete(admin_product_delete))
        .route("/products/{id}/stock", post(admin_stock))
        .route("/products/{id}/stock-history", get(admin_stock_history))
        .route("/tables", post(admin_table))
        .route("/tables/{id}", put(admin_table_update))
        .route("/users", get(admin_users).post(admin_user))
        .route("/users/{id}", put(admin_user_update))
        .route("/setup", post(admin_setup))
        .route("/settings", put(admin_settings));

    Router::new()
        .route("/api/v1/health", get(|| async { Json(json!({"status":"ok","version":env!("CARGO_PKG_VERSION")})) }))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/ws", get(ws::handler))
        .nest("/api/v1/pos", pos)
        .nest("/api/v1/admin", adm)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Deserialize)]
struct LoginBody { username: String, password: String }

async fn login(State(st): State<AppState>, headers: HeaderMap, ApiJson(b): ApiJson<LoginBody>) -> Out {
    let client = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).unwrap_or("local").to_owned();
    let st2 = st.clone();
    let data = st.db(move |c, _| auth::login(c, &st2, &b.username, &b.password, &client)).await?;
    Ok(Json(json!({ "success": true, "message": "Login berhasil", "data": data })))
}

async fn me(user: AuthUser) -> Out { ok(json!({ "id": user.id, "name": user.name, "role": user.role })) }

async fn init(user: AuthUser, State(st): State<AppState>) -> Out {
    let u = user.clone();
    let data = st.db(move |c, env| {
        Ok(json!({
            "user": { "id": u.id, "name": u.name, "role": u.role },
            "products": catalog::products(c, &Default::default())?,
            "categories": catalog::categories(c)?,
            "tables": catalog::tables(c)?,
            "active_orders": orders::list_orders(c, env, "active", None)?,
            "current_shift": shifts::current(c, &u)?,
            "stats": reports::stats(c, env)?,
            "billing_settings": settings::billing_json(c)?,
            "shop_settings": settings::shop(c)?,
            "payment_methods": catalog::payment_methods(c)?,
            "customers": customers::list(c, None, 50)?,
        }))
    }).await?;
    ok(data)
}

#[derive(Deserialize)]
struct ProductParams { q: Option<String>, category_id: Option<i64>, barcode: Option<String> }

async fn products(_u: AuthUser, State(st): State<AppState>, Query(p): Query<ProductParams>) -> Out {
    ok(st.db(move |c, _| catalog::products(c, &catalog::ProductQuery { q: p.q, category_id: p.category_id, barcode: p.barcode })).await?)
}
async fn categories(_u: AuthUser, State(st): State<AppState>) -> Out { ok(st.db(|c, _| catalog::categories(c)).await?) }
async fn tables(_u: AuthUser, State(st): State<AppState>) -> Out { ok(st.db(|c, _| catalog::tables(c)).await?) }
async fn payment_methods(_u: AuthUser, State(st): State<AppState>) -> Out { ok(st.db(|c, _| catalog::payment_methods(c)).await?) }
async fn billing_settings(_u: AuthUser, State(st): State<AppState>) -> Out { ok(st.db(|c, _| settings::billing_json(c)).await?) }
async fn stats(_u: AuthUser, State(st): State<AppState>) -> Out { ok(st.db(|c, env| reports::stats(c, env)).await?) }

#[derive(Deserialize)]
struct StatusBody { status: String }

async fn table_status(_u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<StatusBody>) -> Out {
    st.db(move |c, env| {
        admin::set_table_status(c, id, &b.status)?;
        env.emit("table.updated", json!({"id": id, "status": b.status}));
        Ok(())
    }).await?;
    ok(json!({"id": id}))
}

#[derive(Deserialize)]
struct MoveBody { order_id: i64, to_table_id: i64 }
async fn table_move(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<MoveBody>) -> Out {
    st.db(move |c, env| catalog::move_order_table(c, env, &u, b.order_id, b.to_table_id)).await?;
    ok(json!({"order_id": b.order_id, "table_id": b.to_table_id}))
}

#[derive(Deserialize)]
struct OrderListParams { status: Option<String>, date: Option<String> }

async fn list_orders(_u: AuthUser, State(st): State<AppState>, Query(p): Query<OrderListParams>) -> Out {
    let date = parse_date(p.date.as_deref())?;
    let status = p.status.unwrap_or_else(|| "active".into());
    ok(st.db(move |c, env| orders::list_orders(c, env, &status, date)).await?)
}

async fn create_order(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<orders::OrderIn>) -> Result<(StatusCode, Json<Value>), AppError> {
    let out = st.db(move |c, env| {
        let tx = c.transaction()?;
        let id = orders::create_order(&tx, env, &u, &b, "pending")?;
        let o = orders::get_order(&tx, id)?;
        tx.commit()?;
        env.emit("order.updated", json!({"id": id}));
        if let Some(t) = o["dining_table_id"].as_i64() { env.emit("table.updated", json!({"id": t, "status": "occupied", "current_order_id": id})); }
        Ok(o)
    }).await?;
    Ok((StatusCode::CREATED, Json(json!({ "success": true, "message": "Pesanan dibuat", "data": out }))))
}

async fn get_order(_u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>) -> Out { ok(st.db(move |c, _| orders::get_order(c, id)).await?) }

async fn update_order(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<orders::OrderIn>) -> Out {
    ok(st.db(move |c, env| orders::update_order(c, env, &u, id, &b)).await?)
}
async fn order_status(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<StatusBody>) -> Out {
    ok(st.db(move |c, env| orders::set_status(c, env, &u, id, &b.status)).await?)
}
async fn complete_order(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>) -> Out {
    ok_msg(st.db(move |c, env| orders::complete(c, env, &u, id)).await?, "Pesanan berhasil ditandai selesai")
}
#[derive(Deserialize, Default)]
struct CancelBody { reason: Option<String> }
async fn cancel_order(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, body: OptJson<CancelBody>) -> Out {
    let reason = body.0.and_then(|b| b.reason);
    ok_msg(st.db(move |c, env| orders::cancel(c, env, &u, id, reason.as_deref())).await?, "Pesanan dibatalkan")
}

async fn pay(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<orders::PayIn>) -> Result<(StatusCode, Json<Value>), AppError> {
    let is_debt = matches!(b.payment_method.trim().to_lowercase().as_str(), "hutang" | "debt" | "piutang");
    let out = st.db(move |c, env| orders::pay(c, env, &u, &b)).await?;
    let msg = if is_debt { "Pesanan hutang berhasil" } else { "Pembayaran berhasil" };
    Ok((StatusCode::CREATED, Json(json!({ "success": true, "message": msg, "data": out }))))
}

#[derive(Deserialize)]
struct TrxParams { date: Option<String>, end_date: Option<String>, status: Option<String>, payment_method: Option<String>, search: Option<String> }

async fn list_transactions(_u: AuthUser, State(st): State<AppState>, Query(p): Query<TrxParams>) -> Out {
    let from = parse_date(p.date.as_deref())?;
    let to = parse_date(p.end_date.as_deref())?;
    let out = st.db(move |c, env| {
        let today = db::local_date(db::now(), env.tz_offset_min);
        let from = from.unwrap_or(today);
        orders::list_transactions(c, env, &orders::TrxQuery { from, to: to.unwrap_or(from), status: p.status, method: p.payment_method, search: p.search })
    }).await?;
    Ok(Json(json!({ "success": true, "data": out["data"], "summary": out["summary"] })))
}

async fn get_transaction(_u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>) -> Out { ok(st.db(move |c, _| refunds::get_transaction(c, id)).await?) }

async fn void_transaction(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<refunds::VoidIn>) -> Out {
    ok_msg(st.db(move |c, env| refunds::void_transaction(c, env, &u, id, &b)).await?, "Transaksi di-void")
}
async fn refund_transaction(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<refunds::RefundIn>) -> Result<(StatusCode, Json<Value>), AppError> {
    let out = st.db(move |c, env| refunds::refund(c, env, &u, id, &b)).await?;
    Ok((StatusCode::CREATED, Json(json!({ "success": true, "message": "Refund dicatat", "data": out }))))
}
async fn reverse_debt_payment(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<debts::ReverseIn>) -> Out {
    ok_msg(st.db(move |c, env| debts::reverse_payment(c, env, &u, id, &b)).await?, "Pelunasan dibatalkan")
}

#[derive(Deserialize)]
struct CustomerParams { q: Option<String>, search: Option<String>, limit: Option<i64> }

async fn list_customers(_u: AuthUser, State(st): State<AppState>, Query(p): Query<CustomerParams>) -> Out {
    ok(st.db(move |c, _| customers::list(c, p.q.as_deref().or(p.search.as_deref()), p.limit.unwrap_or(50))).await?)
}
async fn create_customer(_u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<Value>) -> Result<(StatusCode, Json<Value>), AppError> {
    let out = st.db(move |c, _| customers::save(c, None, &b)).await?;
    Ok((StatusCode::CREATED, Json(json!({ "success": true, "data": out }))))
}
async fn update_customer(_u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<Value>) -> Out {
    ok(st.db(move |c, _| customers::save(c, Some(id), &b)).await?)
}
async fn delete_customer(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>) -> Out {
    u.require_admin()?;
    st.db(move |c, _| customers::delete(c, id)).await?;
    ok(json!({"id": id}))
}
async fn customer_history(_u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>) -> Out { ok(st.db(move |c, _| customers::history(c, id)).await?) }

#[derive(Deserialize)]
struct DebtParams { status: Option<String>, customer_id: Option<i64> }

async fn list_debts(_u: AuthUser, State(st): State<AppState>, Query(p): Query<DebtParams>) -> Out {
    let status = p.status.unwrap_or_else(|| "open".into());
    ok(st.db(move |c, env| debts::list(c, env, &status, p.customer_id)).await?)
}
async fn get_debt(_u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>) -> Out { ok(st.db(move |c, env| debts::get(c, env, id)).await?) }

async fn pay_debt(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<debts::RepayIn>) -> Result<(StatusCode, Json<Value>), AppError> {
    let out = st.db(move |c, env| debts::repay(c, env, &u, debts::Scope::Debt(id), &b)).await?;
    Ok((StatusCode::CREATED, Json(json!({ "success": true, "message": "Pembayaran hutang dicatat", "data": out }))))
}
async fn pay_customer_debts(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<debts::RepayIn>) -> Result<(StatusCode, Json<Value>), AppError> {
    let out = st.db(move |c, env| debts::repay(c, env, &u, debts::Scope::Customer(id), &b)).await?;
    Ok((StatusCode::CREATED, Json(json!({ "success": true, "message": "Pembayaran hutang dicatat", "data": out }))))
}

async fn shift_current(u: AuthUser, State(st): State<AppState>) -> Out { ok(st.db(move |c, _| shifts::current(c, &u)).await?) }
async fn shift_history(u: AuthUser, State(st): State<AppState>) -> Out { ok(st.db(move |c, _| shifts::history(c, &u, 30)).await?) }
async fn shift_summary(_u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>) -> Out { ok(st.db(move |c, _| shifts::summary(c, id)).await?) }

#[derive(Deserialize)]
struct StartBody { opening_cash: i64, shift_type: Option<String>, notes: Option<String> }
async fn shift_start(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<StartBody>) -> Result<(StatusCode, Json<Value>), AppError> {
    let out = st.db(move |c, env| shifts::start(c, env, &u, b.opening_cash, b.shift_type.as_deref(), b.notes.as_deref())).await?;
    Ok((StatusCode::CREATED, Json(json!({ "success": true, "message": "Shift dibuka", "data": out }))))
}
#[derive(Deserialize)]
struct CloseBody { shift_id: Option<i64>, actual_cash: i64, notes: Option<String> }
async fn shift_close(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<CloseBody>) -> Out {
    let out = st.db(move |c, env| {
        let id = match b.shift_id { Some(id) => id, None => shifts::open_shift_id(c, u.id)?.ok_or_else(|| AppError::conflict("Tidak ada shift yang terbuka."))? };
        shifts::close(c, env, &u, id, b.actual_cash, b.notes.as_deref())
    }).await?;
    ok_msg(out, "Shift ditutup")
}
#[derive(Deserialize)]
struct CashBody { r#type: String, amount: i64, description: Option<String> }
async fn shift_cash(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<CashBody>) -> Out {
    ok(st.db(move |c, _| shifts::cash_movement(c, &u, &b.r#type, b.amount, b.description.as_deref().unwrap_or(""))).await?)
}

#[derive(Deserialize)]
struct ExpenseParams { all: Option<bool> }
async fn list_expenses(u: AuthUser, State(st): State<AppState>, Query(p): Query<ExpenseParams>) -> Out {
    ok(st.db(move |c, _| expenses::list(c, &u, p.all.unwrap_or(false))).await?)
}
#[derive(Deserialize)]
struct ExpenseBody { category: String, description: Option<String>, amount: i64 }
async fn create_expense(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<ExpenseBody>) -> Result<(StatusCode, Json<Value>), AppError> {
    let out = st.db(move |c, env| expenses::create(c, env, &u, &b.category, b.description.as_deref(), b.amount)).await?;
    Ok((StatusCode::CREATED, Json(json!({ "success": true, "data": out }))))
}
async fn delete_expense(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>) -> Out {
    st.db(move |c, _| expenses::delete(c, &u, id)).await?;
    ok(json!({"id": id}))
}

#[derive(Deserialize)]
struct DailyParams { date: Option<String> }
async fn report_daily(_u: AuthUser, State(st): State<AppState>, Query(p): Query<DailyParams>) -> Out {
    let date = parse_date(p.date.as_deref())?;
    ok(st.db(move |c, env| reports::daily(c, env, date.unwrap_or_else(|| db::local_date(db::now(), env.tz_offset_min)))).await?)
}

async fn admin_category(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<Value>) -> Out { ok(st.db(move |c, _| admin::save_category(c, &u, None, &b)).await?) }
async fn admin_category_update(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<Value>) -> Out { ok(st.db(move |c, _| admin::save_category(c, &u, Some(id), &b)).await?) }
async fn admin_product(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<Value>) -> Out { ok(st.db(move |c, _| admin::save_product(c, &u, None, &b)).await?) }
async fn admin_product_update(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<Value>) -> Out { ok(st.db(move |c, _| admin::save_product(c, &u, Some(id), &b)).await?) }
async fn admin_product_delete(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>) -> Out { st.db(move |c, _| admin::deactivate_product(c, &u, id)).await?; ok(json!({"id": id})) }
#[derive(Deserialize)]
struct StockBody { delta: i64, notes: Option<String> }
async fn admin_stock(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<StockBody>) -> Out {
    ok(st.db(move |c, _| { u.require_admin()?; admin::adjust_stock(c, &u, id, b.delta, b.notes.as_deref()) }).await?)
}
async fn admin_stock_history(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>) -> Out {
    u.require_admin()?;
    ok(st.db(move |c, _| admin::stock_history(c, id)).await?)
}
async fn admin_table(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<Value>) -> Out { ok(st.db(move |c, _| admin::save_table(c, &u, None, &b)).await?) }
async fn admin_table_update(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<Value>) -> Out { ok(st.db(move |c, _| admin::save_table(c, &u, Some(id), &b)).await?) }
async fn admin_users(u: AuthUser, State(st): State<AppState>) -> Out { ok(st.db(move |c, _| admin::list_users(c, &u)).await?) }
async fn admin_user(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<Value>) -> Out { ok(st.db(move |c, _| admin::save_user(c, &u, None, &b)).await?) }
async fn admin_user_update(u: AuthUser, State(st): State<AppState>, Path(id): Path<i64>, ApiJson(b): ApiJson<Value>) -> Out { ok(st.db(move |c, _| admin::save_user(c, &u, Some(id), &b)).await?) }
async fn admin_setup(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<Value>) -> Out { ok(st.db(move |c, _| admin::save_setup(c, &u, &b)).await?) }
async fn admin_settings(u: AuthUser, State(st): State<AppState>, ApiJson(b): ApiJson<Value>) -> Out { ok(st.db(move |c, _| admin::save_settings(c, &u, &b)).await?) }
