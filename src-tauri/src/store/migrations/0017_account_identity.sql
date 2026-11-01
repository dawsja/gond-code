-- who an account signs in as (to merge duplicates and label
-- rows), the default account per harness, and an optional spending cap.
ALTER TABLE accounts ADD COLUMN identity TEXT;
ALTER TABLE accounts ADD COLUMN is_default INTEGER NOT NULL DEFAULT 0;
ALTER TABLE accounts ADD COLUMN cap_usd REAL;
