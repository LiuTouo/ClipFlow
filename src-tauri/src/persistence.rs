//! Optional SQLite write-through persistence for the clipboard History.
//! Enabled via the `persist` config option; the database lives next to the
//! executable (`clipflow.db`) so portable installs stay self-contained.

use std::collections::HashSet;

use rusqlite::{params, Connection};

use crate::models::{Clip, ClipKind};

/// Minimum time between stale-row reconciliations: 72 hours, in milliseconds
/// (the same unit as `Clip::captured_at` and the monitor's clock).
pub const CLEANUP_INTERVAL_MS: u64 = 72 * 60 * 60 * 1000;

/// Metadata key holding the last cleanup timestamp (ms since Unix epoch).
const LAST_CLEANUP_KEY: &str = "last_cleanup";

pub struct Persistence {
    conn: Connection,
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS clips (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            text_content TEXT,
            image_data BLOB,
            thumbnail_base64 TEXT,
            content_hash TEXT NOT NULL UNIQUE,
            preview TEXT NOT NULL,
            truncated INTEGER NOT NULL,
            source_exe TEXT NOT NULL,
            source_title TEXT NOT NULL,
            source_icon TEXT,
            captured_at INTEGER NOT NULL,
            pinned INTEGER NOT NULL,
            byte_size INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize database schema: {}", e))
}

impl Persistence {
    /// Open (creating if necessary) the database next to the executable.
    pub fn open() -> Result<Self, String> {
        let path = db_path();
        let conn = Connection::open(&path)
            .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
        init_schema(&conn)?;
        Ok(Self { conn })
    }

    #[cfg(test)]
    fn from_conn(conn: Connection) -> Self {
        Self { conn }
    }

    /// Record the current time as the last-cleanup gate. Disabling persistence
    /// calls this so the leftover DB survives a 72-hour grace before a later
    /// startup reconciliation purges its now-stale rows.
    pub fn record_last_cleanup(&self, now_ms: u64) -> Result<(), String> {
        set_meta(&self.conn, LAST_CLEANUP_KEY, now_ms as i64)
    }

    /// Reconcile only when due. Returns `true` when a cleanup ran.
    pub fn reconcile_if_due(&mut self, active_ids: &[&str], now_ms: u64) -> Result<bool, String> {
        reconcile_if_due(&mut self.conn, active_ids, now_ms)
    }

