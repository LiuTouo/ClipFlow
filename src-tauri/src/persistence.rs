//! Optional SQLite write-through persistence for the clipboard History.
//! Enabled via the `persist` config option; the database lives next to the
//! executable (`clipflow.db`) so portable installs stay self-contained.

use rusqlite::{params, Connection};

use crate::models::{Clip, ClipKind};

pub struct Persistence {
    conn: Connection,
}

impl Persistence {
    /// Open (creating if necessary) the database next to the executable.
    pub fn open() -> Result<Self, String> {
        let path = db_path();
        let conn = Connection::open(&path)
            .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
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
        .map_err(|e| format!("Failed to initialize database schema: {}", e))?;
        Ok(Self { conn })
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
        self.conn
            .execute(
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

    /// Replace the entire table contents with the given Clips.
    pub fn dump(&self, clips: &[Clip]) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM clips", [])
            .map_err(|e| e.to_string())?;
        for clip in clips {
            self.upsert_capture(clip)?;
        }
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

fn db_path() -> std::path::PathBuf {
    crate::models::data_dir().join("clipflow.db")
}
