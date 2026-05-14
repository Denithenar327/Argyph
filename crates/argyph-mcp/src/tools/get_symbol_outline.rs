use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;

use crate::error::{self, McpErrorBody};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    pub file: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OutlineNode {
    pub symbol_id: String,
    pub name: String,
    pub kind: String,
    pub range: (u64, u64),
    pub children: Vec<OutlineNode>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<Vec<OutlineNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    fn ok(file: String, outline: Vec<OutlineNode>) -> Self {
        Self {
            file: Some(file),
            language: None,
            outline: Some(outline),
            error: None,
        }
    }

    fn err(body: McpErrorBody) -> Self {
        Self {
            file: None,
            language: None,
            outline: None,
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

    let file = Utf8Path::new(&request.file);

    let index = supervisor.index();
    match index.get_symbol_outline(file).await {
        Ok(outlines) => {
            let nodes: Vec<OutlineNode> = outlines
                .into_iter()
                .map(|o| convert_outline(file.as_str(), &o))
                .collect();
            Response::ok(request.file, nodes)
        }
        Err(e) => Response::err(error::internal(e.to_string())),
    }
}

fn convert_outline(file: &str, outline: &argyph_graph::graph::SymbolOutline) -> OutlineNode {
    OutlineNode {
        symbol_id: format!("{file}::{name}::0", name = outline.name),
        name: outline.name.clone(),
        kind: outline.kind.clone(),
        range: outline.range,
        children: outline
            .children
            .iter()
            .map(|c| convert_outline(file, c))
            .collect(),
    }
}
