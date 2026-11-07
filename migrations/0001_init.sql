CREATE TABLE orders (
  id uuid PRIMARY KEY,
  ticket_no smallint NOT NULL,
  status text NOT NULL CHECK (status IN ('active','cancelled')),
  name text NOT NULL,
  student_id text NOT NULL,
  phone text NOT NULL,
  slip_path text NOT NULL,
  slip_ref text NOT NULL UNIQUE,
  created_at timestamptz NOT NULL DEFAULT now(),
  cancelled_at timestamptz,
  cancel_reason text,
  picked_up_at timestamptz
);
CREATE UNIQUE INDEX orders_active_ticket ON orders (ticket_no) WHERE status = 'active';
CREATE UNIQUE INDEX orders_active_student ON orders (student_id) WHERE status = 'active';

CREATE TABLE settings (key text PRIMARY KEY, value text NOT NULL);
INSERT INTO settings VALUES ('sales_open', 'false'), ('presale_closed_forever', 'false');

CREATE TABLE rejected_attempts (
  id uuid PRIMARY KEY,
  reason text NOT NULL,
  name text,
  student_id text,
  phone text,
  slip_path text,
  slip_ref text,
  created_at timestamptz NOT NULL DEFAULT now()
);
