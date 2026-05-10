use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use camino::Utf8PathBuf;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use argyph_store::SqliteStore;
use argyph_store::Store;

use crate::config::Config;
use crate::error::Result;
use crate::index::Index;
use crate::tiers::{self, TierState};

/// Filesystem watcher placeholder — concrete integration lands in a later
/// milestone.
pub struct FsWatcher {
    _private: (),
}

/// The single owner of runtime state.
///
/// Boots the index, marks Tier 0 ready, and manages the background task pool.
/// All long-lived tasks must be registered via [`Supervisor::spawn`].
#[allow(dead_code)]
pub struct Supervisor {
    config: Arc<Config>,
    index: Arc<Index>,
    tier_state: Arc<RwLock<TierState>>,
    tasks: Mutex<JoinSet<()>>,
    shutdown: CancellationToken,
    #[allow(dead_code)]
    watcher: Option<FsWatcher>,
}

impl Supervisor {
    /// Boot the supervisor against the given repo root.
    ///
    /// Opens the SQLite store, runs Tier 0 (walk + upsert), and marks Tier 0
    /// ready before returning.
    #[tracing::instrument(skip(config), fields(root = %root.as_str()))]
    pub async fn boot(root: Utf8PathBuf, config: Config) -> Result<Self> {
        tracing::info!("booting supervisor");

        let store: Arc<dyn Store> = {
            let sqlite = SqliteStore::open_at(&root)?;
            Arc::new(sqlite)
        };

        let index = Arc::new(Index::new(Arc::clone(&store)));
        let tier_state = Arc::new(RwLock::new(TierState::Offline));

        // ── Tier 0 ────────────────────────────────────────────────
        tiers::run_tier0(&root, &*store).await?;

        let now = SystemTime::now();
        *tier_state.write().await = TierState::Tier0 { ready_at: now };
        tracing::info!(
            ready_at = ?now,
            "Tier 0 ready"
        );

        Ok(Self {
            config: Arc::new(config),
            index,
            tier_state,
            tasks: Mutex::new(JoinSet::new()),
            shutdown: CancellationToken::new(),
            watcher: None,
        })
    }

    /// Block until the supervisor shuts down. Useful for long-running server
    /// processes.
    pub async fn run(&self) -> Result<()> {
        tracing::info!("supervisor running");
        self.shutdown.cancelled().await;
        tracing::info!("supervisor shutdown signal received");
        Ok(())
    }

    /// Access the [`Index`] facade for domain queries.
    pub fn index(&self) -> Arc<Index> {
        Arc::clone(&self.index)
    }

    /// Get a copy of the current [`TierState`].
    pub async fn get_tier_state(&self) -> TierState {
        *self.tier_state.read().await
    }

    /// Gracefully shut down: cancel all background tasks and drain the pool.
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

        tracing::info!("supervisor shut down");
        Ok(())
    }

    /// Register a long-lived background task.
    ///
    /// The task is tied to the supervisor's [`CancellationToken`]; it is
    /// cancelled when [`Supervisor::shutdown`] is called.
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
    use std::time::Duration;

    struct TestFixture {
        _dir: tempfile::TempDir,
        root: Utf8PathBuf,
    }

    /// Copy the fixture directory into a temp dir so each test has its own
    /// SQLite database and filesystem state.
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
        let config = Config;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);

        let sup = Supervisor::boot(root, config).await.unwrap();

        let elapsed = deadline.elapsed();
        assert!(
            elapsed < Duration::from_secs(1),
            "boot took {:?}, expected <1s",
            elapsed
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
        let config = Config;
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
        let sup = Supervisor::boot(fixture.root, Config).await.unwrap();
        sup.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn spawn_registers_cancellation_aware_task() {
        let fixture = temp_fixture();
        let sup = Supervisor::boot(fixture.root, Config).await.unwrap();

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
}
