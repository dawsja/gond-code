-- How a custom agent takes its prompt: 'arg' (last argument) or 'stdin'.
ALTER TABLE custom_agents ADD COLUMN prompt_via TEXT NOT NULL DEFAULT 'arg';
