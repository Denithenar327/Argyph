use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;

use crate::error::{self, McpErrorBody};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    pub name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub language_hint: Option<String>,
    #[serde(default)]
    pub file_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SourceRange {
    pub file: String,
    pub range: (u64, u64),
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Definition {
    pub symbol_id: String,
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub location: SourceRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definitions: Option<Vec<Definition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    fn ok(defs: Vec<Definition>) -> Self {
        Self {
            definitions: Some(defs),
            error: None,
        }
    }

    fn err(body: McpErrorBody) -> Self {
        Self {
            definitions: None,
            error: Some(body),
        }
    }
}

pub async fn handle(
    supervisor: &Arc<Supervisor>,
    _root: &Utf8PathBuf,
    request: Request,
) -> Response {
    if supervisor.get_tier_state().await.tier_number() < 2 {
        return Response::err(error::index_not_ready());
    }

    let file_opt = request.file_hint.as_deref().map(camino::Utf8Path::new);

    let index = supervisor.index();
    match index.find_symbol(&request.name, file_opt).await {
        Ok(symbols) => {
            let defs = symbols
                .into_iter()
                .map(|s| {
                    let range = (s.range.start as u64, s.range.end as u64);
                    Definition {
                        symbol_id: s.id.as_str().to_string(),
                        name: s.name,
                        kind: format!("{:?}", s.kind).to_lowercase(),
                        signature: s.signature,
                        location: SourceRange {
                            file: s.file.to_string(),
                            range,
                        },
                        language: None,
                        docstring: None,
                    }
                })
                .collect();
            Response::ok(defs)
        }
        Err(e) => Response::err(error::internal(e.to_string())),
    }
}
