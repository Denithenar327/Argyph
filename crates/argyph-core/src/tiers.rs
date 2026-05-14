use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use argyph_embed::Embedder;
use argyph_fs::{self, ChangeKind, ChangedPath, FileEntry, Language, Walker};
use argyph_graph::builder::DefaultGraphBuilder;
use argyph_graph::GraphBuilder;
use argyph_parse::DefaultParser;
use argyph_parse::Parser;
use argyph_store::search::VectorEntry;
use argyph_store::Store;
use camino::{Utf8Path, Utf8PathBuf};
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierState {
    Offline,
    Tier0 { files_indexed: usize },
    Tier1 { symbols_indexed: usize },
    Tier1_5 { structural_files: usize },
    Tier2 { embedded: usize, total: usize },
    Ready,
}

use std::fmt;
impl fmt::Display for TierState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline => write!(f, "offline"),
            Self::Tier0 { .. } => write!(f, "tier0"),
            Self::Tier1 { .. } => write!(f, "tier1"),
            Self::Tier1_5 { .. } => write!(f, "tier1_5"),
            Self::Tier2 { .. } => write!(f, "tier2"),
            Self::Ready => write!(f, "ready"),
        }
    }
}

impl TierState {
    pub fn is_ready(&self) -> bool {
        matches!(
            self,
            Self::Tier0 { .. }
                | Self::Tier1 { .. }
                | Self::Tier1_5 { .. }
                | Self::Tier2 { .. }
                | Self::Ready
        )
    }

    pub fn tier_number(&self) -> u8 {
        match self {
            Self::Offline => 0,
            Self::Tier0 { .. } => 1,
            Self::Tier1 { .. } | Self::Tier1_5 { .. } => 2,
            Self::Tier2 { .. } | Self::Ready => 3,
        }
    }

    #[must_use]
    pub fn symbol_count(&self) -> u64 {
        match self {
            Self::Tier1 {
                symbols_indexed, ..
            } => *symbols_indexed as u64,
            _ => 0,
        }
    }
}

#[tracing::instrument(skip(store), fields(root = %root.as_str()))]
pub async fn run_tier0(root: &Utf8Path, store: &dyn Store) -> Result<Vec<FileEntry>> {
    tracing::info!("starting Tier 0 walk");
    let started = std::time::Instant::now();

    let walker = argyph_fs::IgnoreWalker::new();
    let entries: Vec<FileEntry> = walker.walk(root).collect();

    tracing::info!(
        count = entries.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "Tier 0 walk complete"
    );

    if !entries.is_empty() {
        store.upsert_files(&entries).await?;
        tracing::info!("Tier 0 upsert complete");
    }

    tracing::info!(
        total_ms = started.elapsed().as_millis() as u64,
        "Tier 0 finished"
    );

    Ok(entries)
}

#[tracing::instrument(skip(store), fields(root = %root.as_str()))]
pub async fn run_tier1(root: &Utf8Path, store: &dyn Store) -> Result<u64> {
    tracing::info!("starting Tier 1 parse");
    let started = std::time::Instant::now();

    let files = store.list_files().await?;
    let parser = DefaultParser::new();
    let builder = DefaultGraphBuilder;

    let mut parsed: Vec<(Utf8PathBuf, argyph_parse::ParsedFile)> = Vec::with_capacity(files.len());
    let mut total_symbols: u64 = 0;

    for entry in &files {
        let path = root.join(entry.path.as_str());
        let source = match std::fs::read_to_string(path.as_str()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(file = %entry.path, error = %e, "skipping unreadable file");
                continue;
            }
        };

        let pf = parser.parse(entry, &source)?;
        total_symbols += pf.symbols.len() as u64;

        if !pf.symbols.is_empty() {
            store.upsert_symbols(&pf.symbols).await?;
        }
        if !pf.chunks.is_empty() {
            store.upsert_chunks(&pf.chunks).await?;
        }
        parsed.push((entry.path.clone(), pf));
    }

    tracing::info!(
        files_parsed = parsed.len(),
        symbols = total_symbols,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "Tier 1 parse complete, building edges"
    );

    let edges = builder.build_edges(&parsed)?;
    store.upsert_edges(&edges).await?;

    tracing::info!(
        edges = edges.len(),
        total_ms = started.elapsed().as_millis() as u64,
        "Tier 1 finished"
    );

    Ok(total_symbols)
}

