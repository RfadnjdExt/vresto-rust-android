# API VRESTO (Rust) — v1

Base: `http://<server>:8000/api/v1`. Semua respons JSON.
Sukses: `{"success":true,"data":…,"message"?}` · Error: `{"success":false,"code":"…","message":"…"}`

| HTTP | `code` | Arti |
|---|---|---|
| 401 | `unauthorized` | token hilang/kedaluwarsa/akun nonaktif → login ulang |
| 403 | `forbidden` | butuh role admin / milik kasir lain |
| 404 | `not_found` | |
| 409 | `conflict` | mis. **shift belum dibuka**, shift ganda, constraint DB |
| 422 | `validation` / `business` | input salah / aturan bisnis (limit hutang, uang kurang, meja terisi, …) |

Auth: `Authorization: Bearer <jwt>`. WebSocket tidak bisa kirim header di semua klien → `?token=<jwt>`.
Body JSON **opsional** pada endpoint yang tak butuh data (mis. `cancel`, `complete`): kosong dianggap tidak ada, walau `Content-Type: application/json` dikirim. Body rusak → `422` JSON.
Semua uang = **integer rupiah**. Waktu = RFC3339 UTC.

## Auth
- `POST /auth/login` `{username|email, password}` → `{token, expires_in, user}` (throttle 5/menit)
- `GET /auth/me`

## Bootstrap
- `GET /pos/init` → user, products, categories, tables, active_orders, current_shift, stats, billing_settings, shop_settings, payment_methods, customers. **Satu panggilan untuk membuka layar kasir.**

## Katalog & meja
- `GET /pos/products?q=&category_id=&barcode=` (varian, modifier, `effective_price`, `is_low_stock`)
- `GET /pos/categories` · `GET /pos/tables` · `GET /pos/payment-methods`
- `GET /pos/billing-settings` → tarif/biaya + `refund_requires_admin` (klien memakainya untuk menampilkan tombol Refund)
- `PATCH /pos/tables/{id}/status` `{status}` · `POST /pos/tables/move` `{order_id,to_table_id}`

## Order
Body order (`OrderIn`): `items[{product_id,variant_id?,qty,note?,modifiers[{id,quantity}],takeaway?}]`, `order_type` (`dine_in|take_away|delivery|gofood|grabfood|shopeefood|karaoke`; `takeaway`/`Take Away` diterima), `table_id?`, `customer_id?`, `customer_name?`, `guest_count?`, `discount_type?` (`percent|fixed|nominal`), `discount_value?`, `shipping_fee?`, `points_used?`, `notes?`.
**Harga tidak dikirim klien** — server menghitung dari katalog.

- `GET /pos/orders?status=active|completed|cancelled|all&date=YYYY-MM-DD` · `GET /pos/orders/{id}`
- `POST /pos/orders` → order `pending`, meja jadi `occupied`
- `PUT /pos/orders/{id}` ganti isi (hanya **belum dibayar**)
- `PATCH /pos/orders/{id}/status` `{status: confirmed|preparing|ready|served}`
- `POST /pos/orders/{id}/complete` (harus lunas; **stok dikurangi di sini**; meja dibebaskan)
- `POST /pos/orders/{id}/cancel` `{reason?}` (hanya belum dibayar)

## Pembayaran
`POST /pos/transactions`
```json
{ "client_uuid": "uuid-dibuat-sekali-per-upaya",
  "order_id": 12,                       
  "payment_method": "cash|qris|transfer|debit|credit|card|ewallet|debt|points",
  "amount_paid": 100000, "customer_id": 1 }
```
`client_uuid` membuat request **idempoten**: kirim ulang setelah timeout → transaksi yang sama, bukan ganda.
Aturan: wajib shift terbuka (409) · non-hutang: `amount_paid ≥ total` · `debt`: wajib pelanggan & `total_debt+total ≤ credit_limit` · `points`: 1 poin = Rp100 (dibulatkan ke atas).
Respons: `transaction_id, trx_number, order_id, order_number, total_amount, amount_paid, change_amount, payment_method, status, paid_at, queue_number`.
- `GET /pos/transactions/{id}` → transaksi + `refunds[]` + `void` (alasan, siapa, kapan)
- `GET /pos/transactions?date=&end_date=&status=completed|cancelled|all&payment_method=&search=` → `data[]` + `summary`

