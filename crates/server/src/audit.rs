use adb_remote_protocol::AuditEvent;
use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct AuditStore {
    conn: Arc<Mutex<Connection>>,
}

impl AuditStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init()?;
        Ok(store)
    }

    pub fn init(&self) -> Result<()> {
        self.conn.lock().expect("audit mutex").execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                display_name TEXT NOT NULL DEFAULT '',
                lease_id TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL DEFAULT '',
                reason TEXT NOT NULL DEFAULT '',
                bytes_from_user INTEGER NOT NULL DEFAULT 0,
                bytes_from_device INTEGER NOT NULL DEFAULT 0,
                created_at_unix_millis INTEGER NOT NULL
            );",
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        event_type: &str,
        display_name: impl Into<String>,
        lease_id: impl Into<String>,
        source: impl Into<String>,
        reason: impl Into<String>,
        bytes_from_user: u64,
        bytes_from_device: u64,
    ) {
        let display_name = display_name.into();
        let lease_id = lease_id.into();
        let source = source.into();
        let reason = reason.into();
        let now = Utc::now().timestamp_millis();
        let result = self.conn.lock().expect("audit mutex").execute(
            "INSERT INTO audit_events
             (event_type, display_name, lease_id, source, reason, bytes_from_user, bytes_from_device, created_at_unix_millis)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event_type,
                display_name,
                lease_id,
                source,
                reason,
                bytes_from_user as i64,
                bytes_from_device as i64,
                now,
            ],
        );
        if let Err(error) = result {
            tracing::warn!(%error, "failed to record audit event");
        }
    }

    pub fn list(&self, limit: u32) -> Result<Vec<AuditEvent>> {
        let limit = if limit == 0 { 50 } else { limit.min(500) };
        let conn = self.conn.lock().expect("audit mutex");
        let mut stmt = conn.prepare(
            "SELECT id, event_type, display_name, lease_id, source, reason,
                    bytes_from_user, bytes_from_device, created_at_unix_millis
             FROM audit_events
             ORDER BY id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |row| {
            Ok(AuditEvent {
                id: row.get(0)?,
                event_type: row.get(1)?,
                display_name: row.get(2)?,
                lease_id: row.get(3)?,
                source: row.get(4)?,
                reason: row.get(5)?,
                bytes_from_user: row.get::<_, i64>(6)? as u64,
                bytes_from_device: row.get::<_, i64>(7)? as u64,
                created_at_unix_millis: row.get(8)?,
            })
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("list audit events")
    }
}
