use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::SystemTime;

use camino::{Utf8Path, Utf8PathBuf};
use rusqlite::{params, Connection, Row};

use argyph_fs::{Blake3Hash, FileEntry, Language};
use argyph_graph::edge::{Confidence, Edge, EdgeKind};
use argyph_graph::graph::SymbolOutline;
use argyph_graph::selector::SymbolSelector;
use argyph_parse::types::{ByteRange, Chunk, ChunkId, ChunkKind, Symbol, SymbolId, SymbolKind};

use crate::error::Result;
use crate::migration;
use crate::Store;

pub struct SqliteStore {
    pub(crate) conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open_at(root: &Utf8Path) -> Result<Self> {
        let dir = root.join(".argyph");
        std::fs::create_dir_all(dir.as_std_path())?;

        let db_path = dir.join("meta.sqlite");
        let conn = Connection::open(db_path.as_std_path())?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        migration::run(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        migration::run(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait::async_trait]
impl Store for SqliteStore {
    #[allow(clippy::expect_used)]
    async fn upsert_files(&self, files: &[FileEntry]) -> Result<()> {
        let mut conn = self.conn.lock().expect("mutex poisoned");
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO files (path, hash, language, size, last_seen)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(path) DO UPDATE SET
                     hash     = excluded.hash,
                     language = excluded.language,
                     size     = excluded.size,
                     last_seen = datetime('now')",
            )?;

            for entry in files {
                let lang_val = entry.language.map(language_to_str);
                stmt.execute(params![
                    entry.path.as_str(),
                    entry.hash.as_bytes().as_slice(),
                    lang_val,
                    entry.size as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    #[allow(clippy::expect_used)]
    async fn get_file(&self, path: &Utf8Path) -> Result<Option<FileEntry>> {
        let conn = self.conn.lock().expect("mutex poisoned");
        let mut stmt =
            conn.prepare_cached("SELECT path, hash, language, size FROM files WHERE path = ?1")?;
        let mut rows = stmt.query_map(params![path.as_str()], |row| {
            let path_str: String = row.get(0)?;
            let hash_blob: Vec<u8> = row.get(1)?;
            let language: Option<String> = row.get(2)?;
            let size: i64 = row.get(3)?;
            Ok((path_str, hash_blob, language, size))
        })?;

        match rows.next() {
            Some(Ok((path_str, hash_blob, lang_str, size))) => {
                let path = Utf8PathBuf::from(&path_str);
                let hash = blob_to_hash(&hash_blob);
                let language = lang_str.and_then(|s| str_to_language(&s));
                Ok(Some(FileEntry {
                    path,
                    hash,
                    language,
                    size: size as u64,
                    modified: SystemTime::UNIX_EPOCH,
                }))
            }
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    #[allow(clippy::expect_used)]
    async fn list_files(&self) -> Result<Vec<FileEntry>> {
        let conn = self.conn.lock().expect("mutex poisoned");
        let mut stmt =
            conn.prepare_cached("SELECT path, hash, language, size FROM files ORDER BY path")?;
        let rows = stmt.query_map([], |row| {
            let path_str: String = row.get(0)?;
            let hash_blob: Vec<u8> = row.get(1)?;
            let language: Option<String> = row.get(2)?;
            let size: i64 = row.get(3)?;
            Ok((path_str, hash_blob, language, size))
        })?;

        let mut entries = Vec::new();
        for row in rows {
            let (path_str, hash_blob, lang_str, size) = row?;
            let path = Utf8PathBuf::from(&path_str);
            let hash = blob_to_hash(&hash_blob);
            let language = lang_str.and_then(|s| str_to_language(&s));
            entries.push(FileEntry {
                path,
                hash,
                language,
                size: size as u64,
                modified: SystemTime::UNIX_EPOCH,
            });
        }
        Ok(entries)
    }

    #[allow(clippy::expect_used)]
    async fn delete_file(&self, path: &Utf8Path) -> Result<()> {
        let conn = self.conn.lock().expect("mutex poisoned");
        conn.execute("DELETE FROM files WHERE path = ?1", params![path.as_str()])?;
        Ok(())
    }

    #[allow(clippy::expect_used)]
    async fn upsert_symbols(&self, symbols: &[Symbol]) -> Result<()> {
        let mut conn = self.conn.lock().expect("mutex poisoned");
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO symbols
                    (id, name, kind, file, range_start, range_end, signature, parent_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;

            for sym in symbols {
                stmt.execute(params![
                    sym.id.as_str(),
                    sym.name.as_str(),
                    symbol_kind_to_str(sym.kind),
                    sym.file.as_str(),
                    sym.range.start as i64,
                    sym.range.end as i64,
                    sym.signature.as_deref(),
                    sym.parent.as_ref().map(|p| p.as_str()),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    #[allow(clippy::expect_used)]
    async fn upsert_chunks(&self, chunks: &[Chunk]) -> Result<()> {
        let mut conn = self.conn.lock().expect("mutex poisoned");
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO chunks
                    (id, file, range_start, range_end, text, kind, language)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;

            for ch in chunks {
                let cid = hex_encode(ch.id.as_bytes());
                stmt.execute(params![
                    cid,
                    ch.file.as_str(),
                    ch.range.start as i64,
                    ch.range.end as i64,
                    ch.text.as_str(),
                    chunk_kind_to_str(ch.kind),
                    language_to_str(ch.language),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    #[allow(clippy::expect_used)]
    async fn upsert_edges(&self, edges: &[Edge]) -> Result<()> {
        if edges.is_empty() {
            return Ok(());
        }

        let file_prefixes: HashSet<String> = edges
            .iter()
            .filter_map(|e| e.from.as_str().split("::").next())
            .map(String::from)
            .collect();

        let mut conn = self.conn.lock().expect("mutex poisoned");
        let tx = conn.transaction()?;
        {
            let mut del_stmt =
                tx.prepare_cached("DELETE FROM edges WHERE from_id LIKE ?1 || '::%'")?;
            for prefix in &file_prefixes {
                del_stmt.execute(params![prefix.as_str()])?;
            }

            let mut ins_stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO edges (from_id, to_id, kind, confidence)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;

            for edge in edges {
                ins_stmt.execute(params![
                    edge.from.as_str(),
                    edge.to.as_str(),
                    edge_kind_to_str(edge.kind),
                    confidence_to_str(edge.confidence),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    #[allow(clippy::expect_used)]
    async fn replace_edges_for_file(&self, file: &Utf8Path, edges: &[Edge]) -> Result<()> {
        let prefix = file.as_str();
        let mut conn = self.conn.lock().expect("mutex poisoned");
        let tx = conn.transaction()?;
        {
            tx.execute(
                "DELETE FROM edges WHERE from_id LIKE ?1 || '::%' OR to_id LIKE ?1 || '::%'",
                params![prefix],
            )?;

            if !edges.is_empty() {
                let mut ins_stmt = tx.prepare_cached(
                    "INSERT OR IGNORE INTO edges (from_id, to_id, kind, confidence)
                     VALUES (?1, ?2, ?3, ?4)",
                )?;
                for edge in edges {
                    ins_stmt.execute(params![
                        edge.from.as_str(),
                        edge.to.as_str(),
                        edge_kind_to_str(edge.kind),
                        confidence_to_str(edge.confidence),
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    #[allow(clippy::expect_used)]
    async fn find_symbol(&self, name: &str, file: Option<&Utf8Path>) -> Result<Vec<Symbol>> {
        let conn = self.conn.lock().expect("mutex poisoned");

        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(f) = file {
                (
                    "SELECT id, name, kind, file, range_start, range_end, signature, parent_id
                     FROM symbols WHERE name = ?1 AND file = ?2"
                        .to_string(),
                    vec![Box::new(name.to_string()), Box::new(f.as_str().to_string())],
                )
            } else {
                (
                    "SELECT id, name, kind, file, range_start, range_end, signature, parent_id
                     FROM symbols WHERE name = ?1"
                        .to_string(),
                    vec![Box::new(name.to_string())],
                )
            };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), row_to_symbol)?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    #[allow(clippy::expect_used)]
    async fn find_references(&self, sel: &SymbolSelector) -> Result<Vec<Edge>> {
        let conn = self.conn.lock().expect("mutex poisoned");
        query_edges_incoming(&conn, sel, EdgeKind::References)
    }

    #[allow(clippy::expect_used)]
    async fn neighbors(&self, sel: &SymbolSelector, kind: EdgeKind) -> Result<Vec<Edge>> {
        let conn = self.conn.lock().expect("mutex poisoned");
        query_edges_outgoing(&conn, sel, kind)
    }

    #[allow(clippy::expect_used)]
    async fn get_callers(&self, sel: &SymbolSelector) -> Result<Vec<Edge>> {
        let conn = self.conn.lock().expect("mutex poisoned");
        query_edges_incoming(&conn, sel, EdgeKind::Calls)
    }

    #[allow(clippy::expect_used)]
    async fn get_callees(&self, sel: &SymbolSelector) -> Result<Vec<Edge>> {
        let conn = self.conn.lock().expect("mutex poisoned");
        query_edges_outgoing(&conn, sel, EdgeKind::Calls)
    }

    #[allow(clippy::expect_used)]
    async fn get_imports(&self, file: &Utf8Path) -> Result<Vec<Edge>> {
        let conn = self.conn.lock().expect("mutex poisoned");
        let prefix = format!("{}::%", file.as_str());
        let mut stmt = conn.prepare_cached(
            "SELECT from_id, to_id, kind, confidence FROM edges
             WHERE kind = 'Imports' AND from_id LIKE ?1",
        )?;
        let rows = stmt.query_map(params![prefix.as_str()], row_to_edge)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    #[allow(clippy::expect_used)]
    async fn get_symbol_outline(&self, file: &Utf8Path) -> Result<Vec<SymbolOutline>> {
        let conn = self.conn.lock().expect("mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, signature, parent_id
             FROM symbols WHERE file = ?1 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![file.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;

        let all: Vec<(String, String, String, Option<String>, Option<String>)> =
            rows.filter_map(|r| r.ok()).collect();

        let file_ids: HashSet<&str> = all.iter().map(|(id, ..)| id.as_str()).collect();

        let mut map: HashMap<String, SymbolOutline> = HashMap::new();
        for (id, name, kind, sig, _parent_id) in &all {
            map.entry(id.clone()).or_insert(SymbolOutline {
                name: name.clone(),
                kind: kind.clone(),
                signature: sig.clone(),
                children: Vec::new(),
            });
        }

        let mut root_ids: Vec<String> = Vec::new();
        for (id, _name, _kind, _sig, parent_id) in &all {
            let has_in_file_parent = parent_id
                .as_ref()
                .is_some_and(|pid| file_ids.contains(pid.as_str()));
            if !has_in_file_parent {
                root_ids.push(id.clone());
            }
        }

        for (id, _name, _kind, _sig, parent_id) in &all {
            if let Some(pid) = parent_id {
                if file_ids.contains(pid.as_str()) {
                    if let Some(child) = map.remove(id) {
                        if let Some(parent) = map.get_mut(pid) {
                            parent.children.push(child);
                        }
                    }
                }
            }
        }

        let mut roots = Vec::new();
        for root_id in &root_ids {
            if let Some(outline) = map.remove(root_id) {
                roots.push(outline);
            }
        }
        roots.extend(map.into_values());

        Ok(roots)
    }
}

// ---------------------------------------------------------------
// Symbol helper functions
// ---------------------------------------------------------------

fn symbol_kind_to_str(k: SymbolKind) -> &'static str {
    k.as_str()
}

fn symbol_kind_from_str(s: &str) -> Option<SymbolKind> {
    match s {
        "function" => Some(SymbolKind::Function),
        "method" => Some(SymbolKind::Method),
        "struct" => Some(SymbolKind::Struct),
        "enum" => Some(SymbolKind::Enum),
        "trait" => Some(SymbolKind::Trait),
        "impl" => Some(SymbolKind::Impl),
        "class" => Some(SymbolKind::Class),
        "module" => Some(SymbolKind::Module),
        "variable" => Some(SymbolKind::Variable),
        "type_alias" => Some(SymbolKind::TypeAlias),
        "constant" => Some(SymbolKind::Constant),
        "interface" => Some(SymbolKind::Interface),
        "macro" => Some(SymbolKind::Macro),
        "static" => Some(SymbolKind::Static),
        _ => None,
    }
}

fn row_to_symbol(row: &Row) -> rusqlite::Result<Symbol> {
    let name: String = row.get(1)?;
    let kind_str: String = row.get(2)?;
    let file_str: String = row.get(3)?;
    let range_start: i64 = row.get(4)?;
    let range_end: i64 = row.get(5)?;
    let signature: Option<String> = row.get(6)?;
    let parent_id: Option<String> = row.get(7)?;

    let file = Utf8PathBuf::from(&file_str);
    let range = ByteRange::new(range_start as usize, range_end as usize);
    let kind = symbol_kind_from_str(&kind_str).unwrap_or(SymbolKind::Function);
    let id = SymbolId::new(&file, &name, range.start);
    let parent = parent_id
        .as_deref()
        .and_then(parse_symbol_id)
        .map(|(f, n, s)| SymbolId::new(&f, &n, s));

    Ok(Symbol {
        id,
        name,
        kind,
        file,
        range,
        signature,
        parent,
    })
}

// ---------------------------------------------------------------
// Chunk helper functions
// ---------------------------------------------------------------

fn chunk_kind_to_str(k: ChunkKind) -> &'static str {
    match k {
        ChunkKind::FunctionBody => "FunctionBody",
        ChunkKind::TypeDef => "TypeDef",
        ChunkKind::TopLevel => "TopLevel",
        ChunkKind::Fallback => "Fallback",
    }
}

fn _chunk_kind_from_str(s: &str) -> Option<ChunkKind> {
    match s {
        "FunctionBody" => Some(ChunkKind::FunctionBody),
        "TypeDef" => Some(ChunkKind::TypeDef),
        "TopLevel" => Some(ChunkKind::TopLevel),
        "Fallback" => Some(ChunkKind::Fallback),
        _ => None,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[allow(dead_code)]
fn hex_decode(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        bytes[i] = (hi << 4) | lo;
    }
    Some(bytes)
}

#[allow(dead_code)]
fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[allow(dead_code)]
fn _row_to_chunk(row: &Row, _language: Option<Language>) -> rusqlite::Result<Chunk> {
    let id_hex: String = row.get(0)?;
    let file_str: String = row.get(1)?;
    let range_start: i64 = row.get(2)?;
    let range_end: i64 = row.get(3)?;
    let text: String = row.get(4)?;
    let kind_str: String = row.get(5)?;
    let lang_str: String = row.get(6)?;

    let file = Utf8PathBuf::from(&file_str);
    let range = ByteRange::new(range_start as usize, range_end as usize);
    let id = hex_decode(&id_hex)
        .map(ChunkId)
        .unwrap_or(ChunkId([0u8; 32]));
    let kind = _chunk_kind_from_str(&kind_str).unwrap_or(ChunkKind::TopLevel);
    let language = str_to_language(&lang_str).unwrap_or(Language::Markdown);

    Ok(Chunk {
        id,
        file,
        range,
        text,
        kind,
        language,
    })
}

// ---------------------------------------------------------------
// Edge helper functions
// ---------------------------------------------------------------

fn edge_kind_to_str(k: EdgeKind) -> &'static str {
    match k {
        EdgeKind::Defines => "Defines",
        EdgeKind::References => "References",
        EdgeKind::Calls => "Calls",
        EdgeKind::Imports => "Imports",
        EdgeKind::ImportedBy => "ImportedBy",
        EdgeKind::Implements => "Implements",
        EdgeKind::Inherits => "Inherits",
    }
}

fn edge_kind_from_str(s: &str) -> Option<EdgeKind> {
    match s {
        "Defines" => Some(EdgeKind::Defines),
        "References" => Some(EdgeKind::References),
        "Calls" => Some(EdgeKind::Calls),
        "Imports" => Some(EdgeKind::Imports),
        "ImportedBy" => Some(EdgeKind::ImportedBy),
        "Implements" => Some(EdgeKind::Implements),
        "Inherits" => Some(EdgeKind::Inherits),
        _ => None,
    }
}

fn confidence_to_str(c: Confidence) -> &'static str {
    match c {
        Confidence::Resolved => "Resolved",
        Confidence::Heuristic => "Heuristic",
        Confidence::Ambiguous => "Ambiguous",
    }
}

fn confidence_from_str(s: &str) -> Option<Confidence> {
    match s {
        "Resolved" => Some(Confidence::Resolved),
        "Heuristic" => Some(Confidence::Heuristic),
        "Ambiguous" => Some(Confidence::Ambiguous),
        _ => None,
    }
}

fn parse_symbol_id(s: &str) -> Option<(Utf8PathBuf, String, usize)> {
    let mut parts = s.split("::");
    let file = parts.next()?;
    let name = parts.next()?;
    let start_str = parts.next()?;
    let start: usize = start_str.parse().ok()?;
    Some((Utf8PathBuf::from(file), name.to_string(), start))
}

fn row_to_edge(row: &Row) -> rusqlite::Result<Edge> {
    let from_str: String = row.get(0)?;
    let to_str: String = row.get(1)?;
    let kind_str: String = row.get(2)?;
    let conf_str: String = row.get(3)?;

    let kind = edge_kind_from_str(&kind_str).unwrap_or(EdgeKind::References);
    let confidence = confidence_from_str(&conf_str).unwrap_or(Confidence::Heuristic);

    let from = parse_symbol_id(&from_str)
        .map(|(f, n, s)| SymbolId::new(&f, &n, s))
        .unwrap_or_else(|| SymbolId::new(&Utf8PathBuf::from("?"), "?", 0));

    let to = parse_symbol_id(&to_str)
        .map(|(f, n, s)| SymbolId::new(&f, &n, s))
        .unwrap_or_else(|| SymbolId::new(&Utf8PathBuf::from("?"), "?", 0));

    Ok(Edge {
        from,
        to,
        kind,
        confidence,
    })
}

// ---------------------------------------------------------------
// Selector-based edge queries
// ---------------------------------------------------------------

enum SelectorPattern {
    Exact(String),
    Prefix(String),
    Contains(String),
}

fn selector_to_pattern(sel: &SymbolSelector) -> SelectorPattern {
    match sel {
        SymbolSelector::ById(id) => SelectorPattern::Exact(id.as_str().to_string()),
        SymbolSelector::ByName { file, name } => {
            SelectorPattern::Prefix(format!("{file}::{name}::"))
        }
        SymbolSelector::Qualified(qn) => SelectorPattern::Contains(qn.clone()),
    }
}

/// Query edges where `to_id` matches the selector (incoming edges).
fn query_edges_incoming(
    conn: &Connection,
    sel: &SymbolSelector,
    kind: EdgeKind,
) -> Result<Vec<Edge>> {
    let kind_str = edge_kind_to_str(kind);
    let pattern = selector_to_pattern(sel);

    let (sql, param_str): (&str, String) = match &pattern {
        SelectorPattern::Exact(id) => (
            "SELECT from_id, to_id, kind, confidence FROM edges
             WHERE kind = ?1 AND to_id = ?2",
            id.clone(),
        ),
        SelectorPattern::Prefix(prefix) => (
            "SELECT from_id, to_id, kind, confidence FROM edges
             WHERE kind = ?1 AND to_id LIKE ?2",
            format!("{prefix}%"),
        ),
        SelectorPattern::Contains(qn) => (
            "SELECT from_id, to_id, kind, confidence FROM edges
             WHERE kind = ?1 AND to_id LIKE '%' || ?2 || '%'",
            qn.clone(),
        ),
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![kind_str, param_str.as_str()], row_to_edge)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Query edges where `from_id` matches the selector (outgoing edges).
fn query_edges_outgoing(
    conn: &Connection,
    sel: &SymbolSelector,
    kind: EdgeKind,
) -> Result<Vec<Edge>> {
    let kind_str = edge_kind_to_str(kind);
    let pattern = selector_to_pattern(sel);

    let (sql, param_str): (&str, String) = match &pattern {
        SelectorPattern::Exact(id) => (
            "SELECT from_id, to_id, kind, confidence FROM edges
             WHERE kind = ?1 AND from_id = ?2",
            id.clone(),
        ),
        SelectorPattern::Prefix(prefix) => (
            "SELECT from_id, to_id, kind, confidence FROM edges
             WHERE kind = ?1 AND from_id LIKE ?2",
            format!("{prefix}%"),
        ),
        SelectorPattern::Contains(qn) => (
            "SELECT from_id, to_id, kind, confidence FROM edges
             WHERE kind = ?1 AND from_id LIKE '%' || ?2 || '%'",
            qn.clone(),
        ),
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![kind_str, param_str.as_str()], row_to_edge)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ---------------------------------------------------------------
// General helpers
// ---------------------------------------------------------------

fn blob_to_hash(blob: &[u8]) -> Blake3Hash {
    let mut bytes = [0u8; 32];
    let len = blob.len().min(32);
    bytes[..len].copy_from_slice(&blob[..len]);
    Blake3Hash::from(bytes)
}

fn language_to_str(lang: Language) -> &'static str {
    match lang {
        Language::Rust => "Rust",
        Language::TypeScript => "TypeScript",
        Language::Python => "Python",
        Language::JavaScript => "JavaScript",
        Language::Markdown => "Markdown",
    }
}

fn str_to_language(s: &str) -> Option<Language> {
    match s {
        "Rust" => Some(Language::Rust),
        "TypeScript" => Some(Language::TypeScript),
        "Python" => Some(Language::Python),
        "JavaScript" => Some(Language::JavaScript),
        "Markdown" => Some(Language::Markdown),
        _ => None,
    }
}
