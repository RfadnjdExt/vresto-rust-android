# Migrasi VRESTO: Laravel + Alpine.js → Rust + Android native

## 1. Status — apa yang benar-benar sudah diverifikasi

| Bagian | Status | Bukti |
|---|---|---|
| Backend Rust (Axum + SQLite) | **Dikompilasi & diuji** | `cargo test`: 8 tes integrasi E2E + 3 tes kalkulasi lulus; smoke test pada binary asli + WebSocket; uji mutasi & uji paralel pada server nyata (lihat §3.11–3.12) |
| Kalkulasi uang Kotlin | **Dikompilasi & diuji** (kotlinc 2.0.21) | 408 dari 412 vektor identik persis, 4 selisih Rp1 yang dijelaskan di §3.1, 0 salah |
| Layout struk, bukti refund & bukti pelunasan hutang (ESC/POS) | **Dikompilasi & diuji** | semua baris ≤ 32 kolom, byte INIT/CUT benar |
| Aplikasi Android — semua layar (kasir, pesanan, meja, **riwayat, hutang, void, refund**, shift, cetak struk/bukti) | **Ditulis, BELUM dikompilasi/dijalankan** | Sandbox tidak punya Android SDK/Gradle/Maven Google. Yang *sudah* dibuktikan: (a) **kontrak** — `android/tools/contract_check.py` memvalidasi 37 respons server asli terhadap 51 DTO Kotlin: 0 kegagalan (pemeriksanya sendiri diuji dengan 4 kesalahan yang disuntikkan, semuanya terdeteksi); (b) **semua berkas parse tanpa error sintaks** (kotlinc); (c) logika murni dikompilasi & diuji — 14 tes JUnit (aturan tombol Void/Refund/Batalkan, slip cetak, format waktu) + tes kalkulasi. Kode **Compose/Hilt/Retrofit belum pernah dikompilasi penuh**: harapkan beberapa error import/versi di Android Studio |
| Printer Bluetooth | **Belum diuji dengan printer fisik** | |

Jangan anggap APK-nya siap produksi. Yang sudah solid adalah backend dan logika uangnya.

## 2. Arsitektur

```
Android (Kotlin, Compose, Hilt)  ──HTTP/JSON + WebSocket──▶  vresto-server (Rust, Axum)
  Retrofit · OkHttp WS · DataStore                              rusqlite (SQLite WAL) · JWT · bcrypt
  ESC/POS via Bluetooth SPP                                     logika bisnis sinkron di src/services/*
```
Server berjalan di satu mesin di toko (mini-PC/laptop/Raspberry Pi), HP/tablet kasir tersambung lewat Wi-Fi LAN. SQLite satu penulis memang cukup untuk satu toko; seluruh akses DB lewat satu koneksi.

> **Bukan offline-first.** Pilihan awal Anda (Room + backend ditulis ulang di perangkat) diganti "backend Rust". Jadi kasir butuh koneksi ke server toko. Fondasi untuk antrean offline sudah ada (`client_uuid` idempoten di `/transactions`) tetapi **outbox belum dibuat**.

## 3. Perubahan perilaku (keputusan saya — tolong tinjau)

Sumber kebenaran yang saya pakai = apa yang kasir benar-benar lihat & bayar (`computed.js`), bukan `CalculationService.php`, karena keduanya berbeda.

