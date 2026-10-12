#!/usr/bin/env python3
"""
Pemeriksa kontrak DTO Android <-> server Rust.

Tidak butuh Android SDK. Skrip ini:
  1. mem-parse semua `@Serializable data class` di data/Models.kt,
  2. menjalankan server asli (binary hasil `cargo build`) pada DB sementara + data demo,
  3. menjalankan skenario kasir lengkap (bayar, void, refund, hutang, cicilan, pembatalan, shift),
  4. memvalidasi SETIAP respons terhadap DTO yang dipakai Retrofit: nama field (@SerialName), tipe, nullability.

Yang dilaporkan:
  GAGAL   = app Android akan crash/gagal parse respons ini (field wajib hilang, tipe salah, null di field non-null)
  PERIKSA = field ber-default yang TIDAK PERNAH muncul di respons mana pun -> kemungkinan salah ketik @SerialName
            (tidak crash; diam-diam selalu memakai nilai default)

Pakai:  python3 tools/contract_check.py [path/ke/vresto-server] [path/ke/Models.kt]
"""
import json, os, re, subprocess, sys, tempfile, time, urllib.request, urllib.error

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "..", "backend", "target", "debug", "vresto-server")
MODELS = sys.argv[2] if len(sys.argv) > 2 else os.path.join(ROOT, "app/src/main/java/id/vresto/pos/data/Models.kt")
PORT = 8199
if not os.path.exists(BIN) and os.path.exists(BIN + ".exe"):
    BIN = BIN + ".exe"
BASE = f"http://127.0.0.1:{PORT}/api/v1"

def split_top(s):
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch in "(<[": depth += 1
        if ch in ")>]": depth -= 1
        if ch == "," and depth == 0: out.append(cur); cur = ""
        else: cur += ch
    if cur.strip(): out.append(cur)
    return out

def parse_models(path):
    src = open(path, encoding="utf8").read()
    dtos = {}
    for m in re.finditer(r"@Serializable\s+data class (\w+)(?:<\w+>)?\(", src):
        i, depth, start = m.end(), 1, m.end()
        while depth:
            depth += {"(": 1, ")": -1}.get(src[i], 0); i += 1
        body = src[start:i - 1]
        fields = []
        for p in split_top(body):
            p = re.sub(r"/\*.*?\*/|//[^\n]*", "", p, flags=re.S).strip()
            if not p: continue
            sn = re.search(r'@SerialName\("([^"]+)"\)', p)
            fm = re.search(r"(?:val|var)\s+(\w+)\s*:\s*(.+?)(?:\s*=\s*(.+))?$", re.sub(r"@SerialName\([^)]*\)\s*", "", p), re.S)
            if not fm: continue
            name, typ, default = fm.group(1), fm.group(2).strip(), fm.group(3)
            fields.append(dict(key=sn.group(1) if sn else name, prop=name, type=typ, has_default=default is not None))
        dtos[m.group(1)] = fields
    return dtos

DTOS = parse_models(MODELS)
SEEN = {}
ERRORS = []

def check(val, typ, path):
    typ = typ.strip()
    nullable = typ.endswith("?")
    base = typ[:-1] if nullable else typ
    if val is None:
        if not nullable: ERRORS.append(f"{path}: null untuk tipe non-null {base}")
        return
    if base in ("Long", "Int"):
        if isinstance(val, bool) or not isinstance(val, int): ERRORS.append(f"{path}: {base} tapi server kirim {type(val).__name__} ({val!r})")
    elif base == "Double":
        if isinstance(val, bool) or not isinstance(val, (int, float)): ERRORS.append(f"{path}: Double tapi server kirim {val!r}")
    elif base == "Boolean":
        if not isinstance(val, bool): ERRORS.append(f"{path}: Boolean tapi server kirim {type(val).__name__} ({val!r})  <- biasanya kolom int 0/1 tanpa awalan is_")
    elif base == "String":
        if not isinstance(val, str): ERRORS.append(f"{path}: String tapi server kirim {type(val).__name__} ({val!r})")
    elif base.startswith("List<"):
        if not isinstance(val, list): ERRORS.append(f"{path}: List tapi server kirim {type(val).__name__}")
        else:
            for i, v in enumerate(val): check(v, base[5:-1], f"{path}[{i}]")
    elif base.endswith("JsonElement"):
        pass
    elif base in DTOS:
        check_dto(val, base, path)
    else:
        ERRORS.append(f"{path}: tipe tak dikenal oleh pemeriksa: {base}")