## Void & refund
Dua alat berbeda — pilih sesuai situasi:

| | **Void** | **Refund** |
|---|---|---|
| Untuk | transaksi yang **salah**, seolah tak pernah terjadi | mengembalikan uang atas transaksi yang sah |
| Kapan | pesanan **belum selesai** & **shift transaksi masih terbuka** | kapan pun (juga setelah selesai / shift tutup) |
| Siapa | kasir shift tsb. atau admin | **admin** (default; ubah via `refund_requires_admin`) |
| Jumlah | seluruhnya | penuh atau **sebagian** (cicilan refund) |

- `POST /pos/transactions/{id}/void` `{reason}` (alasan wajib). Efek: transaksi `voided`, pesanan `cancelled`, meja bebas, poin/kunjungan/belanja pelanggan dikembalikan. Mengulang void → `200` (idempoten). Untuk **transaksi hutang**: baris piutang jadi `VOID`; ditolak bila masih ada cicilan aktif (batalkan pelunasannya dulu).
- `POST /pos/transactions/{id}/refund`
```json
{ "client_uuid": "opsional-tapi-disarankan", "amount": 10000, "reason": "menu kurang",
  "restock": false, "shift_id": null }
```
  Aturan: `amount` ≤ sisa yang bisa direfund · **selalu memakai metode bayar asli** (refund tunai mengurangi kas laci; QRIS/transfer tidak menyentuh laci) · `restock: true` hanya bila refund ini membuat total refund **penuh** pada pesanan yang **sudah selesai** (stok dibalik persis seperti yang dulu dikurangi: bundle & bahan baku ikut) · transaksi **poin** hanya bisa direfund penuh · `shift_id` hanya admin · refund penuh pada pesanan belum selesai ikut membatalkan pesanan & membebaskan meja · transaksi hutang tidak bisa direfund.
  Uang keluar dicatat di **shift tempat refund diproses**: shift kasir sendiri; bila admin tak punya shift → shift terbuka terbaru. Tanpa shift terbuka, refund **tunai** ditolak `409`.
  Respons `201`: `{refund:{id,amount,payment_method,shift_id,transaction_status,refunded_amount,remaining_refundable,is_restocked}, cash_out}`.
- `POST /pos/debt-payments/{id}/reverse` `{reason}` — batalkan satu cicilan hutang. Cicilan tidak dihapus, hanya ditandai dibatalkan (satu kali; mengulang → `200` tanpa efek ganda); sisa hutang pulih. Boleh: admin, atau kasir penerima **selama shift penerimaannya masih terbuka**. Uang kembali dari laci shift penerimaan bila masih terbuka (net nol); bila sudah tutup → dari shift orang yang membatalkan (cicilan tunai tanpa shift terbuka → `409`).

Poin pelanggan saat void/refund: poin yang dulu didapat ditarik kembali **proporsional** (kumulatif, tanpa galat pembulatan); poin yang dipakai sebagai potongan dikembalikan pada void/refund penuh.

## Hutang (piutang pelanggan)
- `GET /pos/debts?status=open|paid|all&customer_id=` → urut jatuh tempo terdekat; tiap baris: `amount, remaining_amount, status (OPEN|PARTIAL|PAID|VOID), due_date, is_overdue, customer_name, order_number`
- `GET /pos/debts/{id}` → hutang + `payments[]` (riwayat cicilan; tiap baris memuat `is_reversed`, `reversal_reason`)
- `POST /pos/debts/{id}/payments` — bayar satu hutang
- `POST /pos/customers/{id}/debt-payments` — bayar atas nama pelanggan; dialokasikan **FIFO** (jatuh tempo terdekat dulu)
```json
{ "client_uuid": "uuid-sekali-per-upaya", "amount": 40000, "payment_method": "cash|qris|transfer|debit|credit|card|ewallet", "notes": "opsional" }
```
Respons `201`: `{applied, payments:[{id,debt_id,amount,remaining_after,debt_status}], customer:{id,name,total_debt,credit_limit}, payment_method}`.
Aturan: wajib shift terbuka (409) · `amount > 0` · **tidak boleh melebihi** sisa (per hutang) / total hutang pelanggan (422; tidak ada kembalian/saldo titipan) · metode `debt`/`points` ditolak · `client_uuid` idempoten (replay → respons yang sama; `remaining_after` pada replay = kondisi **saat ini**).
Akuntansi (dasar kas): penjualan hutang tidak masuk pendapatan/kas saat dibuat. Tiap pelunasan masuk ke shift **aktif** penerima — tunai menambah kas laci, non-tunai tidak. Transaksi hutang asli tidak pernah diubah, jadi shift lama yang sudah ditutup tidak bergeser.

