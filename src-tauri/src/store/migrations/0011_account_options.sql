-- API-account options:
-- auth style, extra headers, org/project, Azure API version, cloud switch. JSON, no secrets.
ALTER TABLE accounts ADD COLUMN options TEXT NOT NULL DEFAULT '{}';
