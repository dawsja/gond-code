-- Network access is unrestricted: the egress allowlist is gone.
-- Its per-thread and per-project host lists go, and so does the policy's `egress`
-- section (the policy would load without it anyway).
DELETE FROM settings WHERE key IN ('egress.threads', 'egress.projects');
UPDATE settings SET value_json = json_remove(value_json, '$.egress')
  WHERE key = 'policy' AND json_valid(value_json);
