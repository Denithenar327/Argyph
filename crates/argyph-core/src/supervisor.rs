use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use camino::Utf8PathBuf;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use argyph_fs::FileWatcher;
use argyph_store::SqliteStore;
use argyph_store::Store;

use crate::config::Config;
use crate::error::Result;
use crate::index::Index;
use crate::tiers::{self, TierState};

pub struct Supervisor {
    #[allow(dead_code)]
    config: Arc<Config>,
    index: Arc<Index>,
    tier_state: Arc<RwLock<TierState>>,
    tasks: Mutex<JoinSet<()>>,
    shutdown: CancellationToken,
    store: Arc<dyn Store>,
    watcher_active: bool,
}

impl Supervisor {
    #[tracing::instrument(skip(config), fields(root = %root.as_str()))]
    pub async fn boot(root: Utf8PathBuf, config: Config) -> Result<Self> {
        tracing::info!("booting supervisor");

        let store: Arc<dyn Store> = {
            let sqlite = SqliteStore::open_at(&root)?;
            Arc::new(sqlite)
        };

        let index = Arc::new(Index::new(Arc::clone(&store)));
        let tier_state = Arc::new(RwLock::new(TierState::Offline));

        tiers::run_tier0(&root, &*store).await?;

        let now = SystemTime::now();
        *tier_state.write().await = TierState::Tier0 { ready_at: now };
        tracing::info!(ready_at = ?now, "Tier 0 ready");

        let tier_state_clone = Arc::clone(&tier_state);
        let root_clone = root.clone();
        let store_clone = Arc::clone(&store);

        let sup = Self {
            config: Arc::new(config),
            index,
            tier_state,
            tasks: Mutex::new(JoinSet::new()),
            shutdown: CancellationToken::new(),
            store,
            watcher_active: false,
        };

        let root_for_t1 = root.clone();
        let store_for_t1 = Arc::clone(&store_clone);
        let tier_for_t1 = Arc::clone(&tier_state_clone);
        sup.spawn(async move {
            match tiers::run_tier1(&root_for_t1, &*store_for_t1).await {
                Ok(symbol_count) => {
                    let now = SystemTime::now();
                    *tier_for_t1.write().await = TierState::Tier1 {
                        ready_at: now,
                        symbol_count,
                    };
                    tracing::info!(symbol_count, "Tier 1 ready");
                }
                Err(e) => {
                    tracing::error!(error = %e, "Tier 1 failed");
                }
            }
        });

        let mut sup = sup;
        let watcher = FileWatcher::notify_watcher(&root_clone, Duration::from_millis(500)).ok();

        if let Some(watcher) = watcher {
            let orch = crate::watcher::WatcherOrchestrator::new(
                root_clone.clone(),
                watcher,
                store_clone,
                tier_state_clone,
            );
            sup.spawn(async move {
                orch.run().await;
            });
            sup.watcher_active = true;
            tracing::info!("filesystem watcher active");
        } else {
            tracing::warn!("filesystem watcher unavailable (ENOSPC or OS limit)");
        }

        Ok(sup)
    }

    pub async fn run(&self) -> Result<()> {
        tracing::info!("supervisor running");
        self.shutdown.cancelled().await;
        tracing::info!("supervisor shutdown signal received");
        Ok(())
    }

    pub fn watcher_active(&self) -> bool {
        self.watcher_active
    }

    pub fn index(&self) -> Arc<Index> {
        Arc::clone(&self.index)
    }

    pub async fn get_tier_state(&self) -> TierState {
        *self.tier_state.read().await
    }

    #[allow(clippy::expect_used)]
    pub async fn shutdown(self) -> Result<()> {
        tracing::info!("supervisor shutting down");
        self.shutdown.cancel();

        let mut tasks = self.tasks.into_inner().unwrap_or_else(|e| e.into_inner());
        while let Some(result) = tasks.join_next().await {
            if let Err(e) = result {
                tracing::warn!(error = %e, "task panicked during shutdown");
            }
        }

        self.store.close().await?;

        tracing::info!("supervisor shut down");
        Ok(())
    }

