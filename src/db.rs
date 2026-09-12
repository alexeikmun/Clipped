use rusqlite::{params, Connection, OptionalExtension, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

const DB_FILE: &str = "clips.db";
const LEGACY_HISTORY_FILE: &str = "clipboard_history.json";
const MAX_PREVIEW_LEN: usize = 1000;

fn default_clip_type() -> String {
    "text".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClipItem {
    pub id: String,
    pub text: String,
    pub is_favorite: bool,
    #[serde(default = "default_clip_type")]
    pub clip_type: String,
    #[serde(default)]
    pub image_path: Option<String>,
    #[serde(default)]
    pub image_width: Option<u32>,
    #[serde(default)]
    pub image_height: Option<u32>,
    #[serde(default)]
    pub ocr_text: Option<String>,
    #[serde(default)]
    pub full_text_len: usize,
}

fn truncate_preview(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        s.chars().take(max_len).collect()
    } else {
        s.to_string()
    }
}

pub struct Database {
    writer_conn: Mutex<Connection>,
    reader_conn: Mutex<Connection>,
    data_dir: PathBuf,
}

#[allow(dead_code)]
impl Database {
    pub fn init(data_dir: &Path) -> Result<Self> {
        let db_path = data_dir.join(DB_FILE);
        let writer_conn = Connection::open(&db_path)?;
        let reader_conn = Connection::open(&db_path)?;

        // Configure both connections for high-concurrency WAL mode
        for conn in [&writer_conn, &reader_conn] {
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.pragma_update(None, "temp_store", "MEMORY")?;
            conn.pragma_update(None, "busy_timeout", 5000)?;
            conn.pragma_update(None, "cache_size", -2000)?; // 2MB cache
        }

        // 1. Create main clips table
        writer_conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS clips (
                id TEXT PRIMARY KEY,
                text TEXT NOT NULL,
                is_favorite INTEGER NOT NULL DEFAULT 0,
                clip_type TEXT NOT NULL DEFAULT 'text',
                image_path TEXT,
                image_width INTEGER,
                image_height INTEGER,
                ocr_text TEXT,
                created_at INTEGER NOT NULL,
                content_hash TEXT
            );
            ",
        )?;

        // Ensure content_hash column exists if upgrading from an older schema
        let _ = writer_conn.execute("ALTER TABLE clips ADD COLUMN content_hash TEXT;", []);

        // 2. Create B-tree indexes
        writer_conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_clips_created_at ON clips(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_clips_is_fav ON clips(is_favorite);
            CREATE INDEX IF NOT EXISTS idx_clips_hash ON clips(content_hash);
            ",
        )?;

        // 3. Configure FTS5 external content table and automatic synchronization triggers
        writer_conn.execute_batch(
            "
            DROP TABLE IF EXISTS clips_fts;

            CREATE VIRTUAL TABLE IF NOT EXISTS clips_fts USING fts5(
                text,
                ocr_text,
                content='clips',
                content_rowid='rowid',
                tokenize='unicode61'
            );

            -- Initial population for any pre-existing rows
            INSERT INTO clips_fts(rowid, text, ocr_text)
            SELECT rowid, text, COALESCE(ocr_text, '') FROM clips;

            -- Triggers to keep FTS index synced with zero Rust overhead
            DROP TRIGGER IF EXISTS clips_ai;
            CREATE TRIGGER clips_ai AFTER INSERT ON clips BEGIN
                INSERT INTO clips_fts(rowid, text, ocr_text)
                VALUES (new.rowid, new.text, COALESCE(new.ocr_text, ''));
            END;

            DROP TRIGGER IF EXISTS clips_ad;
            CREATE TRIGGER clips_ad AFTER DELETE ON clips BEGIN
                INSERT INTO clips_fts(clips_fts, rowid, text, ocr_text)
                VALUES('delete', old.rowid, old.text, COALESCE(old.ocr_text, ''));
            END;

            DROP TRIGGER IF EXISTS clips_au;
            CREATE TRIGGER clips_au AFTER UPDATE ON clips BEGIN
                INSERT INTO clips_fts(clips_fts, rowid, text, ocr_text)
                VALUES('delete', old.rowid, old.text, COALESCE(old.ocr_text, ''));
                INSERT INTO clips_fts(rowid, text, ocr_text)
                VALUES (new.rowid, new.text, COALESCE(new.ocr_text, ''));
            END;
            ",
        )?;

        let db = Database {
            writer_conn: Mutex::new(writer_conn),
            reader_conn: Mutex::new(reader_conn),
            data_dir: data_dir.to_path_buf(),
        };

        // Migrate legacy clipboard_history.json if present
        db.migrate_legacy_history();

        Ok(db)
    }

    /// P1: Startup orphan image file reconciliation
    pub fn reconcile_orphaned_images(&self) {
        let images_dir = self.data_dir.join("images");
        if !images_dir.exists() {
            return;
        }

        let Ok(indexed) = self.get_all_image_paths() else {
            return;
        };

        let Ok(entries) = fs::read_dir(&images_dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let norm = path.to_string_lossy().replace('\\', "/");
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let is_indexed = indexed.iter().any(|idx_path| {
                    idx_path == &norm || (!file_name.is_empty() && idx_path.ends_with(file_name))
                });
                if !is_indexed {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }

    pub fn get_all_image_paths(&self) -> Result<HashSet<String>> {
        let conn = self.reader_conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT image_path FROM clips WHERE clip_type = 'image' AND image_path IS NOT NULL"
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut paths = HashSet::new();
        for p in rows.flatten() {
            paths.insert(p.replace('\\', "/"));
        }
        Ok(paths)
    }

    fn migrate_legacy_history(&self) {
        let legacy_path = self.data_dir.join(LEGACY_HISTORY_FILE);
        if !legacy_path.exists() {
            return;
        }

        let Ok(content) = fs::read_to_string(&legacy_path) else {
            return;
        };

        let mut items: Vec<ClipItem> = Vec::new();

        if let Ok(parsed) = serde_json::from_str::<Vec<ClipItem>>(&content) {
            items = parsed;
        } else if let Ok(old_strings) = serde_json::from_str::<Vec<String>>(&content) {
            items = old_strings
                .into_iter()
                .map(|text| ClipItem {
                    id: Uuid::new_v4().to_string(),
                    text,
                    is_favorite: false,
                    clip_type: "text".to_string(),
                    image_path: None,
                    image_width: None,
                    image_height: None,
                    ocr_text: None,
                    full_text_len: 0,
                })
                .collect();
        }

        if !items.is_empty() {
            let conn = self.writer_conn.lock().unwrap();
            let tx = conn.unchecked_transaction();
            if let Ok(tx) = tx {
                let base_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64;

                let count = items.len();
                for (idx, item) in items.into_iter().enumerate() {
                    let timestamp = base_time - ((count - idx) as i64 * 10);
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO clips (id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text, created_at, content_hash)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
                        params![
                            item.id,
                            item.text,
                            if item.is_favorite { 1 } else { 0 },
                            item.clip_type,
                            item.image_path,
                            item.image_width,
                            item.image_height,
                            item.ocr_text,
                            timestamp
                        ],
                    );
                }
                let _ = tx.commit();
            }
        }

        let migrated_path = self.data_dir.join(format!("{}.migrated", LEGACY_HISTORY_FILE));
        let _ = fs::rename(&legacy_path, &migrated_path);
    }

    /// Saves a text clip, or bumps an existing clip to the top via indexed 128-bit hash
    pub fn save_or_bump_text(&self, text: String, hash: &str) -> Result<ClipItem> {
        let conn = self.writer_conn.lock().unwrap();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        // P5: Direct indexed lookup by content_hash only (no slow OR text = ?)
        let mut stmt = conn.prepare(
            "SELECT id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text
             FROM clips WHERE clip_type = 'text' AND content_hash = ?1 LIMIT 1"
        )?;

        let existing: Option<ClipItem> = stmt.query_row(params![hash], |row| {
            let full_text: String = row.get(1)?;
            let text_len = full_text.len();
            Ok(ClipItem {
                id: row.get(0)?,
                text: full_text,
                is_favorite: row.get::<_, i32>(2)? != 0,
                clip_type: row.get(3)?,
                image_path: row.get(4)?,
                image_width: row.get(5)?,
                image_height: row.get(6)?,
                ocr_text: row.get(7)?,
                full_text_len: text_len,
            })
        }).optional()?;

        if let Some(clip) = existing {
            conn.execute(
                "UPDATE clips SET created_at = ?1 WHERE id = ?2",
                params![timestamp, clip.id],
            )?;
            return Ok(clip);
        }

        // New clip
        let id = Uuid::new_v4().to_string();
        let text_len = text.len();
        let new_item = ClipItem {
            id: id.clone(),
            text: text.clone(),
            is_favorite: false,
            clip_type: "text".to_string(),
            image_path: None,
            image_width: None,
            image_height: None,
            ocr_text: None,
            full_text_len: text_len,
        };

        conn.execute(
            "INSERT INTO clips (id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text, created_at, content_hash)
             VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                new_item.id,
                new_item.text,
                new_item.clip_type,
                new_item.image_path,
                new_item.image_width,
                new_item.image_height,
                new_item.ocr_text,
                timestamp,
                hash
            ],
        )?;

        Ok(new_item)
    }

    /// Finds an existing image clip by content hash via reader connection
    pub fn find_image_by_hash(&self, hash: &str) -> Result<Option<ClipItem>> {
        let conn = self.reader_conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text
             FROM clips WHERE clip_type = 'image' AND content_hash = ?1 LIMIT 1"
        )?;

        let existing = stmt.query_row(params![hash], |row| {
            let full_text: String = row.get(1)?;
            let text_len = full_text.len();
            Ok(ClipItem {
                id: row.get(0)?,
                text: full_text,
                is_favorite: row.get::<_, i32>(2)? != 0,
                clip_type: row.get(3)?,
                image_path: row.get(4)?,
                image_width: row.get(5)?,
                image_height: row.get(6)?,
                ocr_text: row.get(7)?,
                full_text_len: text_len,
            })
        }).optional()?;

        Ok(existing)
    }

    /// Bumps an existing clip's timestamp to now (moves to top of history)
    pub fn bump_clip_timestamp(&self, id: &str) -> Result<()> {
        let conn = self.writer_conn.lock().unwrap();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        conn.execute(
            "UPDATE clips SET created_at = ?1 WHERE id = ?2",
            params![timestamp, id],
        )?;
        Ok(())
    }

    /// Inserts a new image clip with content hash
    pub fn insert_image_clip(&self, item: &ClipItem, hash: &str) -> Result<()> {
        let conn = self.writer_conn.lock().unwrap();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        conn.execute(
            "INSERT OR REPLACE INTO clips (id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text, created_at, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                item.id,
                item.text,
                if item.is_favorite { 1 } else { 0 },
                item.clip_type,
                item.image_path,
                item.image_width,
                item.image_height,
                item.ocr_text,
                timestamp,
                hash
            ],
        )?;

        Ok(())
    }

    pub fn insert_clip(&self, item: &ClipItem) -> Result<()> {
        let conn = self.writer_conn.lock().unwrap();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        conn.execute(
            "INSERT OR REPLACE INTO clips (id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text, created_at, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
            params![
                item.id,
                item.text,
                if item.is_favorite { 1 } else { 0 },
                item.clip_type,
                item.image_path,
                item.image_width,
                item.image_height,
                item.ocr_text,
                timestamp
            ],
        )?;

        Ok(())
    }

    pub fn get_latest_item(&self) -> Result<Option<ClipItem>> {
        let conn = self.reader_conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text
             FROM clips ORDER BY created_at DESC LIMIT 1"
        )?;

        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let full_text: String = row.get(1)?;
            let ocr: Option<String> = row.get(7)?;
            Ok(Some(ClipItem {
                id: row.get(0)?,
                text: full_text.clone(),
                is_favorite: row.get::<_, i32>(2)? != 0,
                clip_type: row.get(3)?,
                image_path: row.get(4)?,
                image_width: row.get(5)?,
                image_height: row.get(6)?,
                ocr_text: ocr,
                full_text_len: full_text.len(),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_history(&self, limit: usize, favorites_only: bool) -> Result<Vec<ClipItem>> {
        let conn = self.reader_conn.lock().unwrap();
        let sql = if favorites_only {
            "SELECT id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text
             FROM clips WHERE is_favorite = 1 ORDER BY created_at DESC LIMIT ?1"
        } else {
            "SELECT id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text
             FROM clips ORDER BY created_at DESC LIMIT ?1"
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let full_text: String = row.get(1)?;
            let text_len = full_text.len();
            let preview_text = truncate_preview(&full_text, MAX_PREVIEW_LEN);

            let raw_ocr: Option<String> = row.get(7)?;
            let preview_ocr = raw_ocr.map(|o| truncate_preview(&o, MAX_PREVIEW_LEN));

            Ok(ClipItem {
                id: row.get(0)?,
                text: preview_text,
                is_favorite: row.get::<_, i32>(2)? != 0,
                clip_type: row.get(3)?,
                image_path: row.get(4)?,
                image_width: row.get(5)?,
                image_height: row.get(6)?,
                ocr_text: preview_ocr,
                full_text_len: text_len,
            })
        })?;

        let mut items = Vec::new();
        for item in rows {
            items.push(item?);
        }
        Ok(items)
    }

    pub fn search_clips(&self, query: &str, favorites_only: bool, limit: usize) -> Result<Vec<ClipItem>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.get_history(limit, favorites_only);
        }

        let conn = self.reader_conn.lock().unwrap();

        // 1. Build sanitized FTS5 prefix query
        let terms: Vec<String> = trimmed
            .split_whitespace()
            .map(|w| {
                let clean: String = w.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
                clean
            })
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"*", w))
            .collect();

        if !terms.is_empty() {
            let fts_expr = terms.join(" ");
            // P3: External content table query uses direct c.rowid = f.rowid join
            let fts_sql = if favorites_only {
                "SELECT c.id, c.text, c.is_favorite, c.clip_type, c.image_path, c.image_width, c.image_height, c.ocr_text
                 FROM clips c
                 JOIN clips_fts f ON c.rowid = f.rowid
                 WHERE clips_fts MATCH ?1 AND c.is_favorite = 1
                 ORDER BY bm25(clips_fts), c.created_at DESC
                 LIMIT ?2"
            } else {
                "SELECT c.id, c.text, c.is_favorite, c.clip_type, c.image_path, c.image_width, c.image_height, c.ocr_text
                 FROM clips c
                 JOIN clips_fts f ON c.rowid = f.rowid
                 WHERE clips_fts MATCH ?1
                 ORDER BY bm25(clips_fts), c.created_at DESC
                 LIMIT ?2"
            };

            if let Ok(mut stmt) = conn.prepare(fts_sql) {
                let rows_res = stmt.query_map(params![fts_expr, limit as i64], |row| {
                    let full_text: String = row.get(1)?;
                    let text_len = full_text.len();
                    let preview_text = truncate_preview(&full_text, MAX_PREVIEW_LEN);
                    let raw_ocr: Option<String> = row.get(7)?;
                    let preview_ocr = raw_ocr.map(|o| truncate_preview(&o, MAX_PREVIEW_LEN));

                    Ok(ClipItem {
                        id: row.get(0)?,
                        text: preview_text,
                        is_favorite: row.get::<_, i32>(2)? != 0,
                        clip_type: row.get(3)?,
                        image_path: row.get(4)?,
                        image_width: row.get(5)?,
                        image_height: row.get(6)?,
                        ocr_text: preview_ocr,
                        full_text_len: text_len,
                    })
                });

                if let Ok(rows) = rows_res {
                    let mut items = Vec::new();
                    for item in rows.flatten() {
                        items.push(item);
                    }
                    if !items.is_empty() {
                        return Ok(items);
                    }
                }
            }
        }

        // 2. Fallback to LIKE substring search for punctuation, code syntax, or zero FTS results
        let like_pattern = format!("%{}%", trimmed);
        let like_sql = if favorites_only {
            "SELECT id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text
             FROM clips
             WHERE (text LIKE ?1 OR (ocr_text IS NOT NULL AND ocr_text LIKE ?1)) AND is_favorite = 1
             ORDER BY created_at DESC LIMIT ?2"
        } else {
            "SELECT id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text
             FROM clips
             WHERE (text LIKE ?1 OR (ocr_text IS NOT NULL AND ocr_text LIKE ?1))
             ORDER BY created_at DESC LIMIT ?2"
        };

        let mut stmt = conn.prepare(like_sql)?;
        let rows = stmt.query_map(params![like_pattern, limit as i64], |row| {
            let full_text: String = row.get(1)?;
            let text_len = full_text.len();
            let preview_text = truncate_preview(&full_text, MAX_PREVIEW_LEN);
            let raw_ocr: Option<String> = row.get(7)?;
            let preview_ocr = raw_ocr.map(|o| truncate_preview(&o, MAX_PREVIEW_LEN));

            Ok(ClipItem {
                id: row.get(0)?,
                text: preview_text,
                is_favorite: row.get::<_, i32>(2)? != 0,
                clip_type: row.get(3)?,
                image_path: row.get(4)?,
                image_width: row.get(5)?,
                image_height: row.get(6)?,
                ocr_text: preview_ocr,
                full_text_len: text_len,
            })
        })?;

        let mut items = Vec::new();
        for item in rows {
            items.push(item?);
        }
        Ok(items)
    }

    pub fn get_full_clip(&self, id: &str) -> Result<Option<ClipItem>> {
        let conn = self.reader_conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text
             FROM clips WHERE id = ?1 LIMIT 1"
        )?;

        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let full_text: String = row.get(1)?;
            let text_len = full_text.len();
            Ok(Some(ClipItem {
                id: row.get(0)?,
                text: full_text,
                is_favorite: row.get::<_, i32>(2)? != 0,
                clip_type: row.get(3)?,
                image_path: row.get(4)?,
                image_width: row.get(5)?,
                image_height: row.get(6)?,
                ocr_text: row.get(7)?,
                full_text_len: text_len,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn toggle_favorite(&self, id: &str) -> Result<bool> {
        let conn = self.writer_conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT is_favorite FROM clips WHERE id = ?1")?;
        let is_fav: Option<i32> = stmt.query_row(params![id], |r| r.get(0)).ok();

        if let Some(current) = is_fav {
            let new_fav = if current == 1 { 0 } else { 1 };
            conn.execute(
                "UPDATE clips SET is_favorite = ?1 WHERE id = ?2",
                params![new_fav, id],
            )?;
            Ok(new_fav == 1)
        } else {
            Err(rusqlite::Error::QueryReturnedNoRows)
        }
    }

    /// Deletes a clip entirely from the database and returns its image path if it was an image clip
    pub fn delete_clip(&self, id: &str) -> Result<Option<String>> {
        let conn = self.writer_conn.lock().unwrap();
        let img_path: Option<String> = {
            let mut stmt = conn.prepare("SELECT image_path FROM clips WHERE id = ?1")?;
            stmt.query_row(params![id], |r| r.get(0)).optional()?.flatten()
        };

        conn.execute("DELETE FROM clips WHERE id = ?1", params![id])?;
        Ok(img_path)
    }

    pub fn prune_history(&self, max_items: usize) -> Result<Vec<String>> {
        let conn = self.writer_conn.lock().unwrap();
        let total_count: i64 = conn.query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))?;

        if total_count <= max_items as i64 {
            return Ok(Vec::new());
        }

        let excess = (total_count - max_items as i64) as usize;

        // Select oldest non-favorite items to prune
        let mut stmt = conn.prepare(
            "SELECT id, image_path FROM clips WHERE is_favorite = 0 ORDER BY created_at ASC LIMIT ?1"
        )?;

        let rows = stmt.query_map(params![excess as i64], |r| {
            let id: String = r.get(0)?;
            let img_path: Option<String> = r.get(1)?;
            Ok((id, img_path))
        })?;

        let mut to_delete_ids = Vec::new();
        let mut deleted_images = Vec::new();

        for item in rows {
            let (id, img_path) = item?;
            to_delete_ids.push(id);
            if let Some(path) = img_path {
                deleted_images.push(path);
            }
        }

        if !to_delete_ids.is_empty() {
            let tx = conn.unchecked_transaction()?;
            for id in &to_delete_ids {
                tx.execute("DELETE FROM clips WHERE id = ?1", params![id])?;
            }
            tx.commit()?;
        }

        Ok(deleted_images)
    }

    /// Deletes ALL clips (including favorites) and returns image paths for disk deletion
    pub fn clear_all_history(&self) -> Result<Vec<String>> {
        let conn = self.writer_conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT image_path FROM clips WHERE image_path IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut deleted_images = Vec::new();
        for p in rows.flatten() {
            deleted_images.push(p);
        }

        conn.execute("DELETE FROM clips", [])?;
        Ok(deleted_images)
    }

    /// Releases unused cached pages from SQLite back to the OS allocator
    pub fn shrink_memory(&self) {
        if let Ok(conn) = self.writer_conn.lock() {
            let _ = conn.execute_batch("PRAGMA shrink_memory;");
        }
        if let Ok(conn) = self.reader_conn.lock() {
            let _ = conn.execute_batch("PRAGMA shrink_memory;");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clipped_test_{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_db_insert_and_search() {
        let dir = temp_db_dir();
        let db = Database::init(&dir).expect("init db");

        let clip1 = ClipItem {
            id: "1".into(),
            text: "Hello Rust world".into(),
            is_favorite: false,
            clip_type: "text".into(),
            image_path: None,
            image_width: None,
            image_height: None,
            ocr_text: None,
            full_text_len: 16,
        };

        let clip2 = ClipItem {
            id: "2".into(),
            text: "[Image 800x600]".into(),
            is_favorite: true,
            clip_type: "image".into(),
            image_path: Some("test.png".into()),
            image_width: Some(800),
            image_height: Some(600),
            ocr_text: Some("Invoice total $450".into()),
            full_text_len: 15,
        };

        db.insert_clip(&clip1).unwrap();
        db.insert_clip(&clip2).unwrap();

        // Test get_history
        let history = db.get_history(10, false).unwrap();
        assert_eq!(history.len(), 2);

        // Test favorites only
        let favs = db.get_history(10, true).unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].id, "2");

        // Test FTS search on text
        let results = db.search_clips("Rust", false, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");

        // Test FTS search on OCR
        let results_ocr = db.search_clips("Invoice", false, 10).unwrap();
        assert_eq!(results_ocr.len(), 1);
        assert_eq!(results_ocr[0].id, "2");

        // Test toggle favorite
        let new_fav = db.toggle_favorite("1").unwrap();
        assert!(new_fav);
        let favs_after = db.get_history(10, true).unwrap();
        assert_eq!(favs_after.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_bump_duplicate_to_top() {
        let dir = temp_db_dir();
        let db = Database::init(&dir).expect("init db");

        // 1. Copy Apples
        let item1 = db.save_or_bump_text("Apples".into(), "hash_apples").unwrap();
        assert_eq!(item1.text, "Apples");

        // Favorite Apples
        db.toggle_favorite(&item1.id).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(15));

        // 2. Copy Pears
        let item2 = db.save_or_bump_text("Pears".into(), "hash_pears").unwrap();
        assert_eq!(item2.text, "Pears");

        let history_mid = db.get_history(10, false).unwrap();
        assert_eq!(history_mid.len(), 2);
        assert_eq!(history_mid[0].text, "Pears");
        assert_eq!(history_mid[1].text, "Apples");

        std::thread::sleep(std::time::Duration::from_millis(15));

        // 3. Copy Apples again!
        let item3 = db.save_or_bump_text("Apples".into(), "hash_apples").unwrap();
        assert_eq!(item3.id, item1.id);
        assert!(item3.is_favorite); // Favorite preserved!

        // Verify history has only 2 items, and Apples is bumped to index 0!
        let history_after = db.get_history(10, false).unwrap();
        assert_eq!(history_after.len(), 2);
        assert_eq!(history_after[0].text, "Apples");
        assert_eq!(history_after[1].text, "Pears");
        assert!(history_after[0].is_favorite);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_legacy_migration() {
        let dir = temp_db_dir();
        let legacy_file = dir.join(LEGACY_HISTORY_FILE);
        let old_items = vec![
            ClipItem {
                id: "old-1".into(),
                text: "Old clip".into(),
                is_favorite: false,
                clip_type: "text".into(),
                image_path: None,
                image_width: None,
                image_height: None,
                ocr_text: None,
                full_text_len: 8,
            }
        ];
        fs::write(&legacy_file, serde_json::to_string(&old_items).unwrap()).unwrap();

        let db = Database::init(&dir).expect("init db");
        let history = db.get_history(10, false).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, "old-1");

        assert!(!legacy_file.exists());
        assert!(dir.join(format!("{}.migrated", LEGACY_HISTORY_FILE)).exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_orphan_reconciliation() {
        let dir = temp_db_dir();
        let images_dir = dir.join("images");
        fs::create_dir_all(&images_dir).unwrap();

        // Create 1 valid image clip and 1 orphan image file
        let valid_file = images_dir.join("valid.png");
        let orphan_file = images_dir.join("orphan.png");
        fs::write(&valid_file, b"valid").unwrap();
        fs::write(&orphan_file, b"orphan").unwrap();

        let db = Database::init(&dir).expect("init db");

        let clip = ClipItem {
            id: "img1".into(),
            text: "[Image]".into(),
            is_favorite: false,
            clip_type: "image".into(),
            image_path: Some(valid_file.to_string_lossy().replace('\\', "/")),
            image_width: Some(100),
            image_height: Some(100),
            ocr_text: None,
            full_text_len: 0,
        };
        db.insert_image_clip(&clip, "hash_valid").unwrap();

        // Run reconciliation
        db.reconcile_orphaned_images();

        // Valid file stays, orphan file is removed!
        assert!(valid_file.exists());
        assert!(!orphan_file.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_existing_db_schema_upgrade() {
        let dir = temp_db_dir();
        let db_path = dir.join(DB_FILE);

        // Simulate an existing database created before content_hash was added
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE clips (
                    id TEXT PRIMARY KEY,
                    text TEXT NOT NULL,
                    is_favorite INTEGER NOT NULL DEFAULT 0,
                    clip_type TEXT NOT NULL DEFAULT 'text',
                    image_path TEXT,
                    image_width INTEGER,
                    image_height INTEGER,
                    ocr_text TEXT,
                    created_at INTEGER NOT NULL
                );
                CREATE INDEX idx_clips_created_at ON clips(created_at DESC);
                CREATE INDEX idx_clips_is_fav ON clips(is_favorite);
                ",
            ).unwrap();
        }

        // Init Database - must upgrade and not panic
        let db = Database::init(&dir).expect("init existing db must succeed and upgrade schema");

        let item = db.save_or_bump_text("Migrated test".into(), "hash123").unwrap();
        assert_eq!(item.text, "Migrated test");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_delete_clip() {
        let dir = temp_db_dir();
        let db = Database::init(&dir).expect("init db");

        // 1. Insert text clip
        let item1 = db.save_or_bump_text("Delete me text".into(), "hash_del_1").unwrap();
        assert_eq!(db.get_history(10, false).unwrap().len(), 1);

        // Verify FTS search finds it
        assert_eq!(db.search_clips("Delete", false, 10).unwrap().len(), 1);

        // Delete clip
        let img_path = db.delete_clip(&item1.id).unwrap();
        assert!(img_path.is_none());

        // Assert gone from history and FTS
        assert_eq!(db.get_history(10, false).unwrap().len(), 0);
        assert_eq!(db.search_clips("Delete", false, 10).unwrap().len(), 0);

        // 2. Insert image clip
        let img_clip = ClipItem {
            id: "del_img_1".into(),
            text: "[Image]".into(),
            is_favorite: false,
            clip_type: "image".into(),
            image_path: Some("images/test_del.png".into()),
            image_width: Some(200),
            image_height: Some(200),
            ocr_text: Some("Receipt total $50".into()),
            full_text_len: 0,
        };
        db.insert_image_clip(&img_clip, "hash_del_img").unwrap();
        assert_eq!(db.search_clips("Receipt", false, 10).unwrap().len(), 1);

        // Delete image clip
        let deleted_img = db.delete_clip(&img_clip.id).unwrap();
        assert_eq!(deleted_img, Some("images/test_del.png".into()));

        // Assert gone from FTS
        assert_eq!(db.search_clips("Receipt", false, 10).unwrap().len(), 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clear_all_history() {
        let dir = temp_db_dir();
        let db = Database::init(&dir).expect("init db");

        let item1 = db.save_or_bump_text("Item 1".into(), "hash1").unwrap();
        db.toggle_favorite(&item1.id).unwrap(); // Favorite item

        let _item2 = db.save_or_bump_text("Item 2".into(), "hash2").unwrap();

        let img_clip = ClipItem {
            id: "img_clear_all".into(),
            text: "[Image]".into(),
            is_favorite: true,
            clip_type: "image".into(),
            image_path: Some("images/fav_img.png".into()),
            image_width: Some(100),
            image_height: Some(100),
            ocr_text: None,
            full_text_len: 0,
        };
        db.insert_image_clip(&img_clip, "hash_fav_img").unwrap();

        assert_eq!(db.get_history(10, false).unwrap().len(), 3);

        let deleted_imgs = db.clear_all_history().unwrap();
        assert_eq!(deleted_imgs, vec!["images/fav_img.png".to_string()]);

        // Everything cleared, including favorites
        assert_eq!(db.get_history(10, false).unwrap().len(), 0);
        assert_eq!(db.get_history(10, true).unwrap().len(), 0);
        assert_eq!(db.search_clips("Item", false, 10).unwrap().len(), 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_long_text_clip_full_fetch() {
        let dir = temp_db_dir();
        let db = Database::init(&dir).expect("init db");

        let long_text = "A".repeat(5000);
        let item = db.save_or_bump_text(long_text.clone(), "hash_long").unwrap();
        assert_eq!(item.full_text_len, 5000);

        // History gives preview capped at MAX_PREVIEW_LEN (1000)
        let history = db.get_history(10, false).unwrap();
        assert_eq!(history[0].text.len(), 1000);
        assert_eq!(history[0].full_text_len, 5000);

        // get_full_clip gives full 5000 characters
        let full = db.get_full_clip(&item.id).unwrap().expect("full clip found");
        assert_eq!(full.text.len(), 5000);
        assert_eq!(full.text, long_text);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_favorites_never_pruned_on_rotation() {
        let dir = temp_db_dir();
        let db = Database::init(&dir).expect("init db");

        // 1. Create a favorite clip
        let fav = db.save_or_bump_text("Starred secret key".into(), "hash_star").unwrap();
        db.toggle_favorite(&fav.id).unwrap();

        // 2. Insert 1,200 new non-favorite clips, exceeding max_items (999)
        let max_items = 999;
        for i in 0..1200 {
            db.save_or_bump_text(format!("Item {}", i), &format!("hash_{}", i)).unwrap();
            let _ = db.prune_history(max_items);
        }

        // 3. Verify favorite clip is STILL in the database and was NEVER deleted
        let full_fav = db.get_full_clip(&fav.id).unwrap().expect("favorite clip must exist in DB");
        assert!(full_fav.is_favorite);
        assert_eq!(full_fav.text, "Starred secret key");

        // 4. Verify favorite is returned when viewing favorites
        let favs = db.get_history(max_items, true).unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].id, fav.id);

        // 5. Verify search finds it immediately
        let found = db.search_clips("Starred", false, 10).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, fav.id);

        let _ = fs::remove_dir_all(&dir);
    }
}
