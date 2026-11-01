-- The model and thinking level a thread's agent runs with; NULL = the harness default.
ALTER TABLE threads ADD COLUMN model TEXT;
ALTER TABLE threads ADD COLUMN thinking TEXT;
