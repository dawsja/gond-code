-- Each chat's permission mode: ask | auto | allow, passed to the
-- agent as its own native mode. NULL = the default in Settings.
ALTER TABLE threads ADD COLUMN permission_mode TEXT;
