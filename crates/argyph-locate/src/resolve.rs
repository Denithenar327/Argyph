use std::path::Path;
use std::sync::Arc;

use argyph_store::{SearchFilter, Store, StructuralNodeRecord};

use crate::path::Locator;
use crate::types::{ExpandTarget, ExpandTo, Span};

type CacheKey = (i64, [u8; 32]);
type CacheMap = std::collections::HashMap<CacheKey, Vec<StructuralNodeRecord>>;
static OOB_CACHE: once_cell::sync::Lazy<std::sync::Mutex<CacheMap>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

fn compute_blake3(content: &str) -> [u8; 32] {
    *blake3::hash(content.as_bytes()).as_bytes()
}

fn record_from_node(
    file_id: i64,
    node: &argyph_parse::structural::StructuralNode,
) -> StructuralNodeRecord {
    StructuralNodeRecord {
        id: 0,
        file_id,
        kind: format!("{:?}", node.kind),
        label: node.label.clone(),
        path_joined: node.path.join("/"),
        path: node.path.clone(),
        byte_range: (node.byte_range.0 as u32, node.byte_range.1 as u32),
        line_range: node.line_range,
        parent_id: None,
        depth: node.depth as u16,
    }
}

fn parse_structural(
    language: argyph_fs::Language,
    file_id: i64,
    source: &str,
) -> Vec<StructuralNodeRecord> {
    use argyph_fs::Language;
    let nodes: Vec<argyph_parse::structural::StructuralNode> = match language {
        Language::Markdown => argyph_parse::structural::markdown::parse(file_id as u64, source),
        Language::Json => argyph_parse::structural::json::parse(file_id as u64, source),
        Language::Yaml => argyph_parse::structural::yaml::parse(file_id as u64, source),
        Language::Toml => argyph_parse::structural::toml_parser::parse(file_id as u64, source),
        Language::Csv => argyph_parse::structural::csv::parse(file_id as u64, source),
        _ => return Vec::new(),
    };
    nodes.iter().map(|n| record_from_node(file_id, n)).collect()
}

pub async fn parse_on_demand(
    _store: &Arc<dyn Store>,
    root: &Path,
    file_id: i64,
    file_path: &str,
    language: argyph_fs::Language,
    max_bytes: u32,
) -> anyhow::Result<Option<Vec<StructuralNodeRecord>>> {
    let full_path = root.join(file_path);
    let Ok(content) = std::fs::read_to_string(&full_path) else {
        return Ok(None);
    };

    if content.len() > max_bytes as usize {
        return Ok(None);
    }

    let hash = compute_blake3(&content);
    let key = (file_id, hash);

    {
        #[allow(clippy::expect_used)]
        let cache = OOB_CACHE.lock().expect("oob cache lock");
        if let Some(v) = cache.get(&key) {
            return Ok(Some(v.clone()));
        }
    }

    let records = parse_structural(language, file_id, &content);

    #[allow(clippy::expect_used)]
    OOB_CACHE
        .lock()
        .expect("oob cache lock")
        .insert(key, records.clone());

    Ok(Some(records))
}

async fn record_to_span(
    store: &Arc<dyn Store>,
    root: &Path,
    rec: &StructuralNodeRecord,
    max_bytes: u32,
    score: f32,
) -> anyhow::Result<Span> {
    let file_entry = store
        .get_file_by_id(rec.file_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("file not found for file_id={}", rec.file_id))?;

    let file_path = file_entry.path.to_string();
    let file_size = file_entry.size as u32;

    let full_path = root.join(file_entry.path.as_str());

    let file_content = std::fs::read_to_string(&full_path).unwrap_or_default();

    let current_hash = compute_blake3(&file_content);
    let indexed_hash_bytes = file_entry.hash.as_bytes();
    if current_hash != *indexed_hash_bytes {
        tracing::warn!(
            "STALE_INDEX: file {file_path} modified since indexing; results may be imprecise"
        );
    }

    let byte_start = rec.byte_range.0 as usize;

    let mut content = if byte_start < file_content.len() {
        let end = (byte_start + max_bytes as usize).min(file_content.len());
        file_content[byte_start..end].to_string()
    } else {
        String::new()
    };

    let truncated = rec.byte_range.1 - rec.byte_range.0 > max_bytes;
    if truncated && !content.is_empty() {
        let ellipsis = "\n... (truncated)";
        let usable = max_bytes as usize - ellipsis.len();
        if content.len() > usable {
            content.truncate(usable);
        }
        content.push_str(ellipsis);
    }

    let parent: Option<ExpandTarget> = if let Some(pid) = rec.parent_id {
        if let Ok(Some(parent_rec)) = store.structural_node_by_id(pid).await {
            Some(ExpandTarget {
                node_id: Some(parent_rec.id.to_string()),
                label: Some(parent_rec.label.clone()),
                bytes: parent_rec.byte_range.1 - parent_rec.byte_range.0,
            })
        } else {
            None
        }
    } else {
        None
    };

    let file = ExpandTarget {
        node_id: None,
        label: None,
        bytes: file_size,
    };

    Ok(Span {
        file: file_path,
        byte_range: rec.byte_range,
        line_range: rec.line_range,
        kind: rec.kind.clone(),
        path: rec.path.clone(),
        content,
        score,
        truncated,
        expand_to: ExpandTo {
            parent,
            file: Some(file),
        },
    })
}

