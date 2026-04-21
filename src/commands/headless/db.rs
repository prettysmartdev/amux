use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Returns the headless storage root directory.
/// Respects `AMUX_HEADLESS_ROOT` env var for testing; defaults to `~/.amux/headless/`.
pub fn headless_root() -> Result<PathBuf> {
    if let Ok(root) = std::env::var("AMUX_HEADLESS_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".amux").join("headless"))
}

/// Returns the path to the SQLite database file.
pub fn db_path() -> Result<PathBuf> {
    Ok(headless_root()?.join("amux.db"))
}

/// Open (or create) the SQLite database and run migrations.
pub fn open_db(root: &Path) -> Result<Connection> {
    let db_file = root.join("amux.db");
    std::fs::create_dir_all(root)
        .with_context(|| format!("Failed to create headless root {}", root.display()))?;

    let conn = Connection::open(&db_file)
        .with_context(|| format!("Failed to open SQLite database {}", db_file.display()))?;

    // Enable WAL mode for better concurrent read performance.
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;

    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
            id         TEXT PRIMARY KEY,
            workdir    TEXT NOT NULL,
            created_at TEXT NOT NULL,
            status     TEXT NOT NULL DEFAULT 'active',
            closed_at  TEXT
        );

        CREATE TABLE IF NOT EXISTS commands (
            id          TEXT PRIMARY KEY,
            session_id  TEXT NOT NULL REFERENCES sessions(id),
            subcommand  TEXT NOT NULL,
            args        TEXT NOT NULL,
            status      TEXT NOT NULL DEFAULT 'pending',
            exit_code   INTEGER,
            started_at  TEXT,
            finished_at TEXT,
            log_path    TEXT NOT NULL
        );",
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Session operations
// ---------------------------------------------------------------------------

pub struct SessionRow {
    pub id: String,
    pub workdir: String,
    pub created_at: String,
    pub status: String,
    pub closed_at: Option<String>,
}

pub fn insert_session(conn: &Connection, id: &str, workdir: &str, created_at: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions (id, workdir, created_at, status) VALUES (?1, ?2, ?3, 'active')",
        rusqlite::params![id, workdir, created_at],
    )?;
    Ok(())
}

pub fn get_session(conn: &Connection, id: &str) -> Result<Option<SessionRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, workdir, created_at, status, closed_at FROM sessions WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], |row| {
        Ok(SessionRow {
            id: row.get(0)?,
            workdir: row.get(1)?,
            created_at: row.get(2)?,
            status: row.get(3)?,
            closed_at: row.get(4)?,
        })
    })?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn list_sessions(conn: &Connection) -> Result<Vec<SessionRow>> {
    let mut stmt =
        conn.prepare("SELECT id, workdir, created_at, status, closed_at FROM sessions ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| {
        Ok(SessionRow {
            id: row.get(0)?,
            workdir: row.get(1)?,
            created_at: row.get(2)?,
            status: row.get(3)?,
            closed_at: row.get(4)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn close_session(conn: &Connection, id: &str, closed_at: &str) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE sessions SET status = 'closed', closed_at = ?1 WHERE id = ?2 AND status = 'active'",
        rusqlite::params![closed_at, id],
    )?;
    Ok(affected > 0)
}

pub fn count_active_sessions(conn: &Connection) -> Result<i64> {
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM sessions WHERE status = 'active'", [], |r| r.get(0))?;
    Ok(count)
}

// ---------------------------------------------------------------------------
// Command operations
// ---------------------------------------------------------------------------

pub struct CommandRow {
    pub id: String,
    pub session_id: String,
    pub subcommand: String,
    pub args: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub log_path: String,
}

pub fn insert_command(
    conn: &Connection,
    id: &str,
    session_id: &str,
    subcommand: &str,
    args: &str,
    log_path: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO commands (id, session_id, subcommand, args, status, log_path)
         VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
        rusqlite::params![id, session_id, subcommand, args, log_path],
    )?;
    Ok(())
}

pub fn get_command(conn: &Connection, id: &str) -> Result<Option<CommandRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, subcommand, args, status, exit_code, started_at, finished_at, log_path
         FROM commands WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![id], |row| {
        Ok(CommandRow {
            id: row.get(0)?,
            session_id: row.get(1)?,
            subcommand: row.get(2)?,
            args: row.get(3)?,
            status: row.get(4)?,
            exit_code: row.get(5)?,
            started_at: row.get(6)?,
            finished_at: row.get(7)?,
            log_path: row.get(8)?,
        })
    })?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn update_command_started(conn: &Connection, id: &str, started_at: &str) -> Result<()> {
    conn.execute(
        "UPDATE commands SET status = 'running', started_at = ?1 WHERE id = ?2",
        rusqlite::params![started_at, id],
    )?;
    Ok(())
}

