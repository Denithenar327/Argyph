#![forbid(unsafe_code)]

use std::collections::HashMap;

use argyph_store::{MemoryEntry, Store};

pub type Result<T> = std::result::Result<T, MemoryError>;

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("store error: {0}")]
    Store(#[from] argyph_store::StoreError),
    #[error("memory not found: {0}")]
    NotFound(String),
}

#[async_trait::async_trait]
pub trait Memory: Send + Sync {
    async fn save(
        &self,
        scope: &str,
        content: &str,
        metadata: &HashMap<String, String>,
    ) -> Result<String>;
    async fn search(&self, query: &str, scope: Option<&str>, k: usize) -> Result<Vec<MemoryEntry>>;
    async fn list(&self, scope: &str) -> Result<Vec<MemoryEntry>>;
    async fn forget(&self, id: &str) -> Result<()>;
}

pub struct SqliteMemory {
    store: std::sync::Arc<dyn Store>,
}

impl SqliteMemory {
    pub fn new(store: std::sync::Arc<dyn Store>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl Memory for SqliteMemory {
    #[allow(clippy::expect_used)]
    async fn save(
        &self,
        scope: &str,
        content: &str,
        metadata: &HashMap<String, String>,
    ) -> Result<String> {
        let id = self.store.save_memory(scope, content, metadata).await?;
        Ok(id)
    }

    async fn search(&self, query: &str, scope: Option<&str>, k: usize) -> Result<Vec<MemoryEntry>> {
        let entries = self.store.search_memories(query, scope, k).await?;
        Ok(entries)
    }

    async fn list(&self, scope: &str) -> Result<Vec<MemoryEntry>> {
        let entries = self.store.list_memories(scope).await?;
        Ok(entries)
    }

    async fn forget(&self, id: &str) -> Result<()> {
        self.store.forget_memory(id).await?;
        Ok(())
    }
}
