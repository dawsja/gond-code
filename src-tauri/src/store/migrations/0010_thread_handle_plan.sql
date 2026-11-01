-- A thread's @handle, its name everywhere, and the agent's plan as JSON.
-- Older threads keep a NULL handle until they're renamed.
ALTER TABLE threads ADD COLUMN handle TEXT;
ALTER TABLE threads ADD COLUMN plan TEXT;
