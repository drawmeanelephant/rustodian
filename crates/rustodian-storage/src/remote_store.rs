use crate::store::SqliteStore;
use rusqlite::params;
use rustodian_core::error::CoreError;
use rustodian_core::traits::RemoteProjectStore;
use rustodian_types::RemoteProject;

impl RemoteProjectStore for SqliteStore {
    fn save_remote_project(&self, project: &RemoteProject) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        let patterns_json = serde_json::to_string(&project.preserve_patterns)
            .map_err(|e| CoreError::Storage(format!("failed to serialize patterns: {e}")))?;
        conn.execute(
            "INSERT INTO remote_projects (repo_slug, preserve_patterns)
             VALUES (?1, ?2)
             ON CONFLICT(repo_slug) DO UPDATE SET preserve_patterns = excluded.preserve_patterns",
            params![project.repo_slug, patterns_json],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }
    fn list_remote_projects(&self) -> Result<Vec<RemoteProject>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT repo_slug, preserve_patterns FROM remote_projects")
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let repo_slug: String = row.get(0)?;
                let patterns_json: String = row.get(1)?;
                let preserve_patterns = serde_json::from_str(&patterns_json).unwrap_or_default();
                Ok(RemoteProject {
                    repo_slug,
                    preserve_patterns,
                })
            })
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let mut projects = Vec::new();
        for row in rows {
            projects.push(row.map_err(|e| CoreError::Storage(e.to_string()))?);
        }
        Ok(projects)
    }
    fn delete_remote_project(&self, repo_slug: &str) -> Result<bool, CoreError> {
        let conn = self.conn.lock().unwrap();
        let changes = conn
            .execute(
                "DELETE FROM remote_projects WHERE repo_slug = ?1",
                params![repo_slug],
            )
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(changes > 0)
    }
}
