-- The event log: one row per command call, approval, answer
-- or runtime happening. Inputs are redacted per command and capped.
CREATE TABLE events (
  id            TEXT PRIMARY KEY,
  seq           INTEGER NOT NULL UNIQUE,
  at            TEXT NOT NULL,
  kind          TEXT NOT NULL,
  actor_kind    TEXT NOT NULL,
  actor_id      TEXT NOT NULL,
  actor_label   TEXT NOT NULL,
  thread_id     TEXT,
  command       TEXT,
  risk          TEXT,
  decision      TEXT,
  approval_id   TEXT,
  status        TEXT NOT NULL,
  input_json    TEXT,
  result_json   TEXT,
  error_code    TEXT,
  error_message TEXT,
  duration_ms   INTEGER,
  parent_id     TEXT,
  state_version INTEGER,
  epoch         TEXT,
  rule_source   TEXT
);
CREATE INDEX events_thread ON events (thread_id, seq);
CREATE INDEX events_actor ON events (actor_id, seq);
CREATE INDEX events_cmd ON events (command, seq);
