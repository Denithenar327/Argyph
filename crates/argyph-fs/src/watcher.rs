use std::sync::{mpsc, Mutex};

use camino::Utf8Path;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Debug, Clone)]
pub struct ChangedPath {
    pub path: camino::Utf8PathBuf,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
}

/// Filesystem watcher that detects file changes and provides them in batches.
pub struct FsWatcher {
    _watcher: RecommendedWatcher,
    rx: Mutex<mpsc::Receiver<notify::Result<Event>>>,
    root: camino::Utf8PathBuf,
}

impl FsWatcher {
    pub fn new(root: &Utf8Path) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .map_err(std::io::Error::other)?;

        watcher
            .watch(root.as_std_path(), RecursiveMode::Recursive)
            .map_err(std::io::Error::other)?;

        Ok(Self {
            _watcher: watcher,
            rx: Mutex::new(rx),
            root: root.to_path_buf(),
        })
    }

    /// Drain all pending events into a deduplicated batch of changed paths.
    #[allow(dead_code)]
    #[allow(clippy::expect_used)]
    pub fn next_batch(&self) -> Vec<ChangedPath> {
        let mut changes: Vec<ChangedPath> = Vec::new();
        let rx = self.rx.lock().expect("watcher mutex poisoned");
        while let Ok(Ok(event)) = rx.try_recv() {
            for path in &event.paths {
                let kind = match event.kind {
                    notify::EventKind::Create(_) => ChangeKind::Created,
                    notify::EventKind::Modify(_) => ChangeKind::Modified,
                    notify::EventKind::Remove(_) => ChangeKind::Removed,
                    _ => continue,
                };
                if let Ok(relative) = path.strip_prefix(self.root.as_std_path()) {
                    if let Ok(utf8) = camino::Utf8PathBuf::from_path_buf(relative.to_path_buf()) {
                        changes.push(ChangedPath { path: utf8, kind });
                    }
                }
            }
        }
        changes.dedup_by(|a, b| a.path == b.path);
        changes
    }

    /// Shut down the watcher. Further calls to `next_batch` will return empty.
    pub fn shutdown(self) {
        drop(self._watcher);
    }
}
