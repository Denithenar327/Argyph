use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;

use crate::error::McpErrorBody;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    pub scope: String,
    pub content: String,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    pub fn ok(id: String) -> Self {
        Self {
            id: Some(id),
            error: None,
        }
    }

    pub fn err(body: McpErrorBody) -> Self {
        Self {
            id: None,
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
    match store
        .save_memory(&request.scope, &request.content, &request.metadata)
        .await
    {
        Ok(id) => Response::ok(id),
        Err(e) => Response::err(crate::error::internal(e.to_string())),
    }
}
