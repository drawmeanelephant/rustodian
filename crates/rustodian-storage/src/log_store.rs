//! Persistence for command execution logs.

use chrono::Utc;
use rusqlite::{OptionalExtension, params};

use crate::store::SqliteStore;
use rustodian_core::CoreError;

pub use rustodian_types::ProjectLog;

impl SqliteStore {
    /// Persist a command execution log.
    pub fn save_log(&self, log: &ProjectLog) -> Result<(), CoreError> {
        let conn = self.get_conn()?;

        conn.execute(
            "INSERT INTO project_logs (id, project_id, command_name, exit_code, log_text, run_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                exit_code=excluded.exit_code,
                log_text=excluded.log_text",
            params![
                log.id,
                log.project_id,
                log.command_name,
                log.exit_code,
                log.log_text,
                log.run_at.to_rfc3339(),
            ],
        )
        .map_err(|e| CoreError::Storage(format!("failed to save log: {e}")))?;

        Ok(())
    }

    /// List execution logs for a project, ordered by most recent first.
    pub fn list_logs(&self, project_id: &str, limit: usize) -> Result<Vec<ProjectLog>, CoreError> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, command_name, exit_code, log_text, run_at
                 FROM project_logs
                 WHERE project_id = ?1
                 ORDER BY run_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let rows = stmt
            .query_map(params![project_id, limit], |row| {
                let id: String = row.get(0)?;
                let project_id: String = row.get(1)?;
                let command_name: String = row.get(2)?;
                let exit_code: Option<i32> = row.get(3)?;
                let log_text: String = row.get(4)?;
                let run_at_str: String = row.get(5)?;
                Ok((
                    id,
                    project_id,
                    command_name,
                    exit_code,
                    log_text,
                    run_at_str,
                ))
            })
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        let mut logs = Vec::new();
        for row in rows {
            let (id, project_id, command_name, exit_code, log_text, run_at_str) =
                row.map_err(|e| CoreError::Storage(format!("row error: {e}")))?;
            let run_at = chrono::DateTime::parse_from_rfc3339(&run_at_str)
                .map_err(|e| CoreError::Storage(format!("invalid timestamp '{run_at_str}': {e}")))?
                .with_timezone(&Utc);
            logs.push(ProjectLog {
                id,
                project_id,
                command_name,
                exit_code,
                log_text,
                run_at,
            });
        }
        Ok(logs)
    }

    /// Get a specific log entry by ID.
    pub fn get_log(&self, id: &str) -> Result<Option<ProjectLog>, CoreError> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, command_name, exit_code, log_text, run_at
                 FROM project_logs
                 WHERE id = ?1",
            )
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let row = stmt
            .query_row(params![id], |row| {
                let id: String = row.get(0)?;
                let project_id: String = row.get(1)?;
                let command_name: String = row.get(2)?;
                let exit_code: Option<i32> = row.get(3)?;
                let log_text: String = row.get(4)?;
                let run_at_str: String = row.get(5)?;
                Ok((
                    id,
                    project_id,
                    command_name,
                    exit_code,
                    log_text,
                    run_at_str,
                ))
            })
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        match row {
            Some((id, project_id, command_name, exit_code, log_text, run_at_str)) => {
                let run_at = chrono::DateTime::parse_from_rfc3339(&run_at_str)
                    .map_err(|e| {
                        CoreError::Storage(format!("invalid timestamp '{run_at_str}': {e}"))
                    })?
                    .with_timezone(&Utc);
                Ok(Some(ProjectLog {
                    id,
                    project_id,
                    command_name,
                    exit_code,
                    log_text,
                    run_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get the most recent log entry for a project.
    pub fn get_latest_log(&self, project_id: &str) -> Result<Option<ProjectLog>, CoreError> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, command_name, exit_code, log_text, run_at
                 FROM project_logs
                 WHERE project_id = ?1
                 ORDER BY run_at DESC
                 LIMIT 1",
            )
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let row = stmt
            .query_row(params![project_id], |row| {
                let id: String = row.get(0)?;
                let project_id: String = row.get(1)?;
                let command_name: String = row.get(2)?;
                let exit_code: Option<i32> = row.get(3)?;
                let log_text: String = row.get(4)?;
                let run_at_str: String = row.get(5)?;
                Ok((
                    id,
                    project_id,
                    command_name,
                    exit_code,
                    log_text,
                    run_at_str,
                ))
            })
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        match row {
            Some((id, project_id, command_name, exit_code, log_text, run_at_str)) => {
                let run_at = chrono::DateTime::parse_from_rfc3339(&run_at_str)
                    .map_err(|e| {
                        CoreError::Storage(format!("invalid timestamp '{run_at_str}': {e}"))
                    })?
                    .with_timezone(&Utc);
                Ok(Some(ProjectLog {
                    id,
                    project_id,
                    command_name,
                    exit_code,
                    log_text,
                    run_at,
                }))
            }
            None => Ok(None),
        }
    }
}