## Shift & kas
- `GET /pos/shifts/current` (null bila tak ada) · `GET /pos/shifts/history` · `GET /pos/shifts/{id}/summary`
- `POST /pos/shifts/start` `{opening_cash, shift_type?, notes?}`
- `POST /pos/shifts/close` `{shift_id?, actual_cash, notes?}` — selisih ≠ 0 **wajib** `notes`
- `POST /pos/shifts/cash-movement` `{type: add_cash|drop_cash, amount, description?}`
- Kas seharusnya = kas awal + penjualan tunai + **pelunasan hutang tunai** + tambah − ambil − refund tunai − pengeluaran
- Ringkasan shift memuat `debt_collected: {cash, digital, total}` (**bersih** dari pembatalan; digital tidak masuk laci) dan `debt_reversed`
- **Shift tertutup tidak pernah berubah**: penjualan dicatat **bruto** di shift asalnya (termasuk yang kemudian direfund); refund & pembatalan pelunasan dicatat di shift tempat uang keluar.
- Kas seharusnya = kas awal + penjualan tunai (bruto) + pelunasan hutang tunai (bersih) + tambah − ambil − **refund tunai** − pengeluaran
- `GET/POST /pos/expenses` · `DELETE /pos/expenses/{id}` (hanya saat shift masih terbuka)

## Pelanggan & laporan
- `GET /pos/customers?q=&limit=` · `POST` · `PUT /{id}` · `DELETE /{id}` (admin; ditolak bila masih berhutang) · `GET /{id}/history`
- `GET /pos/reports/daily?date=` (dasar kas): `gross_sales` (bruto pada tanggal bayar asli) · `total_refunds`, `refund_count` (pada tanggal uang keluar) · `total_sales` = bruto − refund hari itu · `by_payment_method` (bersih) · `by_order_type`, `top_products` (bruto) · `debt_collected`, `debt_reversed`, `debt_collected_by_method` · `total_cash_in` = `total_sales` + `debt_collected`
- `GET /pos/dashboard/stats` → termasuk `receivables_total` (total piutang berjalan)

## Admin (role `admin`)
`POST|PUT /admin/categories[/{id}]` · `POST|PUT|DELETE /admin/products[/{id}]` (body boleh memuat `variants[]`, `modifiers[]` → mengganti seluruhnya; DELETE = nonaktifkan) · `POST /admin/products/{id}/stock` `{delta,notes}` · `GET /admin/products/{id}/stock-history` · `POST|PUT /admin/tables` · `GET|POST|PUT /admin/users` · `POST /admin/setup` · `PUT /admin/settings`
`PUT /admin/settings` — hanya kunci whitelist: `apply_tax`, `apply_service_charge_dine_in`, `apply_packaging_fee`, `enable_split_bill`, `refund_requires_admin` (bool) · `tax_rate`, `service_charge_rate` (pecahan 0–1) · `packaging_fee` (int ≥ 0). Kunci asing/tipe salah → `422` untuk **seluruh** permintaan (tak ada penulisan setengah jadi). Berlaku untuk pesanan baru.

## Realtime
`GET /ws?token=` → pesan teks JSON `{"event","data"}`: `order.updated {id}` · `table.updated {id,status,current_order_id?}` · `stats.updated` · `debt.updated {customer_id}` · `resync {missed}` (klien tertinggal → muat ulang penuh).
Server tidak memutar ulang event yang terlewat; klien memanggil `/pos/init` atau refresh ringan setelah tersambung kembali.
