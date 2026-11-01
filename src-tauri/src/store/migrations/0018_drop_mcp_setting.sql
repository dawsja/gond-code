-- Gond's MCP server is always on and serves only agents the app launched; the
-- on/off and scope setting is gone.
DELETE FROM settings WHERE key = 'mcp';
