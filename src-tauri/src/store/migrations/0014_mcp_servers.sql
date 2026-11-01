-- User MCP servers. `data` is the McpServer as JSON (secrets
-- only as `{ "secret": NAME }` references); `name` is unique, the key agents see.
CREATE TABLE mcp_servers (
  id    TEXT PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE,
  data  TEXT NOT NULL
);
-- Which named secrets are set. The values live in Credential Manager
-- (`GondCode/mcp/<name>`), never here.
CREATE TABLE mcp_secrets (
  name  TEXT PRIMARY KEY,
  at    TEXT NOT NULL
);
