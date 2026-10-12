CREATE TABLE refunds (
  id INTEGER PRIMARY KEY,
  uuid TEXT NOT NULL UNIQUE,
  client_uuid TEXT UNIQUE,
  transaction_id INTEGER NOT NULL REFERENCES transactions(id),
  order_id INTEGER REFERENCES orders(id),
  shift_id INTEGER REFERENCES shifts(id),
  user_id INTEGER NOT NULL REFERENCES users(id),
  amount INTEGER NOT NULL CHECK (amount > 0),
  payment_method TEXT NOT NULL,
  reason TEXT NOT NULL,
  is_restocked INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_refunds_trx ON refunds(transaction_id);
CREATE INDEX idx_refunds_shift ON refunds(shift_id);
CREATE INDEX idx_refunds_created ON refunds(created_at);

CREATE TABLE transaction_voids (
  id INTEGER PRIMARY KEY,
  transaction_id INTEGER NOT NULL UNIQUE REFERENCES transactions(id),
  order_id INTEGER REFERENCES orders(id),
  shift_id INTEGER REFERENCES shifts(id),
  user_id INTEGER NOT NULL REFERENCES users(id),
  reason TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE debt_payment_reversals (
  id INTEGER PRIMARY KEY,
  payment_id INTEGER NOT NULL UNIQUE REFERENCES debt_payments(id),
  shift_id INTEGER REFERENCES shifts(id),
  user_id INTEGER NOT NULL REFERENCES users(id),
  reason TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_reversals_shift ON debt_payment_reversals(shift_id);
CREATE INDEX idx_reversals_created ON debt_payment_reversals(created_at);

CREATE TRIGGER transactions_refund_bounds BEFORE UPDATE OF refunded_amount ON transactions
WHEN NEW.refunded_amount < 0 OR NEW.refunded_amount > NEW.total_amount
BEGIN SELECT RAISE(ABORT, 'refunded_amount di luar batas'); END;
