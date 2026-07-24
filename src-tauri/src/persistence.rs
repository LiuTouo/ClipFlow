//! Optional SQLite write-through persistence for the clipboard History.
//! Enabled via the `persist` config option; the database lives next to the
//! executable (`clipflow.db`) so portable installs stay self-contained.

use rusqlite::{params, Connection};

use crate::models::{Clip, ClipKind};

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

    /// Remove the database file (used when persistence is disabled).
    pub fn delete_file() -> Result<(), String> {
        let path = db_path();
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete {}: {}", path.display(), e))?;
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Clip, ClipKind};

    fn test_persistence() -> Persistence {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
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
}