    /// Load every Clip, oldest first, so in-memory insertion order and
    /// capacity eviction produce the correct final state.
    pub fn load_all(&self) -> Result<Vec<Clip>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, kind, text_content, image_data, thumbnail_base64,
                        content_hash, preview, truncated, source_exe, source_title,
                        source_icon, captured_at, pinned, byte_size
                 FROM clips ORDER BY captured_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                let kind_str: String = row.get(1)?;
                let kind = match kind_str.as_str() {
                    "Image" => ClipKind::Image,
                    "FilePaths" => ClipKind::FilePaths,
                    _ => ClipKind::Text,
                };
                Ok(Clip {
                    id: row.get(0)?,
                    kind,
                    text_content: row.get(2)?,
                    image_data: row.get(3)?,
                    thumbnail_base64: row.get(4)?,
                    content_hash: row.get(5)?,
                    preview: row.get(6)?,
                    truncated: row.get::<_, i64>(7)? != 0,
                    source_exe: row.get(8)?,
                    source_title: row.get(9)?,
                    source_icon: row.get(10)?,
                    captured_at: row.get::<_, i64>(11)? as u64,
                    pinned: row.get::<_, i64>(12)? != 0,
                    byte_size: row.get::<_, i64>(13)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut clips = Vec::new();
        for clip in rows {
            clips.push(clip.map_err(|e| e.to_string())?);
        }
        Ok(clips)
    }

    /// Insert or refresh a Clip after a capture. On a content-hash conflict
    /// only the recency fields change — matching in-memory dedup semantics.
    pub fn upsert_capture(&self, clip: &Clip) -> Result<(), String> {
        upsert_on(&self.conn, clip)
    }

    /// Replace the entire table contents with the given Clips. Transactional:
    /// a crash mid-dump must not leave the database with a partial history.
    pub fn dump(&mut self, clips: &[Clip]) -> Result<(), String> {
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM clips", [])
            .map_err(|e| e.to_string())?;
        for clip in clips {
            upsert_on(&tx, clip)?;
        }
        tx.commit()
            .map_err(|e| format!("Failed to commit history dump: {}", e))?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM clips WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE clips SET pinned = ?1 WHERE id = ?2",
                params![pinned as i64, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Disable persistence: record the durable last-cleanup gate, then drop the
/// live connection. If the gate write fails the connection is left installed so
/// the caller surfaces the error (and rollback stays truthful) instead of
/// reporting success without a stored 72-hour baseline.
pub fn disable(persistence: &mut Option<Persistence>, now_ms: u64) -> Result<(), String> {
    if let Some(p) = persistence.as_ref() {
        p.record_last_cleanup(now_ms)?;
    }
    *persistence = None;
    Ok(())
}

/// Pure, deterministic due check: cleanup is due when it has never run, or at
/// least `CLEANUP_INTERVAL_MS` has elapsed since the last run. `now_ms` is an
/// explicit current timestamp so tests pin the clock.
pub fn cleanup_due(last_cleanup: Option<u64>, now_ms: u64) -> bool {
    match last_cleanup {
        None => true,
        Some(t) => now_ms.saturating_sub(t) >= CLEANUP_INTERVAL_MS,
    }
}

fn get_meta(conn: &Connection, key: &str) -> Result<Option<i64>, String> {
    let mut stmt = conn
        .prepare("SELECT value FROM meta WHERE key = ?1")
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map(params![key], |row| row.get::<_, i64>(0))
        .map_err(|e| e.to_string())?;
    match rows.next() {
        Some(Ok(v)) => Ok(Some(v)),
        Some(Err(e)) => Err(e.to_string()),
        None => Ok(None),
    }
}

fn set_meta(conn: &Connection, key: &str, value: i64) -> Result<(), String> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn last_cleanup_ms(conn: &Connection) -> Result<Option<u64>, String> {
    get_meta(conn, LAST_CLEANUP_KEY).map(|v| v.map(|n| n as u64))
}

fn persisted_ids(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM clips")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    for id in rows {
        ids.push(id.map_err(|e| e.to_string())?);
    }
    Ok(ids)
}

/// Delete every persisted clip whose id is absent from `active_ids`, then
/// record `now_ms` as the last-cleanup time — all in one transaction, so the
/// timestamp is written only after the deletions succeed (a failure rolls both
/// back). An empty `active_ids` purges the whole table (the disabled case).
pub fn reconcile_stale(
    conn: &mut Connection,
    active_ids: &[&str],
    now_ms: u64,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let active: HashSet<&str> = active_ids.iter().copied().collect();
    for id in persisted_ids(&tx)? {
        if !active.contains(id.as_str()) {
            tx.execute("DELETE FROM clips WHERE id = ?1", params![id])
                .map_err(|e| e.to_string())?;
        }
    }
    set_meta(&tx, LAST_CLEANUP_KEY, now_ms as i64)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// Reconcile only when due. Returns `true` when a cleanup ran.
pub fn reconcile_if_due(
    conn: &mut Connection,
    active_ids: &[&str],
    now_ms: u64,
) -> Result<bool, String> {
    if !cleanup_due(last_cleanup_ms(conn)?, now_ms) {
        return Ok(false);
    }
    reconcile_stale(conn, active_ids, now_ms)?;
    Ok(true)
}

fn kind_str(kind: &ClipKind) -> &'static str {
    match kind {
        ClipKind::Text => "Text",
        ClipKind::Image => "Image",
        ClipKind::FilePaths => "FilePaths",
    }
}

/// The upsert behind both upsert_capture and dump, written against
/// &Connection so a transaction (which derefs to Connection) can use it.
fn upsert_on(conn: &Connection, clip: &Clip) -> Result<(), String> {
    conn.execute(
        "INSERT INTO clips (id, kind, text_content, image_data, thumbnail_base64,
                            content_hash, preview, truncated, source_exe, source_title,
                            source_icon, captured_at, pinned, byte_size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(content_hash) DO UPDATE SET
            captured_at = excluded.captured_at,
            source_exe = excluded.source_exe,
            source_title = excluded.source_title",
        params![
            clip.id,
            kind_str(&clip.kind),
            clip.text_content,
            clip.image_data,
            clip.thumbnail_base64,
            clip.content_hash,
            clip.preview,
            clip.truncated as i64,
            clip.source_exe,
            clip.source_title,
            clip.source_icon,
            clip.captured_at as i64,
            clip.pinned as i64,
            clip.byte_size as i64,
        ],
    )
    .map_err(|e| format!("Failed to persist clip: {}", e))?;
    Ok(())
}

fn db_path() -> std::path::PathBuf {
    crate::models::data_dir().join("clipflow.db")
}

/// True when the database file exists on disk. Used to decide whether a
/// disabled-persistence startup should attempt stale-row reconciliation
/// without creating a new file via `Connection::open`.
pub fn db_exists() -> bool {
    db_path().exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Clip, ClipKind};

    fn test_persistence() -> Persistence {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        Persistence::from_conn(conn)
    }

    /// A live connection whose `meta` table is gone, so `record_last_cleanup`
    /// (an INSERT into `meta`) fails deterministically.
    fn persistence_without_meta() -> Persistence {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn.execute_batch("DROP TABLE meta").unwrap();
        Persistence::from_conn(conn)
    }

    fn clip(id: &str, hash: &str, captured_at: u64) -> Clip {
        Clip {
            id: id.to_string(),
            kind: ClipKind::Text,
            text_content: Some(format!("content-{id}")),
            image_data: None,
            thumbnail_base64: None,
            content_hash: hash.to_string(),
            preview: format!("preview-{id}"),
            truncated: false,
            source_exe: "test.exe".to_string(),
            source_title: String::new(),
            source_icon: None,
            captured_at,
            pinned: false,
            byte_size: 10,
        }
    }

    #[test]
    fn dump_replaces_all_previous_rows() {
        let mut p = test_persistence();
        p.dump(&[clip("c1", "h1", 1), clip("c2", "h2", 2)]).unwrap();
        p.dump(&[clip("c3", "h3", 3)]).unwrap();
        let clips = p.load_all().unwrap();
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].id, "c3");
    }