pub fn update_command_finished(
    conn: &Connection,
    id: &str,
    status: &str,
    exit_code: Option<i32>,
    finished_at: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE commands SET status = ?1, exit_code = ?2, finished_at = ?3 WHERE id = ?4",
        rusqlite::params![status, exit_code, finished_at, id],
    )?;
    Ok(())
}

pub fn count_running_commands(conn: &Connection) -> Result<i64> {
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM commands WHERE status = 'running'", [], |r| r.get(0))?;
    Ok(count)
}

pub fn has_running_command_for_session(conn: &Connection, session_id: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM commands WHERE session_id = ?1 AND status IN ('pending', 'running')",
        rusqlite::params![session_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tmp_db() -> (TempDir, Connection) {
        let tmp = TempDir::new().unwrap();
        let conn = open_db(tmp.path()).unwrap();
        (tmp, conn)
    }

    #[test]
    fn schema_creates_sessions_and_commands_tables() {
        let (_tmp, conn) = make_tmp_db();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('sessions','commands')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2, "both tables must be created by migrate()");
    }

    #[test]
    fn wal_mode_is_enabled_after_open() {
        let (_tmp, conn) = make_tmp_db();
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, "wal");
    }

    // ── Session operations ──────────────────────────────────────────────────

    #[test]
    fn session_insert_and_query_round_trip_all_fields() {
        let (_tmp, conn) = make_tmp_db();
        let id = "aaaaaaaa-0000-0000-0000-000000000001";
        let workdir = "/workspace/project";
        let created_at = "2024-01-15T10:00:00Z";

        insert_session(&conn, id, workdir, created_at).unwrap();

        let row = get_session(&conn, id).unwrap().expect("session should be found");
        assert_eq!(row.id, id);
        assert_eq!(row.workdir, workdir);
        assert_eq!(row.created_at, created_at);
        assert_eq!(row.status, "active");
        assert!(row.closed_at.is_none(), "closed_at must be NULL on insert");
    }

    #[test]
    fn session_get_returns_none_for_nonexistent_id() {
        let (_tmp, conn) = make_tmp_db();
        let result = get_session(&conn, "does-not-exist").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn session_close_sets_status_and_closed_at() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();

        let closed_at = "2024-01-01T01:00:00Z";
        let updated = close_session(&conn, "s1", closed_at).unwrap();
        assert!(updated, "close_session must return true when it modifies a row");

        let row = get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(row.status, "closed");
        assert_eq!(row.closed_at.as_deref(), Some(closed_at));
    }

    #[test]
    fn session_close_returns_false_for_already_closed() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();
        close_session(&conn, "s1", "2024-01-01T01:00:00Z").unwrap();

        // Second close must be idempotent and return false.
        let updated = close_session(&conn, "s1", "2024-01-01T02:00:00Z").unwrap();
        assert!(!updated);
    }

    #[test]
    fn session_close_returns_false_for_nonexistent_id() {
        let (_tmp, conn) = make_tmp_db();
        let updated = close_session(&conn, "does-not-exist", "2024-01-01T00:00:00Z").unwrap();
        assert!(!updated);
    }

    #[test]
    fn count_active_sessions_excludes_closed() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w1", "2024-01-01T00:00:00Z").unwrap();
        insert_session(&conn, "s2", "/tmp/w2", "2024-01-01T00:01:00Z").unwrap();
        insert_session(&conn, "s3", "/tmp/w3", "2024-01-01T00:02:00Z").unwrap();
        close_session(&conn, "s1", "2024-01-01T01:00:00Z").unwrap();

        assert_eq!(count_active_sessions(&conn).unwrap(), 2);
    }

    #[test]
    fn count_active_sessions_is_zero_on_empty_db() {
        let (_tmp, conn) = make_tmp_db();
        assert_eq!(count_active_sessions(&conn).unwrap(), 0);
    }

    #[test]
    fn list_sessions_returns_rows_ordered_by_created_at() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w1", "2024-01-01T00:00:00Z").unwrap();
        insert_session(&conn, "s2", "/tmp/w2", "2024-01-01T00:01:00Z").unwrap();
        insert_session(&conn, "s3", "/tmp/w3", "2024-01-01T00:02:00Z").unwrap();

        let sessions = list_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].id, "s1");
        assert_eq!(sessions[1].id, "s2");
        assert_eq!(sessions[2].id, "s3");
    }

    // ── Command operations ──────────────────────────────────────────────────

    #[test]
    fn command_insert_and_query_round_trip_all_fields() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();

        let id = "cmd-0001";
        let session_id = "s1";
        let subcommand = "implement";
        let args = r#"["--yolo","0001"]"#;
        let log_path = "/root/.amux/headless/sessions/s1/commands/cmd-0001/output.log";

        insert_command(&conn, id, session_id, subcommand, args, log_path).unwrap();

        let cmd = get_command(&conn, id).unwrap().expect("command should be found");
        assert_eq!(cmd.id, id);
        assert_eq!(cmd.session_id, session_id);
        assert_eq!(cmd.subcommand, subcommand);
        assert_eq!(cmd.args, args);
        assert_eq!(cmd.status, "pending");
        assert!(cmd.exit_code.is_none());
        assert!(cmd.started_at.is_none());
        assert!(cmd.finished_at.is_none());
        assert_eq!(cmd.log_path, log_path);
    }

    #[test]
    fn command_get_returns_none_for_nonexistent_id() {
        let (_tmp, conn) = make_tmp_db();
        assert!(get_command(&conn, "does-not-exist").unwrap().is_none());
    }

    #[test]
    fn update_command_started_sets_running_and_started_at() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();
        insert_command(&conn, "c1", "s1", "implement", "[]", "/out").unwrap();

        let started_at = "2024-01-01T00:00:01Z";
        update_command_started(&conn, "c1", started_at).unwrap();

        let cmd = get_command(&conn, "c1").unwrap().unwrap();
        assert_eq!(cmd.status, "running");
        assert_eq!(cmd.started_at.as_deref(), Some(started_at));
        assert!(cmd.exit_code.is_none());
        assert!(cmd.finished_at.is_none());
    }

    #[test]
    fn update_command_finished_done_sets_all_fields() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();
        insert_command(&conn, "c1", "s1", "implement", "[]", "/out").unwrap();
        update_command_started(&conn, "c1", "2024-01-01T00:00:01Z").unwrap();

        let finished_at = "2024-01-01T00:01:00Z";
        update_command_finished(&conn, "c1", "done", Some(0), finished_at).unwrap();

        let cmd = get_command(&conn, "c1").unwrap().unwrap();
        assert_eq!(cmd.status, "done");
        assert_eq!(cmd.exit_code, Some(0));
        assert_eq!(cmd.finished_at.as_deref(), Some(finished_at));
    }

    #[test]
    fn update_command_finished_error_stores_nonzero_exit_code() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();
        insert_command(&conn, "c1", "s1", "chat", "[]", "/out").unwrap();
        update_command_started(&conn, "c1", "2024-01-01T00:00:01Z").unwrap();
        update_command_finished(&conn, "c1", "error", Some(2), "2024-01-01T00:01:00Z").unwrap();

        let cmd = get_command(&conn, "c1").unwrap().unwrap();
        assert_eq!(cmd.status, "error");
        assert_eq!(cmd.exit_code, Some(2));
    }

    #[test]
    fn update_command_finished_null_exit_code_stores_none() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();
        insert_command(&conn, "c1", "s1", "chat", "[]", "/out").unwrap();
        update_command_started(&conn, "c1", "2024-01-01T00:00:01Z").unwrap();
        update_command_finished(&conn, "c1", "error", None, "2024-01-01T00:01:00Z").unwrap();

        let cmd = get_command(&conn, "c1").unwrap().unwrap();
        assert_eq!(cmd.status, "error");
        assert!(cmd.exit_code.is_none());
    }

    #[test]
    fn count_running_commands_tracks_state_transitions() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();

        assert_eq!(count_running_commands(&conn).unwrap(), 0, "empty DB");

        insert_command(&conn, "c1", "s1", "implement", "[]", "/out").unwrap();
        assert_eq!(count_running_commands(&conn).unwrap(), 0, "pending is not running");

        update_command_started(&conn, "c1", "2024-01-01T00:00:01Z").unwrap();
        assert_eq!(count_running_commands(&conn).unwrap(), 1, "running");

        update_command_finished(&conn, "c1", "done", Some(0), "2024-01-01T00:01:00Z").unwrap();
        assert_eq!(count_running_commands(&conn).unwrap(), 0, "done is not running");
    }

    #[test]
    fn has_running_command_for_session_detects_pending() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();
        insert_command(&conn, "c1", "s1", "implement", "[]", "/out").unwrap();

        assert!(has_running_command_for_session(&conn, "s1").unwrap());
    }

    #[test]
    fn has_running_command_for_session_detects_running() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();
        insert_command(&conn, "c1", "s1", "implement", "[]", "/out").unwrap();
        update_command_started(&conn, "c1", "2024-01-01T00:00:01Z").unwrap();

        assert!(has_running_command_for_session(&conn, "s1").unwrap());
    }

    #[test]
    fn has_running_command_for_session_false_when_no_commands() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();

        assert!(!has_running_command_for_session(&conn, "s1").unwrap());
    }

    #[test]
    fn has_running_command_for_session_false_after_done() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();
        insert_command(&conn, "c1", "s1", "implement", "[]", "/out").unwrap();
        update_command_started(&conn, "c1", "2024-01-01T00:00:01Z").unwrap();
        update_command_finished(&conn, "c1", "done", Some(0), "2024-01-01T00:01:00Z").unwrap();

        assert!(!has_running_command_for_session(&conn, "s1").unwrap());
    }

    // ── UUID uniqueness ─────────────────────────────────────────────────────

    #[test]
    fn uuid_v4_ids_are_unique_across_bulk_insert() {
        let (_tmp, conn) = make_tmp_db();
        let n = 20usize;
        let mut inserted_ids = std::collections::HashSet::new();

        for i in 0..n {
            let id = uuid::Uuid::new_v4().to_string();
            let created_at = format!("2024-01-01T{:02}:00:00Z", i % 24);
            insert_session(&conn, &id, "/tmp/w", &created_at).unwrap();
            inserted_ids.insert(id);
        }

        assert_eq!(inserted_ids.len(), n, "all generated UUIDs must be distinct");

        // Confirm DB has exactly n rows.
        let rows = list_sessions(&conn).unwrap();
        assert_eq!(rows.len(), n);

        let db_ids: std::collections::HashSet<String> = rows.into_iter().map(|s| s.id).collect();
        assert_eq!(db_ids, inserted_ids);
    }

    // ── Serde round-trip for args JSON field ────────────────────────────────

    #[test]
    fn command_args_json_field_round_trips_through_serde() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();

        // Store a richer args array.
        let args_original = r#"["--yolo","0042","--worktree","--mount-ssh"]"#;
        insert_command(&conn, "c1", "s1", "implement", args_original, "/out").unwrap();

        let cmd = get_command(&conn, "c1").unwrap().unwrap();

        // The raw string must survive unchanged.
        let v_original: serde_json::Value = serde_json::from_str(args_original).unwrap();
        let v_retrieved: serde_json::Value = serde_json::from_str(&cmd.args).unwrap();
        assert_eq!(v_original, v_retrieved);
    }

    #[test]
    fn command_args_empty_json_array_round_trips() {
        let (_tmp, conn) = make_tmp_db();
        insert_session(&conn, "s1", "/tmp/w", "2024-01-01T00:00:00Z").unwrap();
        insert_command(&conn, "c1", "s1", "status", "[]", "/out").unwrap();

        let cmd = get_command(&conn, "c1").unwrap().unwrap();
        let v: serde_json::Value = serde_json::from_str(&cmd.args).unwrap();
        assert_eq!(v, serde_json::json!([]));
    }

    // ── headless_root respects AMUX_HEADLESS_ROOT ───────────────────────────

    #[test]
    fn headless_root_uses_env_var_when_set() {
        // Use a fixed path that doesn't exist on disk — we only test the return value.
        std::env::set_var("AMUX_HEADLESS_ROOT", "/custom/headless/root");
        let root = headless_root().unwrap();
        std::env::remove_var("AMUX_HEADLESS_ROOT");

        assert_eq!(root, std::path::PathBuf::from("/custom/headless/root"));
    }
}
