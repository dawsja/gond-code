-- Panes are shells and agents run on their own sign-ins: nothing reads these any more.
-- Children before parents, so no foreign key holds a drop up.
DROP TABLE IF EXISTS loops;
DROP TABLE IF EXISTS tasks;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS receipts;
DROP TABLE IF EXISTS threads;
DROP TABLE IF EXISTS accounts;
DROP TABLE IF EXISTS events;
DROP TABLE IF EXISTS mcp_secrets;
DROP TABLE IF EXISTS mcp_servers;
