use crate::*;
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::HashMap;
use std::time::SystemTime;

pub struct TestContext {
    pub files: HashMap<Utf8PathBuf, (String, usize)>,
    pub mtimes: HashMap<Utf8PathBuf, SystemTime>,
    pub in_edges: HashMap<Utf8PathBuf, usize>,
}

impl TestContext {
    pub fn with_content_files(files: HashMap<Utf8PathBuf, String>) -> Self {
        let file_map = files.into_iter().map(|(k, v)| (k, (v, 0))).collect();
        Self {
            files: file_map,
            mtimes: HashMap::new(),
            in_edges: HashMap::new(),
        }
    }
}

impl PackContext for TestContext {
    fn list_files(&self, _scope: &PackScope) -> Vec<Utf8PathBuf> {
        let mut paths: Vec<_> = self.files.keys().cloned().collect();
        paths.sort();
        paths
    }

    fn read(&self, file: &Utf8Path) -> Result<String> {
        self.files
            .get(file)
            .map(|(content, _)| content.clone())
            .ok_or_else(|| PackError::Io(format!("file not found: {file}")))
    }

    fn modified(&self, file: &Utf8Path) -> Option<SystemTime> {
        self.mtimes.get(file).copied()
    }

    fn in_edges(&self, file: &Utf8Path) -> Result<usize> {
        Ok(self.in_edges.get(file).copied().unwrap_or(0))
    }
}

pub fn path(s: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(s)
}
