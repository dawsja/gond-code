-- The thread whose agent created this one: spawn depth and descendant
-- scope follow it. NULL for the human's threads.
ALTER TABLE threads ADD COLUMN parent_thread_id TEXT;