1. **Uang = integer rupiah**, pembulatan half-up dengan aritmetika integer. 4 dari 412 vektor uji berbeda Rp1 dari kode lama: diskon 33,3% yang hasilnya persis `x,5`. JS float menghasilkan `x,4999…` → bulat ke bawah; integer benar → ke atas. Ini galat float di kode lama.
2. **Service charge dihitung dari subtotal setelah diskon** (klien lama). Server lama memakai subtotal sebelum diskon → total bisa berbeda antara layar dan struk. Jika Anda ingin versi server lama, ubah satu baris di `money.rs` dan di `Money.kt`.
3. **Harga selalu dihitung ulang server dari katalog.** Kode lama memercayai `price` dari browser.
4. **`takeaway` vs `take_away`**: kode lama mencampur keduanya sehingga biaya kemasan bisa hilang diam-diam. Sekarang dinormalisasi.
5. **Metode pembayaran tak dikenal ditolak** (dulu diam-diam dianggap tunai). **Uang kurang ditolak** (dulu tidak divalidasi).
6. **Pembayaran wajib shift terbuka** (409). Satu shift terbuka per kasir, ditegakkan unique index DB.
7. **Nomor antrean hanya untuk take-away** (dulu semua order memakai nomor → nomor take-away loncat).
8. **Endpoint yang dulu publik kini wajib login** (stats, pelanggan, katalog).
9. Dipertahankan: stok berkurang saat order **selesai** (bukan saat bayar), bundle diurai rekursif, bahan baku dari resep, 1 poin = Rp100, 1 poin per Rp10.000 belanja, level member 1jt/5jt/10jt, aturan limit hutang, rumus kas shift, hash password bcrypt `$2y$` (user lama bisa login tanpa reset).
10. Stok tidak memblokir penjualan saat minus (sama seperti dulu); hanya ditandai `is_low_stock`.
11. **Pelunasan hutang (baru di versi ini).** Tabel `debt_payments` (append-only, migrasi `0002`, otomatis dijalankan saat server start; DB lama aman). Pembayaran per hutang atau per pelanggan (FIFO), cicilan boleh, `client_uuid` idempoten, dasar kas (lihat `docs/API.md` bagian Hutang). Pembuktian: tes E2E dengan angka dihitung manual; **uji mutasi** (FIFO dibalik / non-tunai ikut kas laci → tes gagal seperti seharusnya); pada server nyata 6 permintaan paralel ke sisa hutang 29.500 → tepat 1 lolos, dan 6 permintaan ber-`client_uuid` sama → uang tercatat sekali. Trigger SQLite menjaga `0 ≤ sisa ≤ nilai awal`. Bonus: jatuh tempo hutang kini memakai tanggal lokal toko (sebelumnya tanggal UTC).
12. **Void, refund, pembatalan pelunasan (baru).** Migrasi `0003`; semua append-only. Penjualan dicatat **bruto** di shift & tanggal aslinya; refund/pembatalan dicatat di shift & tanggal tempat uang keluar → **shift tertutup & laporan hari lampau tak pernah berubah** (tanpa ini, refund akan menggeser shift asal secara retroaktif — perhitungan shift/laporan versi sebelumnya memang punya cacat itu dan sudah diperbaiki). Kebijakan: void = kasir (shift sama, pesanan belum selesai); refund = admin secara default (`PUT /admin/settings`). Pembuktian: tes E2E berangka manual (poin kembali persis 6→0, stok & bahan baku kembali persis, kas shift −11.000 di laci admin sementara shift kasir tetap 111.000); **4 uji mutasi** (refund di shift asal / penjualan bukan-bruto / poin tak dikembalikan saat void / otorisasi pembatalan dihapus → semuanya membuat tes gagal); pada server nyata: 6 refund paralel ke Rp41.400 → tepat 4 lolos, 6 void paralel → poin kembali sekali, 6 pembatalan paralel → sisa pulih sekali. Trigger DB menjaga `0 ≤ refunded ≤ total`. **Perubahan semantik laporan harian**: `total_sales` kini = bruto − refund hari itu (dasar kas); `by_order_type` & `top_products` bruto.
13. **Body request opsional & error selalu JSON (perbaikan, ditemukan oleh pemeriksa kontrak).** `POST /orders/{id}/cancel` tanpa body, tetapi dengan `Content-Type: application/json`, dulu dijawab **teks biasa** oleh Axum 0.8 (melanggar janji "semua error berformat JSON" di `API.md`; klien OkHttp/Retrofit bisa mengirim kombinasi itu). Sekarang body kosong = tidak ada, body rusak = `422` JSON standar. Tes + uji mutasi (kode lama → tes gagal).
14. **Field baru untuk UI** (hanya tambahan, kompatibel): `billing-settings.refund_requires_admin` (agar tombol Refund tampil benar), `debts/{id}.payments[].user_id/shift_id` (agar tombol Batalkan tampil hanya untuk yang berhak), `transactions[].order_status`, `stats.receivables_total`.

## 4. BELUM dipindahkan / belum terbukti (jujur)

**Menghalangi pemakaian nyata:**
- **App Android belum pernah dikompilasi atau dijalankan** (lihat §1) — termasuk layar Riwayat, Hutang, Void, Refund. Backend dan kontraknya terbukti; UI-nya belum.
- **Impor data lama** — tidak ada skrip pemindah data dari DB Laravel (MySQL/SQLite) ke skema baru. User/produk/pelanggan harus diimpor atau diinput ulang.
- **UI admin** — Filament hilang. Tersedia REST `/admin/*` (produk, kategori, meja, user, setup, `PUT /admin/settings` untuk tarif & kebijakan refund) tetapi **belum ada layar**; saat ini menu/meja/user hanya bisa dikelola lewat API.
- **Printer fisik belum diuji** (cetak struk, bukti refund, bukti pelunasan).