    #[test]
    fn dump_round_trips_clip_fields() {
        let mut p = test_persistence();
        let mut original = clip("c1", "h1", 42);
        original.pinned = true;
        original.truncated = true;
        p.dump(std::slice::from_ref(&original)).unwrap();
        let loaded = p.load_all().unwrap();
        assert_eq!(loaded.len(), 1);
        let c = &loaded[0];
        assert_eq!(c.id, original.id);
        assert_eq!(c.content_hash, original.content_hash);
        assert_eq!(c.text_content, original.text_content);
        assert_eq!(c.captured_at, original.captured_at);
        assert!(c.pinned);
        assert!(c.truncated);
        assert_eq!(c.byte_size, original.byte_size);
    }

    #[test]
    fn cleanup_due_never_run() {
        assert!(cleanup_due(None, 0));
    }

    #[test]
    fn cleanup_not_due_before_interval() {
        let last = Some(1_000_000);
        assert!(!cleanup_due(last, 1_000_000 + CLEANUP_INTERVAL_MS - 1));
    }

    #[test]
    fn cleanup_due_at_and_after_interval() {
        let last = Some(1_000_000);
        assert!(cleanup_due(last, 1_000_000 + CLEANUP_INTERVAL_MS));
        assert!(cleanup_due(last, 1_000_000 + CLEANUP_INTERVAL_MS + 1));
    }

