CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  username TEXT NOT NULL UNIQUE,
  email TEXT UNIQUE,
  password_hash TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('admin','cashier')),
  pin_code TEXT,
  phone TEXT,
  is_active INTEGER NOT NULL DEFAULT 1,
  last_login_at TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE categories (
  id INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES categories(id),
  name TEXT NOT NULL,
  icon TEXT, color TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  is_active INTEGER NOT NULL DEFAULT 1,
  show_in_pos INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE products (
  id INTEGER PRIMARY KEY,
  uuid TEXT NOT NULL UNIQUE,
  category_id INTEGER REFERENCES categories(id),
  name TEXT NOT NULL,
  sku TEXT UNIQUE, barcode TEXT,
  type TEXT NOT NULL DEFAULT 'single' CHECK (type IN ('single','bundle')),
  price INTEGER NOT NULL CHECK (price >= 0),
  cost_price INTEGER NOT NULL DEFAULT 0,
  discount_price INTEGER, discount_start TEXT, discount_end TEXT,
  is_stock_managed INTEGER NOT NULL DEFAULT 0,
  stock_quantity INTEGER NOT NULL DEFAULT 0,
  min_stock INTEGER NOT NULL DEFAULT 5,
  image TEXT, station TEXT,
  is_active INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_products_category ON products(category_id);
CREATE INDEX idx_products_barcode ON products(barcode);

CREATE TABLE product_variants (
  id INTEGER PRIMARY KEY,
  product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  price_adjustment INTEGER NOT NULL DEFAULT 0,
  is_active INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE product_modifiers (
  id INTEGER PRIMARY KEY,
  product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  type TEXT,
  price INTEGER NOT NULL DEFAULT 0,
  max_quantity INTEGER NOT NULL DEFAULT 1,
  is_required INTEGER NOT NULL DEFAULT 0,
  is_active INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE product_bundles (
  id INTEGER PRIMARY KEY,
  parent_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  product_id INTEGER NOT NULL REFERENCES products(id),
  quantity INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE ingredients (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  sku TEXT, unit TEXT,
  current_stock REAL NOT NULL DEFAULT 0,
  alert_limit REAL NOT NULL DEFAULT 0,
  use_stock INTEGER NOT NULL DEFAULT 1,
  cost_price INTEGER NOT NULL DEFAULT 0,
  is_active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE product_ingredients (
  id INTEGER PRIMARY KEY,
  product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  ingredient_id INTEGER NOT NULL REFERENCES ingredients(id),
  quantity REAL NOT NULL
);

CREATE TABLE inventory_transactions (
  id INTEGER PRIMARY KEY,
  ingredient_id INTEGER REFERENCES ingredients(id),
  product_id INTEGER REFERENCES products(id),
  order_id INTEGER,
  user_id INTEGER,
  type TEXT NOT NULL,
  quantity REAL NOT NULL,
  stock_after REAL,
  notes TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE dining_tables (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  code TEXT UNIQUE,
  section TEXT, floor TEXT,
  capacity INTEGER NOT NULL DEFAULT 4,
  status TEXT NOT NULL DEFAULT 'available' CHECK (status IN ('available','occupied','reserved')),
  current_order_id INTEGER,
  is_active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE payment_methods (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  slug TEXT NOT NULL UNIQUE,
  icon TEXT, color TEXT, description TEXT,
  is_active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE customers (
  id INTEGER PRIMARY KEY,
  uuid TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  phone TEXT, email TEXT, address TEXT,
  points INTEGER NOT NULL DEFAULT 0,
  total_visits INTEGER NOT NULL DEFAULT 0,
  total_spent INTEGER NOT NULL DEFAULT 0,
  membership_level TEXT NOT NULL DEFAULT 'regular',
  credit_limit INTEGER NOT NULL DEFAULT 0,
  total_debt INTEGER NOT NULL DEFAULT 0,
  default_due_days INTEGER NOT NULL DEFAULT 7,
  is_active INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL
);

CREATE TABLE shifts (
  id INTEGER PRIMARY KEY,
  uuid TEXT NOT NULL UNIQUE,
  shift_number TEXT NOT NULL,
  user_id INTEGER NOT NULL REFERENCES users(id),
  opened_by INTEGER, closed_by INTEGER,
  shift_type TEXT NOT NULL DEFAULT 'morning',
  status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open','closed','suspended')),
  opening_cash INTEGER NOT NULL DEFAULT 0,
  closing_cash INTEGER, expected_cash INTEGER, cash_difference INTEGER,
  total_cash_additions INTEGER NOT NULL DEFAULT 0,
  total_cash_drops INTEGER NOT NULL DEFAULT 0,
  total_expenses INTEGER NOT NULL DEFAULT 0,
  total_sales INTEGER, cash_sales INTEGER, digital_sales INTEGER, total_orders INTEGER,
  note_open TEXT, note_close TEXT,
  opened_at TEXT NOT NULL, closed_at TEXT
);

CREATE UNIQUE INDEX idx_one_open_shift_per_user ON shifts(user_id) WHERE status = 'open';

CREATE TABLE shift_cash_logs (
  id INTEGER PRIMARY KEY,
  shift_id INTEGER NOT NULL REFERENCES shifts(id),
  user_id INTEGER NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('add_cash','drop_cash')),
  amount INTEGER NOT NULL CHECK (amount > 0),
  description TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE orders (
  id INTEGER PRIMARY KEY,
  uuid TEXT NOT NULL UNIQUE,
  order_number TEXT NOT NULL UNIQUE,
  user_id INTEGER NOT NULL REFERENCES users(id),
  shift_id INTEGER REFERENCES shifts(id),
  dining_table_id INTEGER REFERENCES dining_tables(id),
  customer_id INTEGER REFERENCES customers(id),
  customer_name TEXT,
  guest_count INTEGER NOT NULL DEFAULT 1,
  order_type TEXT NOT NULL DEFAULT 'dine_in',
  queue_number INTEGER,
  subtotal INTEGER NOT NULL,
  discount_type TEXT, discount_value REAL,
  discount_amount INTEGER NOT NULL DEFAULT 0,
  tax_amount INTEGER NOT NULL DEFAULT 0,
  tax_rate_bps INTEGER NOT NULL DEFAULT 0,
  service_charge INTEGER NOT NULL DEFAULT 0,
  service_charge_bps INTEGER NOT NULL DEFAULT 0,
  packaging_fee INTEGER NOT NULL DEFAULT 0,
  shipping_fee INTEGER NOT NULL DEFAULT 0,
  points_used INTEGER NOT NULL DEFAULT 0,
  total_amount INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('draft','pending','confirmed','preparing','ready','served','completed','cancelled')),
  payment_status TEXT NOT NULL DEFAULT 'unpaid' CHECK (payment_status IN ('unpaid','partial','paid','refunded')),
  notes TEXT, kitchen_notes TEXT,
  completed_at TEXT, completed_by INTEGER,
  cancelled_at TEXT, cancel_reason TEXT,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_created ON orders(created_at);

CREATE TABLE order_items (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  product_id INTEGER NOT NULL REFERENCES products(id),
  product_variant_id INTEGER,
  product_name TEXT NOT NULL,
  product_sku TEXT,
  quantity INTEGER NOT NULL CHECK (quantity > 0),
  unit_price INTEGER NOT NULL,
  cost_price INTEGER NOT NULL DEFAULT 0,
  modifiers_total INTEGER NOT NULL DEFAULT 0,
  total_price INTEGER NOT NULL,
  note TEXT,
  kitchen_status TEXT NOT NULL DEFAULT 'pending',
  is_takeaway INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_order_items_order ON order_items(order_id);

CREATE TABLE order_item_modifiers (
  id INTEGER PRIMARY KEY,
  order_item_id INTEGER NOT NULL REFERENCES order_items(id) ON DELETE CASCADE,
  product_modifier_id INTEGER,
  modifier_name TEXT NOT NULL,
  quantity INTEGER NOT NULL DEFAULT 1,
  unit_price INTEGER NOT NULL,
  total_price INTEGER NOT NULL
);

CREATE TABLE transactions (
  id INTEGER PRIMARY KEY,
  uuid TEXT NOT NULL UNIQUE,
  client_uuid TEXT UNIQUE,
  trx_number TEXT NOT NULL UNIQUE,
  order_id INTEGER REFERENCES orders(id),
  user_id INTEGER NOT NULL REFERENCES users(id),
  shift_id INTEGER REFERENCES shifts(id),
  customer_id INTEGER REFERENCES customers(id),
  payment_method TEXT NOT NULL,
  subtotal INTEGER NOT NULL,
  tax_amount INTEGER NOT NULL DEFAULT 0,
  discount_amount INTEGER NOT NULL DEFAULT 0,
  service_charge INTEGER NOT NULL DEFAULT 0,
  total_amount INTEGER NOT NULL,
  amount_paid INTEGER NOT NULL,
  change_amount INTEGER NOT NULL DEFAULT 0,
  refunded_amount INTEGER NOT NULL DEFAULT 0,
  remaining_refundable INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL CHECK (status IN ('pending','confirmed','refunded','partial_refund','voided','unpaid','failed','cancelled')),
  notes TEXT,
  paid_at TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_trx_shift ON transactions(shift_id);
CREATE INDEX idx_trx_created ON transactions(created_at);

CREATE TABLE debts (
  id INTEGER PRIMARY KEY,
  customer_id INTEGER NOT NULL REFERENCES customers(id),
  order_id INTEGER, transaction_id INTEGER,
  amount INTEGER NOT NULL,
  remaining_amount INTEGER NOT NULL,
  due_date TEXT,
  status TEXT NOT NULL DEFAULT 'OPEN',
  notes TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE expenses (
  id INTEGER PRIMARY KEY,
  shift_id INTEGER REFERENCES shifts(id),
  category TEXT NOT NULL,
  description TEXT,
  amount INTEGER NOT NULL CHECK (amount > 0),
  expense_date TEXT NOT NULL,
  created_by INTEGER NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE queue_counters (date TEXT PRIMARY KEY, last_number INTEGER NOT NULL DEFAULT 0);

CREATE TABLE audit_log (
  id INTEGER PRIMARY KEY,
  user_id INTEGER, event TEXT NOT NULL, entity TEXT NOT NULL, entity_id INTEGER,
  data TEXT, created_at TEXT NOT NULL
);