def check_dto(obj, name, path):
    if not isinstance(obj, dict):
        ERRORS.append(f"{path}: {name} diharapkan objek, server kirim {type(obj).__name__}"); return
    for f in DTOS[name]:
        k = f["key"]
        if k in obj:
            SEEN[(name, k)] = True

            if obj[k] is None and not f["type"].endswith("?") and f["has_default"]: continue
            check(obj[k], f["type"], f"{path}.{k}")
        else:
            SEEN.setdefault((name, k), False)
            if not f["has_default"] and not f["type"].endswith("?"):
                ERRORS.append(f"{path}: field WAJIB `{k}` ({name}.{f['prop']}) tidak ada di respons server")

def call(method, path, body=None, token=None, expect=None):
    req = urllib.request.Request(BASE + path, method=method, data=json.dumps(body).encode() if body is not None else None,
                                 headers={"content-type": "application/json", **({"authorization": f"Bearer {token}"} if token else {})})
    try:
        with urllib.request.urlopen(req) as r: code, raw = r.status, r.read()
    except urllib.error.HTTPError as e: code, raw = e.code, e.read()
    j = json.loads(raw) if raw else None
    if expect and code != expect: sys.exit(f"SKENARIO RUSAK: {method} {path} -> {code} (harap {expect}): {j}")
    return j

COUNT = 0
def v(resp, dto, label, envelope=True):
    """Validasi respons terhadap DTO. envelope=True -> {success,data}; pakai `data`."""
    global COUNT; COUNT += 1
    check_dto(resp, dto, label) if not envelope else check(resp.get("data"), dto, label)

