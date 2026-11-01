-- The Gond Loop: one loop per task, owned by its thread.
-- `data` is the whole Loop as JSON; the columns are for lookups.
CREATE TABLE loops (
  task_id     TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
  thread_id   TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  data        TEXT NOT NULL
);
CREATE INDEX loops_by_thread ON loops (thread_id, updated_at);
