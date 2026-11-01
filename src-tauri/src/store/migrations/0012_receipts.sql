-- Proof receipts: what the app ran and saw, immutable.
-- `data` is the whole ProofReceipt as JSON; the columns are for lookups.
CREATE TABLE receipts (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  thread_id   TEXT NOT NULL,
  task_id     TEXT,
  run_id      TEXT NOT NULL,
  at          TEXT NOT NULL,
  data        TEXT NOT NULL
);
CREATE INDEX receipts_by_thread ON receipts (thread_id, at);
CREATE INDEX receipts_by_run ON receipts (run_id);