**Keterbatasan yang disengaja** (demi keamanan kas; bisa dilonggarkan bila perlu):
- Refund selalu ke metode bayar asli (pembelian tunai tak bisa direfund ke QRIS); `restock` hanya bila refund membuat total penuh; transaksi poin hanya refund penuh; transaksi hutang tak bisa direfund (void, atau batalkan pelunasannya dulu).
- Pesanan yang sudah dibayar tak bisa **diedit isinya** (void lalu buat ulang).
- Admin tanpa shift memproses refund tunai dari shift terbuka terbaru — kasir shift itu akan melihat uang keluar; audit log mencatat pelakunya.

**Fitur yang belum ada:** penghapusan piutang macet (write-off), denda keterlambatan, saldo titipan bila bayar lebih, persetujuan dua orang untuk refund besar, cetak ulang bukti untuk transaksi lama, split bill, reservasi, gabung meja, mesin promo otomatis, layar dapur/KDS, laporan selain harian, opname/pembelian bahan, karaoke, multi-cabang, edit pesanan tersimpan dari app (server mendukung `PUT`, UI belum), autentikasi lisensi (Ed25519 + sidik jari perangkat).

## 5. Keamanan — perlu tindakan Anda

- `.env.example` di repo lama berisi **`LICENSE_SECRET_KEY` yang tampak asli**. Anggap bocor: rotasi, hapus dari riwayat git.
- `--seed-demo` membuat user `admin / warmangji.adm` dan `kasir / warmangji.ksr` (sama dengan README lama). **Ganti sebelum dipakai**; jangan jalankan seed di produksi.
- Android mengizinkan **cleartext HTTP** (server LAN tanpa TLS) → token terlihat di jaringan. Untuk akses di luar LAN: taruh reverse proxy TLS (Caddy) dan matikan cleartext (`network_security_config.xml`).
- Token WebSocket lewat query string (`?token=`): jangan jalankan server dengan `RUST_LOG=debug` di produksi (URI ikut tercatat).
- Throttle login memakai `username + X-Forwarded-For` (atau "local"): penyerang bisa mengunci username tertentu 1 menit. Token disimpan di DataStore biasa; pindahkan ke Keystore untuk perangkat bersama.

## 6. Peta Laravel → baru

| Laravel | Sekarang |
|---|---|
| `routes/api.php`, `Api/Pos/*Controller` | `backend/src/api.rs` (handler tipis) |
| `CalculationService`, `computed.js` | `backend/src/money.rs` + `android/.../core/Money.kt` (vektor uji bersama) |
| `TransactionController`, `PosOrderController`, `OrderObserver` | `services/orders.rs`, `services/stock.rs` |
| `ShiftService` | `services/shifts.rs` |
| `BusinessFeatureConfig`, `Setting` | `services/settings.rs` |
| Filament Resources | `services/admin.rs` (REST, belum ada UI) |
| Laravel Reverb/Echo | `ws.rs` + `data/Realtime.kt` (auto-reconnect + `resync`) |
| 110 migrasi | `migrations/0001_init.sql` (skema akhir yang dipakai alur POS) |
| `thermal-bluetooth-printer.js` (Web Bluetooth/USB) | `print/EscPos.kt` + `BluetoothPrinter.kt` (SPP) |
| Alpine.js `resources/js/pos/*` | Compose `ui/*` + `PosViewModel` |
| Session cookie + CSRF | JWT Bearer |

## 7. Langkah berikut yang saya sarankan (urut)

1. Buka `android/` di Android Studio → perbaiki error kompilasi → `./gradlew test` (tes kalkulasi, aturan tombol, slip cetak) → coba di emulator melawan server lokal. Setelah mengubah DTO/server, jalankan `python3 android/tools/contract_check.py` (butuh `cargo build` dulu).
2. ~~Pelunasan hutang, void/refund, pembatalan pelunasan, layar Android-nya~~ (selesai; layar belum dikompilasi). Berikutnya: **layar Admin** (produk/kategori/meja/user/pengaturan) — backend `/admin/*` sudah ada.
3. Skrip impor data Laravel → SQLite baru.
4. UI admin (produk/meja/user).
5. Outbox offline di Android (Room) memakai `client_uuid`.
6. Uji dengan printer thermal fisik 58mm; jika lebar 80mm, ubah `EscPos.COLS` ke 48.
