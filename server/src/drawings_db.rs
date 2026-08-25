//! 図面(自分自身がアップロードしたもの)のSQLite永続化(2026-08-25新設)。
//!
//! ユーザー指示「半導体やLSIやカスタムICや自動車や部品などの図面の
//! UPLOADとダウンロードで言葉で図面の不具合や改善や改良や…新規作成も
//! 可能にして」→著作権リスクの確認の結果「自分の図面のアップロード+
//! AI解説のみを小規模に試作」に絞ることで合意→「UPLOADや完成した図面は
//! DATABASEに保存して読み書き可能として」への対応。
//!
//! **正直な開示(最重要)**: `aruaru-llm`は画像を認識できないテキスト
//! 生成モデル(素のGPT-2、`/v1/generate`)であり、アップロードされた
//! 図面ファイル自体の画素を解析することは一切行わない。「AI解説」は
//! 利用者が併せて入力した**テキストの説明文**に基づく助言に過ぎない
//! ——この開示はAPIレスポンス・UI双方に必ず含める。
//!
//! 図面ファイル自体(画像/PDF等のバイナリ)はBLOBとしてSQLiteへ保存する
//! (他人の著作物の模倣・派生生成は一切行わない、あくまで利用者自身の
//! ファイルの保管庫としての機能に限定)。

use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct DrawingsDb {
    conn: Mutex<Connection>,
}

#[derive(serde::Serialize)]
pub struct DrawingSummary {
    pub id: i64,
    pub filename: String,
    pub category: String,
    pub description: String,
    pub created_at: String,
    pub has_analysis: bool,
}

#[derive(serde::Serialize)]
pub struct DrawingFull {
    pub id: i64,
    pub filename: String,
    pub category: String,
    pub description: String,
    pub data_base64: String,
    pub analysis: Option<String>,
    pub created_at: String,
}

impl DrawingsDb {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS drawings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                filename TEXT NOT NULL,
                category TEXT NOT NULL,
                description TEXT NOT NULL,
                data BLOB NOT NULL,
                analysis TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;
        Ok(DrawingsDb { conn: Mutex::new(conn) })
    }

    pub fn insert(&self, filename: &str, category: &str, description: &str, data: &[u8]) -> rusqlite::Result<i64> {
        let conn = self.conn.lock().expect("drawings db mutex poisoned");
        conn.execute(
            "INSERT INTO drawings (filename, category, description, data) VALUES (?1, ?2, ?3, ?4)",
            params![filename, category, description, data],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list(&self) -> rusqlite::Result<Vec<DrawingSummary>> {
        let conn = self.conn.lock().expect("drawings db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, filename, category, description, created_at, analysis IS NOT NULL FROM drawings ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DrawingSummary {
                id: row.get(0)?,
                filename: row.get(1)?,
                category: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
                has_analysis: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<DrawingFull>> {
        let conn = self.conn.lock().expect("drawings db mutex poisoned");
        let mut stmt = conn.prepare("SELECT id, filename, category, description, data, analysis, created_at FROM drawings WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let data: Vec<u8> = row.get(4)?;
            use base64::Engine;
            Ok(Some(DrawingFull {
                id: row.get(0)?,
                filename: row.get(1)?,
                category: row.get(2)?,
                description: row.get(3)?,
                data_base64: base64::engine::general_purpose::STANDARD.encode(data),
                analysis: row.get(5)?,
                created_at: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn save_analysis(&self, id: i64, analysis: &str) -> rusqlite::Result<usize> {
        let conn = self.conn.lock().expect("drawings db mutex poisoned");
        conn.execute("UPDATE drawings SET analysis = ?1 WHERE id = ?2", params![analysis, id])
    }
}

pub fn db_path(root: &Path) -> PathBuf {
    root.join("data").join("open-cg-cad.sqlite3")
}