def main():
    if not os.path.exists(BIN): sys.exit(f"Binary tidak ada: {BIN}\nJalankan dulu: cd backend && cargo build")
    tmp = tempfile.mkdtemp()
    env = {**os.environ, "VRESTO_DB": f"{tmp}/c.sqlite", "VRESTO_BIND": f"127.0.0.1:{PORT}", "VRESTO_JWT_SECRET": "x" * 40}
    srv = subprocess.Popen([BIN, "--seed-demo"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(50):
            try: urllib.request.urlopen(f"http://127.0.0.1:{PORT}/api/v1/health"); break
            except Exception: time.sleep(0.1)
        run()
    finally:
        srv.terminate()

def run():
    L = call("POST", "/auth/login", dict(username="kasir", password="warmangji.ksr"), expect=200); v(L, "LoginData", "login")
    K = L["data"]["token"]
    A = call("POST", "/auth/login", dict(username="admin", password="warmangji.adm"), expect=200)["data"]["token"]
    P = "/pos"
    v(call("POST", P + "/shifts/start", dict(opening_cash=100000, notes="pagi"), K, 201), "ShiftSummaryDto", "shifts/start")
    v(call("POST", P + "/shifts/cash-movement", dict(type="add_cash", amount=5000, description="receh"), K, 200), "ShiftSummaryDto", "cash-movement")
    v(call("POST", P + "/expenses", dict(category="Gas", description="tabung", amount=15000), K, 201), "ExpenseDto", "expenses POST")
    v(call("GET", P + "/expenses", token=K, expect=200), "List<ExpenseDto>", "expenses GET")

    pay = lambda order, m, paid, **kw: call("POST", P + "/transactions", dict(order=order, payment_method=m, amount_paid=paid, **kw), K, 201)
    a = pay(dict(items=[dict(product_id=1, qty=1, modifiers=[dict(id=1, quantity=2)]), dict(product_id=6, variant_id=1, qty=2)],
                 order_type="dine_in", table_id=1, discount_type="percent", discount_value=5), "cash", 100000, customer_id=1)
    v(a, "PayResultDto", "transactions POST (dine-in)")
    b = pay(dict(items=[dict(product_id=9, qty=1)], order_type="take_away"), "qris", 32800); v(b, "PayResultDto", "transactions POST (take-away, antrean)")
    assert b["data"]["queue_number"] == 1
    oa, ob = a["data"]["order_id"], b["data"]["order_id"]; ta = a["data"]["transaction_id"]

    c = call("POST", P + "/orders", dict(items=[dict(product_id=4, qty=2)], order_type="dine_in", table_id=2, notes="tanpa es"), K, 201); v(c, "OrderDto", "orders POST")
    oc = c["data"]["id"]
    v(call("PUT", P + f"/orders/{oc}", dict(items=[dict(product_id=4, qty=3)]), K, 200), "OrderDto", "orders PUT")
    v(call("GET", P + f"/orders/{oc}", token=K, expect=200), "OrderDto", "orders/{id}")
    v(call("GET", P + "/orders?status=active", token=K, expect=200), "List<OrderDto>", "orders?active")
    v(call("PATCH", P + f"/orders/{ob}/status", dict(status="ready"), K, 200), "OrderDto", "orders/status")
    v(call("POST", P + f"/orders/{oa}/complete", token=K, expect=200), "OrderDto", "orders/complete")
    v(call("POST", P + f"/orders/{oc}/cancel", token=K, expect=200), "OrderDto", "orders/cancel")

    d = pay(dict(items=[dict(product_id=4, qty=2)], order_type="dine_in", table_id=3), "cash", 20000)
    td = d["data"]["transaction_id"]
    v(call("POST", P + f"/transactions/{td}/void", dict(reason="salah menu"), K, 200), "TransactionDto", "transactions/void")

    r1 = call("POST", P + f"/transactions/{ta}/refund", dict(client_uuid="c1", amount=10000, reason="menu kurang"), A, 201); v(r1, "RefundResultDto", "refund sebagian")
    rem = r1["data"]["refund"]["remaining_refundable"]
    v(call("POST", P + f"/transactions/{ta}/refund", dict(client_uuid="c2", amount=rem, reason="batal", restock=True), A, 201), "RefundResultDto", "refund penuh+restock")
    v(call("GET", P + f"/transactions/{ta}", token=K, expect=200), "TransactionDto", "transactions/{id} (refunds)")
    v(call("GET", P + f"/transactions/{td}", token=K, expect=200), "TransactionDto", "transactions/{id} (void)")
    lst = call("GET", P + "/transactions?status=all", token=K, expect=200); v(lst, "TransactionListResponse", "transactions list", envelope=False)

    for o in (dict(items=[dict(product_id=3, qty=1)], order_type="dine_in", table_id=1), dict(items=[dict(product_id=1, qty=1)], order_type="take_away")):
        pay(o, "hutang", 0, customer_id=1)
    v(call("GET", P + "/debts?status=open", token=K, expect=200), "List<DebtDto>", "debts list")
    v(call("GET", P + "/debts/1", token=K, expect=200), "DebtDto", "debts/{id}")
    rp = call("POST", P + "/debts/1/payments", dict(client_uuid="d1", amount=10000, payment_method="cash", notes="cicil"), K, 201); v(rp, "RepayResultDto", "debts/{id}/payments")
    v(call("POST", P + "/customers/1/debt-payments", dict(client_uuid="d2", amount=20000, payment_method="qris"), K, 201), "RepayResultDto", "customers/{id}/debt-payments")
    pid = rp["data"]["payments"][0]["id"]
    v(call("POST", P + f"/debt-payments/{pid}/reverse", dict(reason="salah nominal"), K, 200), "ReverseResultDto", "debt-payments/reverse")
    v(call("GET", P + "/debts/1", token=K, expect=200), "DebtDto", "debts/{id} (cicilan dibatalkan)")
    v(call("GET", P + "/debts?status=all", token=K, expect=200), "List<DebtDto>", "debts?all")

    v(call("GET", P + "/init", token=K, expect=200), "InitDto", "init")
    v(call("GET", P + "/tables", token=K, expect=200), "List<TableDto>", "tables")
    v(call("GET", P + "/products", token=K, expect=200), "List<ProductDto>", "products")
    v(call("GET", P + "/categories", token=K, expect=200), "List<CategoryDto>", "categories")
    v(call("GET", P + "/payment-methods", token=K, expect=200), "List<PaymentMethodDto>", "payment-methods")
    v(call("GET", P + "/billing-settings", token=K, expect=200), "BillingDto", "billing-settings")
    v(call("GET", P + "/customers", token=K, expect=200), "List<CustomerDto>", "customers")
    v(call("GET", P + "/dashboard/stats", token=K, expect=200), "StatsDto", "stats")
    v(call("GET", P + "/shifts/current", token=K, expect=200), "ShiftSummaryDto", "shifts/current")
    v(call("POST", P + "/shifts/close", dict(actual_cash=0, notes="uji kontrak"), K, 200), "ShiftSummaryDto", "shifts/close")

    nul = call("GET", P + "/shifts/current", token=K, expect=200); assert nul["data"] is None, nul

    print(f"\n{COUNT} respons divalidasi terhadap {len(DTOS)} DTO.")
    never = sorted(f"{d}.{k}" for (d, k), seen in SEEN.items() if not seen and any(f["key"] == k and f["has_default"] for f in DTOS[d]))
    if ERRORS:
        print(f"\nGAGAL ({len(ERRORS)}) — app Android akan gagal memproses ini:"); [print("  [FAIL]", e) for e in sorted(set(ERRORS))]
    else:
        print("GAGAL: 0  [OK] semua respons cocok dengan DTO")
    if never:
        print(f"\nPERIKSA ({len(never)}) — field ber-default yang tak pernah muncul di respons mana pun (salah ketik @SerialName? atau memang opsional):")
        for n in never: print("  ?", n)
    sys.exit(1 if ERRORS else 0)

if __name__ == "__main__":
    main()