    #[test]
    fn reconcile_deletes_stale_rows_and_preserves_active() {
        let mut p = test_persistence();
        p.dump(&[clip("a", "ha", 1), clip("b", "hb", 2), clip("c", "hc", 3)])
            .unwrap();
        let now = 42_000_000;
        reconcile_stale(&mut p.conn, &["a", "c"], now).unwrap();
        let mut ids: Vec<String> = p.load_all().unwrap().into_iter().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, vec!["a".to_string(), "c".to_string()]);
        assert_eq!(last_cleanup_ms(&p.conn).unwrap(), Some(now));
    }

    #[test]
    fn reconcile_empty_active_purges_everything() {
        let mut p = test_persistence();
        p.dump(&[clip("a", "ha", 1), clip("b", "hb", 2)]).unwrap();
        reconcile_stale(&mut p.conn, &[], 7_000_000).unwrap();
        assert!(p.load_all().unwrap().is_empty());
    }

    #[test]
    fn reconcile_updates_timestamp_only_after_success() {
        // Not due → no reconciliation runs, rows and timestamp stay put.
        let mut p = test_persistence();
        p.dump(&[clip("a", "ha", 1)]).unwrap();
        p.record_last_cleanup(10_000_000).unwrap();
        let ran = p
            .reconcile_if_due(&[], 10_000_000 + CLEANUP_INTERVAL_MS - 1)
            .unwrap();
        assert!(!ran);
        assert_eq!(p.load_all().unwrap().len(), 1);
        assert_eq!(last_cleanup_ms(&p.conn).unwrap(), Some(10_000_000));
    }

    #[test]
    fn reconcile_if_due_runs_after_interval() {
        let mut p = test_persistence();
        p.dump(&[clip("a", "ha", 1)]).unwrap();
        p.record_last_cleanup(10_000_000).unwrap();
        let ran = p
            .reconcile_if_due(&[], 10_000_000 + CLEANUP_INTERVAL_MS)
            .unwrap();
        assert!(ran);
        assert!(p.load_all().unwrap().is_empty());
        assert_eq!(
            last_cleanup_ms(&p.conn).unwrap(),
            Some(10_000_000 + CLEANUP_INTERVAL_MS)
        );
    }

    #[test]
    fn disable_writes_gate_then_drops_connection() {
        let mut p = test_persistence();
        p.dump(&[clip("a", "ha", 1)]).unwrap();
        let mut opt = Some(p);
        disable(&mut opt, 5_000_000).unwrap();
        assert!(opt.is_none());
    }

    #[test]
    fn disable_keeps_connection_when_gate_write_fails() {
        let mut opt = Some(persistence_without_meta());
        assert!(disable(&mut opt, 5_000_000).is_err());
        assert!(
            opt.is_some(),
            "gate write failure must not drop the connection"
        );
    }

    #[test]
    fn disable_lifecycle_records_gate_then_purges_after_grace() {
        // Disable: record the gate timestamp and leave rows in place.
        let mut p = test_persistence();
        p.dump(&[clip("a", "ha", 1)]).unwrap();
        let disabled_at = 50_000_000;
        p.record_last_cleanup(disabled_at).unwrap();
        assert_eq!(p.load_all().unwrap().len(), 1);

        // Within the grace period nothing is purged.
        assert!(!p
            .reconcile_if_due(&[], disabled_at + CLEANUP_INTERVAL_MS - 1)
            .unwrap());
        assert_eq!(p.load_all().unwrap().len(), 1);

        // After 72h, startup reconciliation purges the stale rows.
        assert!(p
            .reconcile_if_due(&[], disabled_at + CLEANUP_INTERVAL_MS)
            .unwrap());
        assert!(p.load_all().unwrap().is_empty());
    }
}
