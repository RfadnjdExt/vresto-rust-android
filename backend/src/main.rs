use vresto::{api, config::Config, db, seed, state::AppState};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()))
        .init();

    let cfg = Config::from_env();
    let conn = db::open(&cfg.database_path).expect("gagal membuka database");
    if std::env::args().any(|a| a == "--seed-demo") {
        match seed::demo(&conn) {
            Ok(true) => tracing::warn!("Data demo dibuat (admin / warmangji.adm, kasir / warmangji.ksr) — GANTI password ini sebelum dipakai sungguhan."),
            Ok(false) => tracing::info!("Seed dilewati: database sudah berisi user."),
            Err(e) => panic!("seed gagal: {e:?}"),
        }
    }

    let bind = cfg.bind.clone();
    let app = api::router(AppState::new(conn, cfg));
    let listener = tokio::net::TcpListener::bind(&bind).await.expect("gagal bind port");
    tracing::info!("VRESTO server berjalan di http://{bind}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; tracing::info!("shutdown…"); })
        .await
        .expect("server error");
}
