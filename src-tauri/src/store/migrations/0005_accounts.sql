-- Harness accounts. Secrets are never here: `secret_ref` names a Windows
-- Credential Manager entry. `config_dir` is the Gond-owned harness home for sign-ins.
CREATE TABLE accounts (
  id          TEXT PRIMARY KEY,
  harness     TEXT NOT NULL,
  provider    TEXT NOT NULL,
  label       TEXT NOT NULL,
  kind        TEXT NOT NULL,
  secret_ref  TEXT,
  config_dir  TEXT,
  endpoint    TEXT,
  created_at  TEXT NOT NULL
);
CREATE INDEX accounts_harness ON accounts(harness);
