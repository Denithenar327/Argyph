use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use camino::{Utf8Path, Utf8PathBuf};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Debug, Clone)]
pub struct ChangedPath {
    pub path: Utf8PathBuf,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
}

pub enum FileWatcher {
    Notify(FsWatcher),
    Poll(PollingWatcher),
}

impl FileWatcher {
    pub fn notify_watcher(root: &Utf8Path, debounce: Duration) -> std::io::Result<Self> {
        if std::env::var("ARGYPH_WATCHER").as_deref() == Ok("poll") {
            tracing::warn!("ARGYPH_WATCHER=poll set — falling back to polling watcher");
            return Ok(Self::poll_watcher(
                root.to_path_buf(),
                Duration::from_secs(5),
            ));
        }
        match FsWatcher::new(root, debounce) {
            Ok(w) => Ok(Self::Notify(w)),
            Err(e) => {
                tracing::warn!(error = %e, "native watcher unavailable (ENOSPC or OS limit), falling back to polling");
                Ok(Self::poll_watcher(
                    root.to_path_buf(),
                    Duration::from_secs(5),
                ))
            }
        }
    }

    pub fn poll_watcher(root: Utf8PathBuf, interval: Duration) -> Self {
        Self::Poll(PollingWatcher::new(root, interval))
    }

    pub async fn next_batch(&mut self) -> Vec<ChangedPath> {
        match self {
            Self::Notify(w) => w.next_batch().await,
            Self::Poll(w) => w.next_batch().await,
        }
    }

    pub fn shutdown(self) {
        match self {
            Self::Notify(w) => w.shutdown(),
            Self::Poll(w) => w.shutdown(),
        }
    }
}

pub struct FsWatcher {
    _watcher: RecommendedWatcher,
    rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<notify::Result<Event>>>,
    root: Utf8PathBuf,
    debounce: Duration,
}

impl FsWatcher {
    pub fn new(root: &Utf8Path, debounce: Duration) -> std::io::Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .map_err(std::io::Error::other)?;

        watcher
            .watch(root.as_std_path(), RecursiveMode::Recursive)
            .map_err(std::io::Error::other)?;

        Ok(Self {
            _watcher: watcher,
            rx: tokio::sync::Mutex::new(rx),
            root: root.to_path_buf(),
            debounce,
        })
    }

    pub async fn next_batch(&self) -> Vec<ChangedPath> {
        let mut changes: Vec<ChangedPath> = Vec::new();

        {
            let mut rx = self.rx.lock().await;
            match rx.recv().await {
                Some(Ok(event)) => {
                    collect_changes(&event, &self.root, &mut changes);
                }
                _ => return changes,
            }
        }

        loop {
            tokio::time::sleep(self.debounce).await;

            let mut has_new = false;
            {
                let mut rx = self.rx.lock().await;
                while let Ok(Ok(event)) = rx.try_recv() {
                    has_new = true;
                    collect_changes(&event, &self.root, &mut changes);
                }
            }

            if !has_new {
                break;
            }
            changes.clear();
            {
                let mut rx = self.rx.lock().await;
                while let Ok(Ok(event)) = rx.try_recv() {
                    collect_changes(&event, &self.root, &mut changes);
                }
            }
        }

        changes.dedup_by(|a, b| a.path == b.path);
        changes
    }

    pub fn shutdown(self) {
        drop(self._watcher);
    }
}

pub struct PollingWatcher {
    root: Utf8PathBuf,
    interval: Duration,
    state: Mutex<HashMap<Utf8PathBuf, (SystemTime, u64)>>,
}

impl PollingWatcher {
    pub fn new(root: Utf8PathBuf, interval: Duration) -> Self {
        Self {
            root,
            interval,
            state: Mutex::new(HashMap::new()),
        }
    }

    #[allow(clippy::expect_used)]
    pub async fn next_batch(&mut self) -> Vec<ChangedPath> {
        tokio::time::sleep(self.interval).await;

        let mut changes = Vec::new();
        let mut current = HashMap::new();

        let walker = ignore::WalkBuilder::new(self.root.as_std_path())
            .standard_filters(true)
            .build();

        for entry in walker.flatten() {
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let Ok(relative) = entry.path().strip_prefix(self.root.as_std_path()) else {
                continue;
            };
            let Ok(path) = Utf8PathBuf::from_path_buf(relative.to_path_buf()) else {
                continue;
            };
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let size = meta.len();

            current.insert(path.clone(), (modified, size));

            let prev = self
                .state
                .lock()
                .expect("mutex poisoned")
                .get(&path)
                .copied();

            match prev {
                None => {
                    changes.push(ChangedPath {
                        path,
                        kind: ChangeKind::Created,
                    });
                }
                Some((prev_mtime, prev_size)) if prev_mtime != modified || prev_size != size => {
                    changes.push(ChangedPath {
                        path,
                        kind: ChangeKind::Modified,
                    });
                }
                Some(_) => {}
            }
        }

        let mut state = self.state.lock().expect("mutex poisoned");
        let removed: Vec<Utf8PathBuf> = state
            .keys()
            .filter(|k| !current.contains_key(*k))
            .cloned()
            .collect();
        for path in removed {
            changes.push(ChangedPath {
                path,
                kind: ChangeKind::Removed,
            });
        }

        *state = current;
        changes
    }

    pub fn shutdown(self) {}
}

fn collect_changes(event: &Event, root: &Utf8Path, out: &mut Vec<ChangedPath>) {
    for path in &event.paths {
        let kind = match event.kind {
            notify::EventKind::Create(_) => ChangeKind::Created,
            notify::EventKind::Modify(_) => ChangeKind::Modified,
            notify::EventKind::Remove(_) => ChangeKind::Removed,
            _ => continue,
        };
        if let Ok(relative) = path.strip_prefix(root.as_std_path()) {
            if let Ok(utf8) = Utf8PathBuf::from_path_buf(relative.to_path_buf()) {
                out.push(ChangedPath { path: utf8, kind });
            }
        }
    }
}