#[tracing::instrument(skip(store), fields(root = %root.as_str()))]
pub async fn run_tier1_5(store: &dyn Store, root: &Utf8Path, max_file_bytes: u64) -> Result<usize> {
    tracing::info!("starting Tier 1.5 structural indexing");
    let files = store.list_files().await?;

    let candidates: Vec<_> = files
        .into_iter()
        .filter(|f| f.size <= max_file_bytes && f.language.is_some())
        .collect();

    let mut count = 0usize;
    for f in &candidates {
        if reindex_structural_for_file(store, root, f).await.is_ok() {
            count += 1;
        }
    }

    tracing::info!(structural_files = count, "Tier 1.5 complete");
    Ok(count)
}

/// Re-parse and upsert structural nodes for a single (non-code) file.
/// Returns Ok(()) when records were written, an error otherwise.
/// `upsert_structural_nodes` clears existing rows for the file first,
/// so this safely handles "file changed" as well as "file added".
async fn reindex_structural_for_file(
    store: &dyn Store,
    root: &Utf8Path,
    f: &FileEntry,
) -> Result<()> {
    use argyph_parse::structural::{self, StructuralNode};
    use argyph_store::StructuralNodeRecord;

    let path = root.join(f.path.as_str());
    let source = std::fs::read_to_string(path.as_str())
        .map_err(|e| crate::error::CoreError::Other(format!("read failed: {e}")))?;

    let lang = f
        .language
        .ok_or_else(|| crate::error::CoreError::Other("no language".into()))?;
    let file_key = f.path.as_str().len() as u64;
    let nodes: Vec<StructuralNode> = match lang {
        Language::Markdown => structural::markdown::parse(file_key, &source),
        Language::Json => structural::json::parse(file_key, &source),
        Language::Yaml => structural::yaml::parse(file_key, &source),
        Language::Toml => structural::toml_parser::parse(file_key, &source),
        Language::Csv => structural::csv::parse(file_key, &source),
        _ => {
            return Err(crate::error::CoreError::Other(
                "non-structural language".into(),
            ))
        }
    };

    let file_id = store
        .get_file_id(&f.path)
        .await?
        .ok_or_else(|| crate::error::CoreError::Other("file_id missing".into()))?;

    let records: Vec<StructuralNodeRecord> = nodes
        .into_iter()
        .map(|n| StructuralNodeRecord {
            id: n.id.0 as i64,
            file_id,
            kind: format!("{:?}", n.kind),
            label: n.label,
            path_joined: n.path.join("/"),
            path: n.path,
            byte_range: (n.byte_range.0 as u32, n.byte_range.1 as u32),
            line_range: n.line_range,
            parent_id: n.parent.map(|p| p.0 as i64),
            depth: n.depth as u16,
        })
        .collect();

    // Always upsert (even when empty) so a file that no longer has any
    // structural nodes gets its stale rows cleared.
    store.upsert_structural_nodes(file_id, &records).await?;
    Ok(())
}

/// Progress update emitted during Tier 2 embedding.
#[derive(Debug, Clone)]
pub struct Tier2Progress {
    pub embedded: usize,
    pub total: usize,
}

