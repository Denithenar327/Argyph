use std::fmt;
use std::time::SystemTime;

use argyph_fs::{FileEntry, IgnoreWalker, Walker};
use argyph_store::Store;

use crate::error::Result;

/// Progressive indexing state — each tier is independently useful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierState {
    /// No index exists yet; supervisor is uninitialized.
    Offline,
    /// File-level index complete — `search_text`, `pack_repo`, `read_file_range`.
    Tier0 { ready_at: SystemTime },
}

impl fmt::Display for TierState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline => write!(f, "offline"),
            Self::Tier0 { .. } => write!(f, "tier0"),
        }
    }
}

impl TierState {
    /// Whether Tier 0 (or higher) is available.
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Tier0 { .. })
    }
}

/// Run Tier 0 indexing: walk the repo and upsert file metadata into the store.
///
/// Returns the entries that were upserted so the caller can feed them into
/// Tier 1 later. Instrumented with `tracing`.
#[tracing::instrument(skip(store), fields(root = %root.as_str()))]
pub async fn run_tier0(root: &camino::Utf8Path, store: &dyn Store) -> Result<Vec<FileEntry>> {
    tracing::info!("starting Tier 0 walk");
    let started = std::time::Instant::now();

    let walker = IgnoreWalker::new();
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn tier_state_display() {
        assert_eq!(TierState::Offline.to_string(), "offline");
        assert_eq!(
            TierState::Tier0 {
                ready_at: SystemTime::UNIX_EPOCH
            }
            .to_string(),
            "tier0"
        );
    }

    #[test]
    fn tier_state_is_ready() {
        assert!(!TierState::Offline.is_ready());
        assert!(TierState::Tier0 {
            ready_at: SystemTime::now()
        }
        .is_ready());
    }
}