pub async fn resolve_structural_path(
    store: Arc<dyn Store>,
    root: &Path,
    locator: &Locator,
    single_file: Option<i64>,
    max_bytes: u32,
) -> anyhow::Result<Vec<Span>> {
    let path_joined = match locator {
        Locator::Heading(parts) => parts.join("/"),
        Locator::FilePlusHeading { path, .. } => path.join("/"),
        Locator::Name(name) => name.clone(),
        Locator::Any => {
            return Ok(vec![]);
        }
    };

    let file_id = match locator {
        Locator::FilePlusHeading { file, .. } => {
            store.get_file_id(&camino::Utf8PathBuf::from(file)).await?
        }
        _ => single_file,
    };

    if let Some(rec) = store
        .get_structural_node_by_path(file_id, &path_joined)
        .await?
    {
        let span = record_to_span(&store, root, &rec, max_bytes, 1.0).await?;
        return Ok(vec![span]);
    }

    if let Some(fid) = file_id {
        if let Some(file_entry) = store.get_file_by_id(fid).await? {
            if let Some(lang) = file_entry.language {
                if let Some(records) = parse_on_demand(
                    &store,
                    root,
                    fid,
                    file_entry.path.as_str(),
                    lang,
                    10_485_760,
                )
                .await?
                {
                    for rec in &records {
                        if rec.path_joined == path_joined || rec.label == path_joined {
                            let span = record_to_span(&store, root, rec, max_bytes, 0.9).await?;
                            return Ok(vec![span]);
                        }
                    }
                    for rec in &records {
                        if rec.path_joined.contains(&path_joined) {
                            let span = record_to_span(&store, root, rec, max_bytes, 0.8).await?;
                            return Ok(vec![span]);
                        }
                    }
                }
            }
        }
    }

    Ok(vec![])
}

pub async fn resolve_structural_search(
    store: Arc<dyn Store>,
    root: &Path,
    query: &str,
    file_ids: Option<&[i64]>,
    max_results: usize,
    max_bytes: u32,
) -> anyhow::Result<Vec<Span>> {
    let records = store
        .fts_search_structural(query, file_ids, max_results)
        .await?;

    let mut spans = Vec::with_capacity(records.len());
    for rec in records {
        let span = record_to_span(&store, root, &rec, max_bytes, 1.0).await?;
        spans.push(span);
    }
    Ok(spans)
}

pub async fn resolve_hybrid(
    store: Arc<dyn Store>,
    root: &Path,
    embedder: Arc<dyn argyph_embed::Embedder>,
    query: &str,
    file_ids: Option<&[i64]>,
    max_results: usize,
    max_bytes: u32,
) -> anyhow::Result<Vec<Span>> {
    let filter = SearchFilter {
        file_ids: file_ids.map(|ids| ids.to_vec()),
        ..Default::default()
    };

    let query_vec = embedder.embed_query(query).await.unwrap_or_default();

    let hybrid_result = store
        .search_hybrid(query, &query_vec, max_results.max(5), &filter)
        .await?;

    let fts_records = store
        .fts_search_structural(query, file_ids, max_results)
        .await?;

    let fts_spans: Vec<SyntheticSpan> = fts_records
        .iter()
        .map(|r| SyntheticSpan {
            file: String::new(),
            byte_start: r.byte_range.0,
            byte_end: r.byte_range.1,
            score: 1.0,
            kind: "structural-node".into(),
            node_record: Some(r.clone()),
        })
        .collect();

    let mut hybrid_spans: Vec<SyntheticSpan> = Vec::with_capacity(hybrid_result.hits.len());
    for hit in &hybrid_result.hits {
        let file_id = store
            .get_file_id(&camino::Utf8PathBuf::from(&hit.file))
            .await?;

        let mid = (hit.byte_range.0 + hit.byte_range.1) / 2;

        let node_record = match file_id {
            Some(fid) => store
                .enclosing_structural_node(fid, mid)
                .await
                .unwrap_or(None),
            None => None,
        };

        if let Some(ref rec) = node_record {
            hybrid_spans.push(SyntheticSpan {
                file: hit.file.clone(),
                byte_start: hit.byte_range.0,
                byte_end: hit.byte_range.1,
                score: hit.score,
                kind: "hybrid".into(),
                node_record: Some(rec.clone()),
            });
        } else {
            let synthesized = StructuralNodeRecord {
                id: 0,
                file_id: file_id.unwrap_or(0),
                kind: "chunk".into(),
                label: hit.chunk_text.chars().take(60).collect(),
                path_joined: hit.chunk_id.clone(),
                path: vec![hit.chunk_text.chars().take(40).collect()],
                byte_range: hit.byte_range,
                line_range: hit.line_range,
                parent_id: None,
                depth: 0,
            };
            hybrid_spans.push(SyntheticSpan {
                file: hit.file.clone(),
                byte_start: hit.byte_range.0,
                byte_end: hit.byte_range.1,
                score: hit.score,
                kind: "hybrid-chunk".into(),
                node_record: Some(synthesized),
            });
        }
    }

    let mut merged: Vec<SyntheticSpan> = fts_spans;
    merged.extend(hybrid_spans);

    dedupe_spans(&mut merged);

    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    merged.truncate(max_results);

    expand_synthetic_spans(store, root, &merged, max_bytes).await
}