#[tracing::instrument(skip(store, embedder, progress_tx))]
pub async fn run_tier2(
    store: Arc<dyn Store>,
    embedder: Arc<dyn Embedder>,
    progress_tx: mpsc::UnboundedSender<Tier2Progress>,
    concurrency: usize,
) -> Result<()> {
    let model = embedder.model_id().to_string();
    let dim = embedder.dimension();
    let batch_size = 32;
    let sem = Arc::new(Semaphore::new(concurrency));
    let pending = Arc::new(AtomicUsize::new(0));
    const BACKPRESSURE_THRESHOLD: usize = 10_000;

    tracing::info!(model = %model, dim, concurrency, "Tier 2 embedding started");

    loop {
        let missing = store.missing_vectors(&model).await?;
        if missing.is_empty() {
            break;
        }

        let total = missing.len();
        let done = Arc::new(AtomicUsize::new(0));
        let mut join_set: JoinSet<Result<()>> = JoinSet::new();

        for chunk_ids in missing.chunks(batch_size) {
            while pending.load(Ordering::Relaxed) > BACKPRESSURE_THRESHOLD {
                if let Some(res) = join_set.join_next().await {
                    res.map_err(|e| crate::CoreError::Embed(format!("task join error: {e}")))??;
                }
            }

            let chunk_ids = chunk_ids.to_vec();
            let n = chunk_ids.len();
            pending.fetch_add(n, Ordering::Relaxed);

            let store = Arc::clone(&store);
            let embedder = Arc::clone(&embedder);
            let progress_tx = progress_tx.clone();
            let model = model.clone();
            let sem = Arc::clone(&sem);
            let pending = Arc::clone(&pending);
            let done = Arc::clone(&done);

            join_set.spawn(async move {
                let result = async {
                    let _permit = Arc::clone(&sem)
                        .acquire_owned()
                        .await
                        .map_err(|_| crate::CoreError::Embed("semaphore closed".into()))?;

                    let pairs = store.get_chunk_texts(&chunk_ids).await?;

                    let chunk_order: Vec<&str> = chunk_ids.iter().map(|s| s.as_str()).collect();
                    let text_map: std::collections::HashMap<&str, &str> = pairs
                        .iter()
                        .map(|(id, text)| (id.as_str(), text.as_str()))
                        .collect();

                    let texts: Vec<String> = chunk_order
                        .iter()
                        .filter_map(|id| text_map.get(id).map(|t| t.to_string()))
                        .collect();

                    if texts.is_empty() {
                        return Ok(());
                    }

                    let embeddings = embedder
                        .embed(&texts)
                        .await
                        .map_err(|e| crate::CoreError::Embed(format!("{e}")))?;

                    let entries: Vec<VectorEntry> = chunk_ids
                        .iter()
                        .zip(embeddings.iter())
                        .map(|(id, vec)| VectorEntry {
                            chunk_id: id.clone(),
                            vector: vec.clone(),
                            model: model.clone(),
                            dimension: dim,
                        })
                        .collect();

                    store.upsert_vectors(&entries).await?;
                    Ok(())
                }
                .await;

                pending.fetch_sub(n, Ordering::Relaxed);
                let prev = done.fetch_add(n, Ordering::Relaxed);
                let _ = progress_tx.send(Tier2Progress {
                    embedded: prev + n,
                    total,
                });

                result
            });
        }

        while let Some(res) = join_set.join_next().await {
            res.map_err(|e| crate::CoreError::Embed(format!("task join error: {e}")))??;
        }
    }

    tracing::info!("Tier 2 embedding complete");
    Ok(())
}

#[tracing::instrument(skip(store), fields(root = %root.as_str()))]
pub async fn incremental_reindex(
    root: &Utf8Path,
    store: &dyn Store,
    changes: &[ChangedPath],
) -> Result<()> {
    let parser = DefaultParser::new();
    let builder = DefaultGraphBuilder;

    let mut changed_files: HashSet<Utf8PathBuf> = HashSet::new();
    let mut parsed: Vec<(Utf8PathBuf, argyph_parse::ParsedFile)> = Vec::new();

    for change in changes {
        let path = &change.path;

        if change.kind == ChangeKind::Removed {
            // FK cascade clears structural_nodes and symbol/chunk rows.
            store.delete_file(path).await?;
            changed_files.insert(path.clone());
            continue;
        }

        changed_files.insert(path.clone());

        let abs = root.join(path.as_str());

        let entry = match read_file_entry(root, path) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(file = %path, error = %e, "skipping changed file");
                continue;
            }
        };

        let source = match std::fs::read_to_string(abs.as_str()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(file = %path, error = %e, "skipping unreadable file");
                continue;
            }
        };

        // Persist file metadata for either branch below.
        store.upsert_files(&[entry.clone()]).await?;

        // Structural-only languages bypass the code parser and refresh Tier 1.5 nodes.
        let is_structural = matches!(
            entry.language,
            Some(Language::Markdown)
                | Some(Language::Json)
                | Some(Language::Yaml)
                | Some(Language::Toml)
                | Some(Language::Csv)
        );
        if is_structural {
            if let Err(e) = reindex_structural_for_file(store, root, &entry).await {
                tracing::warn!(file = %path, error = %e, "structural reindex failed");
            }
            continue;
        }

        let pf = match parser.parse(&entry, &source) {
            Ok(pf) => pf,
            Err(e) => {
                tracing::warn!(file = %path, error = %e, "parse failed");
                continue;
            }
        };

        if !pf.symbols.is_empty() {
            store.upsert_symbols(&pf.symbols).await?;
        }
        if !pf.chunks.is_empty() {
            store.upsert_chunks(&pf.chunks).await?;
        }
        parsed.push((path.clone(), pf));
    }

    if parsed.is_empty() && changed_files.is_empty() {
        return Ok(());
    }

    let neighbors = find_import_neighbors(store, &changed_files).await;
    let neighbor_files: HashSet<&Utf8PathBuf> = neighbors.iter().collect();

    let all_files = store.list_files().await?;
    for entry in &all_files {
        if parsed.iter().any(|(p, _)| p == &entry.path) {
            continue;
        }
        if !neighbor_files.contains(&entry.path) {
            continue;
        }

        let abs = root.join(entry.path.as_str());
        let source = match std::fs::read_to_string(abs.as_str()) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let pf = match parser.parse(entry, &source) {
            Ok(pf) => pf,
            Err(_) => continue,
        };
        parsed.push((entry.path.clone(), pf));
    }

    let edges = builder.build_edges(&parsed)?;

    let mut affected: HashSet<&Utf8PathBuf> = parsed.iter().map(|(p, _)| p).collect();
    for change in changes {
        affected.insert(&change.path);
    }

    for file_path in affected {
        store.replace_edges_for_file(file_path, &edges).await?;
    }

    Ok(())
}

