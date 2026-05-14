use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;
use argyph_store::MemoryEntry;

use crate::error::McpErrorBody;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    pub scope: String,
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
    pub memories: Option<Vec<MemoryHit>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    pub fn ok(entries: Vec<MemoryEntry>) -> Self {
        Self {
            memories: Some(
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
            memories: None,
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
    match store.list_memories(&request.scope).await {
        Ok(entries) => Response::ok(entries),
        Err(e) => Response::err(crate::error::internal(e.to_string())),
    }
}
