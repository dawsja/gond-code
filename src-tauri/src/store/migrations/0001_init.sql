CREATE TABLE projects (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  path           TEXT NOT NULL UNIQUE,
  default_branch TEXT NOT NULL,
  added_at       TEXT NOT NULL,
  last_opened_at TEXT NOT NULL
);

CREATE TABLE custom_agents (
  id        TEXT PRIMARY KEY,
  name      TEXT NOT NULL,
  command   TEXT NOT NULL,
  args_json TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE threads (
  id               TEXT PRIMARY KEY,
  project_id       TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  agent_id         TEXT NOT NULL,
  title            TEXT NOT NULL,
  slug             TEXT NOT NULL,
  branch           TEXT NOT NULL,
  base_branch      TEXT NOT NULL,
  worktree_path    TEXT NOT NULL,
  status           TEXT NOT NULL,
  agent_session_id TEXT,
  pr_url           TEXT,
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  archived_at      TEXT
);
CREATE INDEX threads_project ON threads(project_id);

CREATE TABLE messages (
  id         TEXT PRIMARY KEY,
  thread_id  TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
  role       TEXT NOT NULL,
  text       TEXT NOT NULL,
  tools_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL
);
CREATE INDEX messages_thread ON messages(thread_id, created_at);

CREATE TABLE settings (
  key        TEXT PRIMARY KEY,
  value_json TEXT NOT NULL
);