fn read_file_entry(root: &Utf8Path, path: &Utf8Path) -> Result<FileEntry> {
    let abs = root.join(path.as_str());
    let meta = std::fs::metadata(abs.as_str())?;
    let size = meta.len();
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    let hash = argyph_fs::hash_file(&abs)
        .map_err(|e| crate::CoreError::Io(std::io::Error::other(e.to_string())))?;

    let ext = path.extension().unwrap_or("");
    let language = Language::from_extension(ext);

    Ok(FileEntry {
        path: path.to_path_buf(),
        hash,
        language,
        size,
        modified,
    })
}

async fn find_import_neighbors(
    store: &dyn Store,
    files: &HashSet<Utf8PathBuf>,
) -> Vec<Utf8PathBuf> {
    let mut result = HashSet::new();
    for file in files {
        if let Ok(edges) = store.get_imports(file).await {
            for e in &edges {
                if let Some((imported, _, _)) = parse_sid_prefix(e.to.as_str()) {
                    if !files.contains(&imported) {
                        result.insert(imported);
                    }
                }
                if let Some((importer, _, _)) = parse_sid_prefix(e.from.as_str()) {
                    if !files.contains(&importer) {
                        result.insert(importer);
                    }
                }
            }
        }
    }
    result.into_iter().collect()
}

fn parse_sid_prefix(id: &str) -> Option<(Utf8PathBuf, String, usize)> {
    let rest = id.rsplit_once("::")?;
    let (prefix, start_str) = rest;
    let start: usize = start_str.parse().ok()?;
    let (file, name) = prefix.rsplit_once("::")?;
    Some((Utf8PathBuf::from(file), name.to_string(), start))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn tier_state_display() {
        assert_eq!(TierState::Offline.to_string(), "offline");
        assert_eq!(TierState::Tier0 { files_indexed: 0 }.to_string(), "tier0");
        assert_eq!(
            TierState::Tier1 {
                symbols_indexed: 100
            }
            .to_string(),
            "tier1"
        );
        assert_eq!(
            TierState::Tier1_5 {
                structural_files: 5
            }
            .to_string(),
            "tier1_5"
        );
        assert_eq!(
            TierState::Tier2 {
                embedded: 25,
                total: 50
            }
            .to_string(),
            "tier2"
        );
        assert_eq!(TierState::Ready.to_string(), "ready");
    }

    #[test]
    fn tier_state_is_ready() {
        assert!(!TierState::Offline.is_ready());
        assert!(TierState::Tier0 { files_indexed: 0 }.is_ready());
        assert!(TierState::Tier1 { symbols_indexed: 1 }.is_ready());
        assert!(TierState::Tier1_5 {
            structural_files: 1
        }
        .is_ready());
        assert!(TierState::Tier2 {
            embedded: 1,
            total: 2
        }
        .is_ready());
        assert!(TierState::Ready.is_ready());
    }

    #[test]
    fn tier_number_progression() {
        assert_eq!(TierState::Offline.tier_number(), 0);
        assert_eq!(TierState::Tier0 { files_indexed: 0 }.tier_number(), 1);
        assert_eq!(TierState::Tier1 { symbols_indexed: 0 }.tier_number(), 2);
        assert_eq!(
            TierState::Tier1_5 {
                structural_files: 0
            }
            .tier_number(),
            2
        );
        assert_eq!(
            TierState::Tier2 {
                embedded: 0,
                total: 0
            }
            .tier_number(),
            3
        );
        assert_eq!(TierState::Ready.tier_number(), 3);
    }
}
