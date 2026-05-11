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
    /// Symbol index complete — `find_definition`, `find_references`, graph queries.
    Tier1 {
        ready_at: SystemTime,
        symbol_count: u64,
    },
    /// Embedding index complete — `search_semantic` at full coverage.
    Tier2 {
        ready_at: SystemTime,
        embedded_count: u64,
    },
}

impl fmt::Display for TierState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline => write!(f, "offline"),
            Self::Tier0 { .. } => write!(f, "tier0"),
            Self::Tier1 { .. } => write!(f, "tier1"),
            Self::Tier2 { .. } => write!(f, "tier2"),
        }
    }
}

impl TierState {
    /// Whether Tier 0 (or higher) is available.
    pub fn is_ready(&self) -> bool {
        matches!(
            self,
            Self::Tier0 { .. } | Self::Tier1 { .. } | Self::Tier2 { .. }
        )
    }

    /// Minimum tier reached. Higher-numbered tiers imply lower ones.
    pub fn tier_number(&self) -> u8 {
        match self {
            Self::Offline => 0,
            Self::Tier0 { .. } => 1,
            Self::Tier1 { .. } => 2,
            Self::Tier2 { .. } => 3,
        }
    }

    /// Number of indexed symbols (0 if not yet at Tier 1).
    #[must_use]
    pub fn symbol_count(&self) -> u64 {
        match self {
            Self::Tier1 { symbol_count, .. } => *symbol_count,
            _ => 0,
        }
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
        assert_eq!(
            TierState::Tier1 {
                ready_at: SystemTime::UNIX_EPOCH,
                symbol_count: 100
            }
            .to_string(),
            "tier1"
        );
        assert_eq!(
            TierState::Tier2 {
                ready_at: SystemTime::UNIX_EPOCH,
                embedded_count: 50
            }
            .to_string(),
            "tier2"
        );
    }

    #[test]
    fn tier_state_is_ready() {
        assert!(!TierState::Offline.is_ready());
        assert!(TierState::Tier0 {
            ready_at: SystemTime::now()
        }
        .is_ready());
        assert!(TierState::Tier1 {
            ready_at: SystemTime::now(),
            symbol_count: 1
        }
        .is_ready());
        assert!(TierState::Tier2 {
            ready_at: SystemTime::now(),
            embedded_count: 1
        }
        .is_ready());
    }

    #[test]
    fn tier_number_progression() {
        assert_eq!(TierState::Offline.tier_number(), 0);
        assert_eq!(
            TierState::Tier0 {
                ready_at: SystemTime::now()
            }
            .tier_number(),
            1
        );
        assert_eq!(
            TierState::Tier1 {
                ready_at: SystemTime::now(),
                symbol_count: 0
            }
            .tier_number(),
            2
        );
        assert_eq!(
            TierState::Tier2 {
                ready_at: SystemTime::now(),
                embedded_count: 0
            }
            .tier_number(),
            3
        );
    }
}
