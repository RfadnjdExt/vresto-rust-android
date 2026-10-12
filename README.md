# VRESTO — Rust backend + Android native

```
backend/   Server Rust (Axum + SQLite).  cargo test  ·  cargo run --release -- --seed-demo
android/   App kasir Kotlin + Jetpack Compose.  Buka di Android Studio (Ladybug+, JDK 17)
docs/      MIGRATION.md (BACA DULU) · API.md
```

**Jalankan backend**
```bash
cd backend
cargo test
VRESTO_JWT_SECRET=$(openssl rand -hex 32) cargo run --release -- --seed-demo
```
Variabel lingkungan: lihat `backend/.env.example`. Butuh Rust ≥ 1.85.

**Jalankan app**: buka folder `android/` di Android Studio, sync Gradle, jalankan di perangkat satu Wi-Fi dengan server, isi alamat `192.168.x.x:8000` di layar login. Emulator → `10.0.2.2:8000`.
Gradle wrapper (jar) tidak disertakan; Android Studio membuatnya otomatis, atau jalankan `gradle wrapper`.

Status verifikasi & daftar fitur yang belum dipindahkan ada di **docs/MIGRATION.md**.

**Memeriksa kontrak Android ↔ server tanpa Android SDK**
```bash
cd backend && cargo build
cd ../android && python3 tools/contract_check.py
```
Melaporkan field wajib yang hilang, tipe salah, dan `@SerialName` yang salah ketik (yang tidak crash tapi diam-diam selalu bernilai default).