#[allow(clippy::too_many_arguments)]
pub async fn resolve_scoped_semantic(
    store: Arc<dyn Store>,
    root: &Path,
    embedder: Arc<dyn argyph_embed::Embedder>,
    locator: &Locator,
    query: &str,
    single_file: Option<i64>,
    max_results: usize,
    max_bytes: u32,
) -> anyhow::Result<Vec<Span>> {
    let scope_spans =
        resolve_structural_path(Arc::clone(&store), root, locator, single_file, max_bytes).await?;

    let Some(scope) = scope_spans.first() else {
        return Ok(vec![]);
    };

    let filter = SearchFilter {
        file_ids: single_file.map(|id| vec![id]),
        ..Default::default()
    };

    let query_vec = embedder.embed_query(query).await.unwrap_or_default();

    let hybrid_result = store
        .search_hybrid(query, &query_vec, max_results.max(5), &filter)
        .await?;

    let mut synthetic: Vec<SyntheticSpan> = Vec::with_capacity(hybrid_result.hits.len());
    for hit in &hybrid_result.hits {
        if hit.byte_range.0 < scope.byte_range.0 || hit.byte_range.1 > scope.byte_range.1 {
            continue;
        }

        let file_id = store
            .get_file_id(&camino::Utf8PathBuf::from(&hit.file))
            .await?;
        let mid = (hit.byte_range.0 + hit.byte_range.1) / 2;

        let node_record = match file_id {
            Some(fid) => store
                .enclosing_structural_node(fid, mid)
                .await
                .unwrap_or(None),
            None => None,
        };

        let rec = node_record.unwrap_or_else(|| StructuralNodeRecord {
            id: 0,
            file_id: file_id.unwrap_or(0),
            kind: "chunk".into(),
            label: hit.chunk_text.chars().take(60).collect(),
            path_joined: hit.chunk_id.clone(),
            path: vec![hit.chunk_text.chars().take(40).collect()],
            byte_range: hit.byte_range,
            line_range: hit.line_range,
            parent_id: None,
            depth: 0,
        });

        synthetic.push(SyntheticSpan {
            file: hit.file.clone(),
            byte_start: hit.byte_range.0,
            byte_end: hit.byte_range.1,
            score: hit.score,
            kind: "scoped-hybrid".into(),
            node_record: Some(rec),
        });
    }

    synthetic.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    synthetic.truncate(max_results);

    expand_synthetic_spans(store, root, &synthetic, max_bytes).await
}

struct SyntheticSpan {
    file: String,
    byte_start: u32,
    byte_end: u32,
    score: f32,
    #[allow(dead_code)]
    kind: String,
    node_record: Option<StructuralNodeRecord>,
}

fn dedupe_spans(spans: &mut Vec<SyntheticSpan>) {
    let mut seen = std::collections::HashSet::new();
    spans.retain(|s| {
        let key = (s.file.clone(), s.byte_start, s.byte_end);
        seen.insert(key)
    });
}

