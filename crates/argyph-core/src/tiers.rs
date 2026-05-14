use std::collections::HashSet;
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
use tokio::sync::mpsc;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierState {
    Offline,
    Tier0 { files_indexed: usize },
    Tier1 { symbols_indexed: usize },
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
            Self::Tier2 { .. } => write!(f, "tier2"),
            Self::Ready => write!(f, "ready"),
        }
    }
}

impl TierState {
    pub fn is_ready(&self) -> bool {
        matches!(
            self,
            Self::Tier0 { .. } | Self::Tier1 { .. } | Self::Tier2 { .. } | Self::Ready
        )
    }

    pub fn tier_number(&self) -> u8 {
        match self {
            Self::Offline => 0,
            Self::Tier0 { .. } => 1,
            Self::Tier1 { .. } => 2,
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
) -> Result<()> {
    let model = embedder.model_id().to_string();
    let dim = embedder.dimension();
    let batch_size = 32;

    tracing::info!(model = %model, dim, "Tier 2 embedding started");

    loop {
        let missing = store.missing_vectors(&model).await?;
        if missing.is_empty() {
            break;
        }

        let total = missing.len();
        let mut done = 0usize;

        for chunk_ids in missing.chunks(batch_size) {
            let pairs = store.get_chunk_texts(chunk_ids).await?;

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
                done += chunk_ids.len();
                let _ = progress_tx.send(Tier2Progress {
                    embedded: done,
                    total,
                });
                tokio::task::yield_now().await;
                continue;
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
            done += chunk_ids.len();

            let _ = progress_tx.send(Tier2Progress {
                embedded: done,
                total,
            });

            tokio::task::yield_now().await;
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

        let pf = match parser.parse(&entry, &source) {
            Ok(pf) => pf,
            Err(e) => {
                tracing::warn!(file = %path, error = %e, "parse failed");
                continue;
            }
        };

        store.upsert_files(&[entry]).await?;
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
