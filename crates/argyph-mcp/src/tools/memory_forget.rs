use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;

use crate::error::McpErrorBody;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    pub forgotten: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    pub fn ok() -> Self {
        Self {
            forgotten: true,
            error: None,
        }
    }

    pub fn err(body: McpErrorBody) -> Self {
        Self {
            forgotten: false,
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
    match store.forget_memory(&request.id).await {
        Ok(()) => Response::ok(),
        Err(e) => Response::err(crate::error::internal(e.to_string())),
    }
}