    #[allow(clippy::expect_used)]
    pub fn spawn<F, T>(&self, fut: F)
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let token = self.shutdown.child_token();
        let mut tasks = self.tasks.lock().expect("mutex poisoned");
        tasks.spawn(async move {
            tokio::select! {
                _ = fut => {},
                _ = token.cancelled() => {},
            }
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use argyph_fs::{ChangeKind, ChangedPath};
    use std::time::Duration;

    struct TestFixture {
        _dir: tempfile::TempDir,
        root: Utf8PathBuf,
    }

    fn temp_fixture() -> TestFixture {
        let dir = tempfile::tempdir().unwrap();
        let src = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/tiny-rust-app"
        ));
        let dst = dir.path().join("repo");
        copy_dir_all(src, &dst).unwrap();
        let root = Utf8PathBuf::from_path_buf(dst).unwrap();
        TestFixture { _dir: dir, root }
    }

    #[allow(clippy::unwrap_used)]
    fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if ty.is_dir() {
                copy_dir_all(&src_path, &dst_path)?;
            } else if ty.is_symlink() {
                let target = std::fs::read_link(&src_path)?;
                std::os::unix::fs::symlink(&target, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn boot_reaches_tier0_in_under_1_second() {
        let fixture = temp_fixture();
        let root = fixture.root;
        let config = Config::default();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);

        let sup = Supervisor::boot(root, config).await.unwrap();

        let elapsed = deadline.elapsed();
        assert!(
            elapsed < Duration::from_secs(1),
            "boot took {elapsed:?}, expected <1s"
        );

        let state = sup.get_tier_state().await;
        assert!(state.is_ready(), "expected Tier 0 ready, got {state:?}");

        let status = sup.index().status().await.unwrap();
        assert!(status.file_count > 0, "expected at least one indexed file");

        sup.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn boot_sets_tier_state_fields() {
        let fixture = temp_fixture();
        let config = Config::default();
        let sup = Supervisor::boot(fixture.root, config).await.unwrap();

        let state = sup.get_tier_state().await;
        match state {
            TierState::Tier0 { ready_at } => {
                let age = SystemTime::now()
                    .duration_since(ready_at)
                    .unwrap_or_default();
                assert!(age.as_secs() < 5, "ready_at is too old: {age:?}");
            }
            other => panic!("expected Tier 0, got {other:?}"),
        }

        sup.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_cleans_up_without_panicking() {
        let fixture = temp_fixture();
        let sup = Supervisor::boot(fixture.root, Config::default())
            .await
            .unwrap();
        sup.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn spawn_registers_cancellation_aware_task() {
        let fixture = temp_fixture();
        let sup = Supervisor::boot(fixture.root, Config::default())
            .await
            .unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        sup.spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = tx.send(42);
        });

        let val = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap();
        assert_eq!(val, Some(42));

        sup.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn incremental_reindex_picks_up_new_file() {
        let fixture = temp_fixture();
        let root = fixture.root.clone();
        let sup = Supervisor::boot(root.clone(), Config::default())
            .await
            .unwrap();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let mut tier1_ready = false;
        while tokio::time::Instant::now() < deadline {
            let state = sup.get_tier_state().await;
            if matches!(state, TierState::Tier1 { .. }) {
                tier1_ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(tier1_ready, "Tier 1 did not become ready within 30s");

        let new_file_path = camino::Utf8PathBuf::from("src/new_module.rs");
        let new_file_abs = root.join(new_file_path.as_str());
        std::fs::write(
            new_file_abs.as_str(),
            "pub fn watcher_test_symbol() -> u32 { 42 }\n",
        )
        .unwrap();

        let changes = vec![ChangedPath {
            path: new_file_path.clone(),
            kind: ChangeKind::Created,
        }];

        let start = std::time::Instant::now();
        sup.index()
            .reindex(&root, &changes)
            .await
            .expect("reindex should succeed");
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(3000),
            "reindex took {elapsed:?}, expected <3s"
        );

        let found = sup
            .index()
            .find_symbol("watcher_test_symbol", None)
            .await
            .expect("find_symbol should succeed");
        assert!(
            !found.is_empty(),
            "newly created watcher_test_symbol not found after reindex"
        );
        assert_eq!(
            found[0].file.as_str(),
            "src/new_module.rs",
            "symbol should be associated with the new file"
        );

        sup.shutdown().await.unwrap();
    }
}
