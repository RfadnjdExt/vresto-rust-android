CREATE TABLE debt_payments (
  id INTEGER PRIMARY KEY,
  uuid TEXT NOT NULL UNIQUE,
  client_uuid TEXT,
  debt_id INTEGER NOT NULL REFERENCES debts(id),
  customer_id INTEGER NOT NULL REFERENCES customers(id),
  shift_id INTEGER REFERENCES shifts(id),
  user_id INTEGER NOT NULL REFERENCES users(id),
  amount INTEGER NOT NULL CHECK (amount > 0),
  payment_method TEXT NOT NULL,
  notes TEXT,
  created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_debt_payments_idem ON debt_payments(client_uuid, debt_id) WHERE client_uuid IS NOT NULL;
CREATE INDEX idx_debt_payments_debt ON debt_payments(debt_id);
CREATE INDEX idx_debt_payments_shift ON debt_payments(shift_id);
CREATE INDEX idx_debt_payments_created ON debt_payments(created_at);
CREATE INDEX idx_debts_customer_open ON debts(customer_id) WHERE remaining_amount > 0;

CREATE TRIGGER debts_remaining_bounds BEFORE UPDATE OF remaining_amount ON debts
WHEN NEW.remaining_amount < 0 OR NEW.remaining_amount > NEW.amount
BEGIN SELECT RAISE(ABORT, 'remaining_amount di luar batas'); END;
