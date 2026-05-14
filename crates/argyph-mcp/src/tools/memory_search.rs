use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;
use argyph_store::MemoryEntry;

use crate::error::McpErrorBody;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default = "default_k")]
    pub k: usize,
}

fn default_k() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MemoryHit {
    pub id: String,
    pub scope: String,
    pub content: String,
    pub metadata: std::collections::HashMap<String, String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hits: Option<Vec<MemoryHit>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    pub fn ok(entries: Vec<MemoryEntry>) -> Self {
        Self {
            hits: Some(
                entries
                    .into_iter()
                    .map(|e| MemoryHit {
                        id: e.id,
                        scope: e.scope,
                        content: e.content,
                        metadata: e.metadata,
                        created_at: e.created_at,
                    })
                    .collect(),
            ),
            error: None,
        }
    }

    pub fn err(body: McpErrorBody) -> Self {
        Self {
            hits: None,
            error: Some(body),
        }
    }
}

pub async fn handle(
    supervisor: &Arc<Supervisor>,
    _root: &Utf8PathBuf,
    request: Request,
) -> Response {
    let store = supervisor.store();
    let k = request.k.clamp(1, 100);
    match store
        .search_memories(&request.query, request.scope.as_deref(), k)
        .await
    {
        Ok(entries) => Response::ok(entries),
        Err(e) => Response::err(crate::error::internal(e.to_string())),
    }
}
