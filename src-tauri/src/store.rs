use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 一条剪贴板记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    pub id: i64,
    pub kind: String,
    pub preview: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub image_path: Option<String>,
    pub thumb_path: Option<String>,
    pub files: Option<Vec<String>>,
    pub size: i64,
    pub source_app: Option<String>,
    pub pinned: bool,
    pub favorite: bool,
    pub note: Option<String>,
    pub copies: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 从系统剪贴板读到的一次原始内容
#[derive(Debug, Default, Clone)]
pub struct Payload {
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub png: Option<Vec<u8>>,
    pub files: Option<Vec<String>>,
    pub source_app: Option<String>,
}

impl Payload {
    pub fn kind(&self) -> &'static str {
        if self.files.is_some() {
            "files"
        } else if self.png.is_some() {
            "image"
        } else {
            "text"
        }
    }

    pub fn hash(&self) -> String {
        let mut h = Sha256::new();
        match self.kind() {
            "files" => {
                h.update(b"files:");
                if let Some(f) = &self.files {
                    for p in f {
                        h.update(p.as_bytes());
                        h.update(b"\x00");
                    }
                }
            }
            "image" => {
                h.update(b"image:");
                if let Some(d) = &self.png {
                    h.update(d);
                }
            }
            _ => {
                h.update(b"text:");
                if let Some(t) = &self.text {
                    h.update(t.as_bytes());
                }
                if let Some(hh) = &self.html {
                    h.update(b"\x01html:");
                    h.update(hh.as_bytes());
                }
            }
        }
        hex::encode(h.finalize())
    }

    pub fn preview(&self) -> String {
        match self.kind() {
            "files" => {
                let f = self.files.clone().unwrap_or_default();
                let names: Vec<String> = f
                    .iter()
                    .take(3)
                    .map(|p| {
                        Path::new(p)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| p.clone())
                    })
                    .collect();
                if f.len() > 3 {
                    format!("{}  等 {} 个文件", names.join("、"), f.len())
                } else {
                    names.join("、")
                }
            }
            "image" => "图片".to_string(),
            _ => {
                let t = self.text.clone().unwrap_or_default();
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    self.html
                        .as_ref()
                        .map(|h| strip_html(h))
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                } else {
                    trimmed.chars().take(400).collect()
                }
            }
        }
    }

    pub fn size(&self) -> i64 {
        let mut n = 0i64;
        n += self.text.as_ref().map(|s| s.len() as i64).unwrap_or(0);
        n += self.html.as_ref().map(|s| s.len() as i64).unwrap_or(0);
        n += self.png.as_ref().map(|s| s.len() as i64).unwrap_or(0);
        n += self
            .files
            .as_ref()
            .map(|f| f.iter().map(|p| p.len() as i64 + 1).sum())
            .unwrap_or(0);
        n
    }
}

