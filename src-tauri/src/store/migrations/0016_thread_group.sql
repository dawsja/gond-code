-- A crew started together on one task (thread.createMany): the group it
-- belongs to, the task it works on, and its role (builder | reviewer |
-- researcher | judge; NULL means builder). NULL for threads started on their own.
ALTER TABLE threads ADD COLUMN group_id TEXT;
ALTER TABLE threads ADD COLUMN task_id TEXT;
ALTER TABLE threads ADD COLUMN role TEXT;