async fn expand_synthetic_spans(
    store: Arc<dyn Store>,
    root: &Path,
    spans: &[SyntheticSpan],
    max_bytes: u32,
) -> anyhow::Result<Vec<Span>> {
    let mut out = Vec::with_capacity(spans.len());
    for s in spans {
        if let Some(ref rec) = s.node_record {
            let span = record_to_span(&store, root, rec, max_bytes, s.score).await?;
            out.push(span);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn record_to_span_produces_span() {
        let tmp = tempfile::tempdir().unwrap();
        let root_path = tmp.path();
        let file_path = root_path.join("test.md");
        std::fs::write(&file_path, "# Hello\n\nThis is a test.\n").unwrap();

        let db = argyph_store::SqliteStore::open_in_memory().unwrap();

        let entry = argyph_fs::FileEntry {
            path: camino::Utf8PathBuf::from("test.md"),
            hash: argyph_fs::Blake3Hash::from([0u8; 32]),
            language: Some(argyph_fs::Language::Markdown),
            size: 27,
            modified: std::time::SystemTime::UNIX_EPOCH,
        };
        db.upsert_files(&[entry]).await.unwrap();

        let file_id = db
            .get_file_id(&camino::Utf8PathBuf::from("test.md"))
            .await
            .unwrap()
            .unwrap();

        let rec = StructuralNodeRecord {
            id: 1,
            file_id,
            kind: "MdSection".into(),
            label: "Hello".into(),
            path_joined: "Hello".into(),
            path: vec!["Hello".into()],
            byte_range: (0, 8),
            line_range: (1, 1),
            parent_id: None,
            depth: 0,
        };

        let store: Arc<dyn Store> = Arc::new(db);
        let span = record_to_span(&store, root_path, &rec, 4096, 1.0)
            .await
            .unwrap();
        assert_eq!(span.file, "test.md");
        assert_eq!(span.kind, "MdSection");
        assert!(span.content.contains("Hello"));
        assert!(!span.truncated);
    }

    #[tokio::test]
    async fn record_to_span_truncates() {
        let tmp = tempfile::tempdir().unwrap();
        let root_path = tmp.path();
        let content = "A".repeat(5000);
        let file_path = root_path.join("big.md");
        std::fs::write(&file_path, &content).unwrap();

        let db = argyph_store::SqliteStore::open_in_memory().unwrap();
        let entry = argyph_fs::FileEntry {
            path: camino::Utf8PathBuf::from("big.md"),
            hash: argyph_fs::Blake3Hash::from([0u8; 32]),
            language: Some(argyph_fs::Language::Markdown),
            size: 5000,
            modified: std::time::SystemTime::UNIX_EPOCH,
        };
        db.upsert_files(&[entry]).await.unwrap();

        let file_id = db
            .get_file_id(&camino::Utf8PathBuf::from("big.md"))
            .await
            .unwrap()
            .unwrap();

        let rec = StructuralNodeRecord {
            id: 2,
            file_id,
            kind: "MdSection".into(),
            label: "Big".into(),
            path_joined: "Big".into(),
            path: vec!["Big".into()],
            byte_range: (0, 5000),
            line_range: (1, 100),
            parent_id: None,
            depth: 0,
        };

        let store: Arc<dyn Store> = Arc::new(db);
        let span = record_to_span(&store, root_path, &rec, 100, 1.0)
            .await
            .unwrap();
        assert!(span.truncated);
        assert!(span.content.len() <= 110);
        assert!(span.content.contains("(truncated)"));
    }

    #[tokio::test]
    async fn record_to_span_with_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let root_path = tmp.path();
        let file_path = root_path.join("nested.md");
        std::fs::write(&file_path, "## Sub\n\nChild content here.\n").unwrap();

        let db = argyph_store::SqliteStore::open_in_memory().unwrap();
        let entry = argyph_fs::FileEntry {
            path: camino::Utf8PathBuf::from("nested.md"),
            hash: argyph_fs::Blake3Hash::from([0u8; 32]),
            language: Some(argyph_fs::Language::Markdown),
            size: 30,
            modified: std::time::SystemTime::UNIX_EPOCH,
        };
        db.upsert_files(&[entry]).await.unwrap();

        let file_id = db
            .get_file_id(&camino::Utf8PathBuf::from("nested.md"))
            .await
            .unwrap()
            .unwrap();

        let parent_rec = StructuralNodeRecord {
            id: 10,
            file_id,
            kind: "MdSection".into(),
            label: "Top".into(),
            path_joined: "Top".into(),
            path: vec!["Top".into()],
            byte_range: (0, 40),
            line_range: (1, 5),
            parent_id: None,
            depth: 0,
        };
        let child_rec = StructuralNodeRecord {
            id: 11,
            file_id,
            kind: "MdSection".into(),
            label: "Sub".into(),
            path_joined: "Top/Sub".into(),
            path: vec!["Top".into(), "Sub".into()],
            byte_range: (0, 30),
            line_range: (1, 3),
            parent_id: Some(10),
            depth: 1,
        };
        db.upsert_structural_nodes(file_id, &[parent_rec, child_rec])
            .await
            .unwrap();

        let store: Arc<dyn Store> = Arc::new(db);
        let got_child = store.structural_node_by_id(11).await.unwrap().unwrap();
        let span = record_to_span(&store, root_path, &got_child, 4096, 1.0)
            .await
            .unwrap();
        assert!(span.expand_to.parent.is_some());
        let p = span.expand_to.parent.unwrap();
        assert_eq!(p.node_id.as_deref(), Some("10"));
        assert_eq!(p.label.as_deref(), Some("Top"));
    }
}
