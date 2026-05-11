use std::sync::Arc;
use std::time::{Duration, Instant};

use argyph_fs::FileWatcher;
use argyph_store::Store;
use camino::Utf8PathBuf;
use tokio::sync::RwLock;

use crate::tiers::{self, TierState};

const MAX_EVENTS_PER_MINUTE: usize = 60;
const WINDOW_SECS: u64 = 60;
const POLL_INTERVAL_SECS: u64 = 5;

pub struct WatcherOrchestrator {
    root: Utf8PathBuf,
    watcher: RwLock<FileWatcher>,
    store: Arc<dyn Store>,
    _tier_state: Arc<RwLock<TierState>>,
    event_times: RwLock<Vec<Instant>>,
}

impl WatcherOrchestrator {
    pub fn new(
        root: Utf8PathBuf,
        watcher: FileWatcher,
        store: Arc<dyn Store>,
        #[allow(dead_code)] tier_state: Arc<RwLock<TierState>>,
    ) -> Self {
        Self {
            root,
            watcher: RwLock::new(watcher),
            store,
            _tier_state: tier_state,
            event_times: RwLock::new(Vec::new()),
        }
    }

    pub async fn run(&self) {
        loop {
            let batch = {
                let mut w = self.watcher.write().await;
                w.next_batch().await
            };

            if batch.is_empty() {
                continue;
            }

            tracing::info!(count = batch.len(), "watcher batch received");

            if self.rate_limited().await {
                tracing::warn!(
                    limit = MAX_EVENTS_PER_MINUTE,
                    window_s = WINDOW_SECS,
                    "reindex rate cap exceeded, falling back to polling"
                );
                self.switch_to_polling().await;
                continue;
            }

            match tiers::incremental_reindex(&self.root, &*self.store, &batch).await {
                Ok(()) => {
                    tracing::info!(affected = batch.len(), "incremental reindex complete");
                }
                Err(e) => {
                    tracing::error!(error = %e, "incremental reindex failed");
                }
            }
        }
    }

    async fn rate_limited(&self) -> bool {
        let now = Instant::now();
        let mut times = self.event_times.write().await;
        times.push(now);
        times.retain(|t| now.duration_since(*t).as_secs() < WINDOW_SECS);
        times.len() > MAX_EVENTS_PER_MINUTE
    }

    async fn switch_to_polling(&self) {
        let root_buf = self.root.to_path_buf();
        let poller = FileWatcher::poll_watcher(root_buf, Duration::from_secs(POLL_INTERVAL_SECS));
        let mut w = self.watcher.write().await;
        *w = poller;
    }
}
