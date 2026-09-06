use rusqlite::{params, Connection, Result};
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
    conn: Mutex<Connection>,
    data_dir: PathBuf,
}

impl Database {
    pub fn init(data_dir: &Path) -> Result<Self> {
        let db_path = data_dir.join(DB_FILE);
        let conn = Connection::open(db_path)?;

        // Configure SQLite for high performance and concurrency
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "cache_size", -64000)?; // 64MB cache

        // Create main clips table
        conn.execute_batch(
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
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_clips_created_at ON clips(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_clips_is_fav ON clips(is_favorite);

            CREATE VIRTUAL TABLE IF NOT EXISTS clips_fts USING fts5(
                id UNINDEXED,
                text,
                ocr_text,
                tokenize='unicode61'
            );
            ",
        )?;

        let db = Database {
            conn: Mutex::new(conn),
            data_dir: data_dir.to_path_buf(),
        };

        // Migrate legacy clipboard_history.json if present
        db.migrate_legacy_history();

        Ok(db)
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
            let conn = self.conn.lock().unwrap();
            let tx = conn.unchecked_transaction();
            if let Ok(tx) = tx {
                let base_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64;

                let count = items.len();
                for (idx, item) in items.into_iter().enumerate() {
                    // Give oldest items smaller timestamp, newer items larger
                    let timestamp = base_time - ((count - idx) as i64 * 10);
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO clips (id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
                    let _ = tx.execute(
                        "INSERT OR IGNORE INTO clips_fts (id, text, ocr_text) VALUES (?1, ?2, ?3)",
                        params![item.id, item.text, item.ocr_text.unwrap_or_default()],
                    );
                }
                let _ = tx.commit();
            }
        }

        // Rename legacy file to avoid migrating again
        let migrated_path = self.data_dir.join(format!("{}.migrated", LEGACY_HISTORY_FILE));
        let _ = fs::rename(&legacy_path, &migrated_path);
    }

    pub fn insert_clip(&self, item: &ClipItem) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let tx = conn.unchecked_transaction()?;

        tx.execute(
            "INSERT OR REPLACE INTO clips (id, text, is_favorite, clip_type, image_path, image_width, image_height, ocr_text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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

        // Keep FTS table in sync
        tx.execute("DELETE FROM clips_fts WHERE id = ?1", params![item.id])?;
        tx.execute(
            "INSERT INTO clips_fts (id, text, ocr_text) VALUES (?1, ?2, ?3)",
            params![item.id, item.text, item.ocr_text.as_deref().unwrap_or("")],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn get_latest_item(&self) -> Result<Option<ClipItem>> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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

        let conn = self.conn.lock().unwrap();

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
            let fts_sql = if favorites_only {
                "SELECT c.id, c.text, c.is_favorite, c.clip_type, c.image_path, c.image_width, c.image_height, c.ocr_text
                 FROM clips c
                 JOIN clips_fts f ON c.id = f.id
                 WHERE clips_fts MATCH ?1 AND c.is_favorite = 1
                 ORDER BY bm25(clips_fts), c.created_at DESC
                 LIMIT ?2"
            } else {
                "SELECT c.id, c.text, c.is_favorite, c.clip_type, c.image_path, c.image_width, c.image_height, c.ocr_text
                 FROM clips c
                 JOIN clips_fts f ON c.id = f.id
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
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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

    pub fn prune_history(&self, max_items: usize) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
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
                tx.execute("DELETE FROM clips_fts WHERE id = ?1", params![id])?;
            }
            tx.commit()?;
        }

        Ok(deleted_images)
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

        // Test cleanup
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

        // Legacy file should have been renamed
        assert!(!legacy_file.exists());
        assert!(dir.join(format!("{}.migrated", LEGACY_HISTORY_FILE)).exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