/// 极简 HTML 去标签，用于生成纯文本摘要
pub fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len() / 2);
    let mut in_tag = false;
    let mut skip = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<' {
            in_tag = true;
            let rest: String = chars[i..chars.len().min(i + 9)].iter().collect::<String>().to_lowercase();
            if rest.starts_with("<script") || rest.starts_with("<style") {
                skip = true;
            }
            if rest.starts_with("</script") || rest.starts_with("</style") {
                skip = false;
            }
        } else if c == '>' && in_tag {
            in_tag = false;
            if !skip {
                out.push(' ');
            }
        } else if !in_tag && !skip {
            out.push(c);
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub struct Store {
    conn: Mutex<Connection>,
    pub media_dir: PathBuf,
}

impl Store {
    pub fn open(db_path: &Path, media_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&media_dir)?;
        let conn = Connection::open(db_path).context("打开数据库失败")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_millis(2000))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS items (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                hash         TEXT NOT NULL UNIQUE,
                kind         TEXT NOT NULL,
                preview      TEXT NOT NULL DEFAULT '',
                text         TEXT,
                html         TEXT,
                rtf          TEXT,
                image_path   TEXT,
                thumb_path   TEXT,
                files        TEXT,
                size         INTEGER NOT NULL DEFAULT 0,
                source_app   TEXT,
                pinned       INTEGER NOT NULL DEFAULT 0,
                favorite     INTEGER NOT NULL DEFAULT 0,
                note         TEXT,
                copies       INTEGER NOT NULL DEFAULT 1,
                created_at   INTEGER NOT NULL,
                updated_at   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_items_updated ON items(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_items_pinned ON items(pinned DESC, updated_at DESC);
            "#,
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
            media_dir,
        })
    }

    fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipItem> {
        let files_json: Option<String> = row.get("files")?;
        Ok(ClipItem {
            id: row.get("id")?,
            kind: row.get("kind")?,
            preview: row.get("preview")?,
            text: row.get("text")?,
            html: row.get("html")?,
            rtf: row.get("rtf")?,
            image_path: row.get("image_path")?,
            thumb_path: row.get("thumb_path")?,
            files: files_json.and_then(|s| serde_json::from_str(&s).ok()),
            size: row.get("size")?,
            source_app: row.get("source_app")?,
            pinned: row.get::<_, i64>("pinned")? != 0,
            favorite: row.get::<_, i64>("favorite")? != 0,
            note: row.get("note")?,
            copies: row.get("copies")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    /// 写入一条剪贴板内容，内容相同则只更新时间与次数
    pub fn insert(&self, payload: &Payload) -> Result<ClipItem> {
        let hash = payload.hash();
        let now = chrono::Utc::now().timestamp_millis();
        let kind = payload.kind();
        let size = payload.size();

        let image_path = if let Some(png) = &payload.png {
            let base = self.media_dir.join(format!("{}.png", &hash[..24]));
            if !base.exists() {
                std::fs::write(&base, png)?;
                // 记录大小用于统计
            }
            Some(base.to_string_lossy().to_string())
        } else {
            None
        };
        let files_json = payload
            .files
            .as_ref()
            .map(|f| serde_json::to_string(f))
            .transpose()?;

        {
            let conn = self.conn.lock().unwrap();
            let existing: Option<i64> = conn
                .query_row("SELECT id FROM items WHERE hash = ?1", params![hash], |r| r.get(0))
                .optional()?;

            if let Some(id) = existing {
                conn.execute(
                    "UPDATE items SET updated_at = ?2, copies = copies + 1,
                     text = COALESCE(?3, text), html = COALESCE(?4, html),
                     rtf = COALESCE(?5, rtf), preview = ?6, kind = ?7, size = ?8
                     WHERE id = ?1",
                    params![id, now, payload.text, payload.html, payload.rtf, payload.preview(), kind, size],
                )?;
            } else {
                conn.execute(
                    "INSERT INTO items
                     (hash, kind, preview, text, html, rtf, image_path, thumb_path, files,
                      size, source_app, pinned, favorite, copies, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, 0, 1, ?12, ?12)",
                    params![
                        hash, kind, payload.preview(), payload.text, payload.html, payload.rtf,
                        image_path, None::<String>, files_json, size, payload.source_app, now
                    ],
                )?;
            }
        }

        Ok(self
            .find_by_hash(&hash)?
            .context("插入后未找到记录")?)
    }

    pub fn find_by_hash(&self, hash: &str) -> Result<Option<ClipItem>> {
        let conn = self.conn.lock().unwrap();
        let item = conn
            .query_row("SELECT * FROM items WHERE hash = ?1", params![hash], |row| {
                Self::row_to_item(row)
            })
            .optional()?;
        Ok(item)
    }

    pub fn list(&self, query: &str, kind: &str, limit: i64) -> Result<Vec<ClipItem>> {
        let conn = self.conn.lock().unwrap();
        let q = query.trim();
        let like = format!("%{}%", q);
        let mut sql = String::from("SELECT * FROM items WHERE 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if !q.is_empty() {
            sql.push_str(
                " AND (text LIKE ? OR preview LIKE ? OR note LIKE ? OR files LIKE ?)",
            );
            for _ in 0..4 {
                args.push(Box::new(like.clone()));
            }
        }
        match kind {
            "favorite" => sql.push_str(" AND favorite = 1"),
            "image" => sql.push_str(" AND kind = 'image'"),
            "files" => sql.push_str(" AND kind = 'files'"),
            "text" => sql.push_str(" AND kind = 'text'"),
            _ => {}
        }
        sql.push_str(" ORDER BY pinned DESC, updated_at DESC LIMIT ?");
        args.push(Box::new(limit));

        let mut stmt = conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(refs.as_slice(), |row| Self::row_to_item(row))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get(&self, id: i64) -> Result<Option<ClipItem>> {
        let conn = self.conn.lock().unwrap();
        let item = conn
            .query_row("SELECT * FROM items WHERE id = ?1", params![id], |row| {
                Self::row_to_item(row)
            })
            .optional()?;
        Ok(item)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let path: Option<String> = tx
            .query_row("SELECT image_path FROM items WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?
            .flatten();
        tx.execute("DELETE FROM items WHERE id = ?1", params![id])?;
        tx.commit()?;
        if let Some(p) = path {
            let _ = std::fs::remove_file(p);
        }
        Ok(())
    }

    /// 清空历史；keep_pinned 为 true 时保留置顶与收藏
    pub fn clear(&self, keep_pinned: bool) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let n = if keep_pinned {
            conn.execute("DELETE FROM items WHERE pinned = 0 AND favorite = 0", [])?
        } else {
            conn.execute("DELETE FROM items", [])?
        };
        Ok(n)
    }

    pub fn set_flag(&self, id: i64, field: &str, value: bool) -> Result<()> {
        let col = match field {
            "pinned" => "pinned",
            "favorite" => "favorite",
            _ => anyhow::bail!("非法字段"),
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            &format!("UPDATE items SET {} = ?2 WHERE id = ?1", col),
            params![id, if value { 1 } else { 0 }],
        )?;
        Ok(())
    }

    pub fn set_note(&self, id: i64, note: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE items SET note = ?2 WHERE id = ?1", params![id, note])?;
        Ok(())
    }

    pub fn update_text(&self, id: i64, text: &str) -> Result<()> {
        let mut h = Sha256::new();
        h.update(b"text:");
        h.update(text.as_bytes());
        let hash = hex::encode(h.finalize());
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE items SET text = ?2, preview = ?2, hash = ?3, size = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, text, hash, text.len() as i64, now],
        )?;
        Ok(())
    }

    /// 超出条数/天数的旧记录清理（不动置顶和收藏）
    pub fn prune(&self, keep_count: i64, keep_days: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut removed = 0;
        if keep_count > 0 {
            removed += conn.execute(
                "DELETE FROM items WHERE pinned = 0 AND favorite = 0 AND id NOT IN (
                   SELECT id FROM items WHERE pinned = 0 AND favorite = 0
                   ORDER BY updated_at DESC LIMIT ?1)",
                params![keep_count],
            )?;
        }
        if keep_days > 0 {
            let cutoff = chrono::Utc::now().timestamp_millis() - keep_days * 86_400_000;
            removed += conn.execute(
                "DELETE FROM items WHERE pinned = 0 AND favorite = 0 AND updated_at < ?1",
                params![cutoff],
            )?;
        }
        Ok(removed)
    }

    pub fn stats(&self) -> Result<(i64, i64)> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))?;
        let bytes: i64 = conn.query_row("SELECT COALESCE(SUM(size),0) FROM items", [], |r| r.get(0))?;
        Ok((count, bytes))
    }

    /// 清理没有被任何记录引用的图片文件
    pub fn gc_media(&self) -> Result<usize> {
        let referenced: std::collections::HashSet<String> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT image_path FROM items WHERE image_path IS NOT NULL")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        let mut removed = 0;
        if let Ok(entries) = std::fs::read_dir(&self.media_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file() && !referenced.contains(&p.to_string_lossy().to_string()) {
                    if std::fs::remove_file(&p).is_ok() {
                        removed += 1;
                    }
                }
            }
        }
        Ok(removed)
    }
}
