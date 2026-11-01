-- The account a thread's agent runs as; NULL = the harness's own sign-in.
ALTER TABLE threads ADD COLUMN account_id TEXT;
