ALTER TABLE projects ADD COLUMN removed_at TEXT;

CREATE UNIQUE INDEX uq_projects_root_path_nocase
    ON projects(root_path COLLATE NOCASE);

CREATE INDEX idx_projects_registration
    ON projects(removed_at, display_name COLLATE NOCASE, root_path);

CREATE TABLE onboarding_state (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    completed_at TEXT NOT NULL
);
