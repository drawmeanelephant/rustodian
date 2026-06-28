CREATE TABLE IF NOT EXISTS project_logs (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    command_name TEXT NOT NULL,
    exit_code   INTEGER,
    log_text    TEXT NOT NULL DEFAULT '',
    run_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_logs_project ON project_logs(project_id);
CREATE INDEX IF NOT EXISTS idx_logs_run_at  ON project_logs(run_at DESC);
