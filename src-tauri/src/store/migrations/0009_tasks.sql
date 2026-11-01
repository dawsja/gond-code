-- The board's tasks. With thread_id set, the status shown is
-- derived from the thread; the stored one is for unassigned tasks.
CREATE TABLE tasks (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  title       TEXT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'queued',
  ord         REAL NOT NULL,
  thread_id   TEXT REFERENCES threads(id) ON DELETE SET NULL,
  blocked_by  TEXT NOT NULL DEFAULT '[]',
  created_by  TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
CREATE INDEX tasks_by_project ON tasks (project_id, ord);
