use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;
use argyph_store::search::SearchFilter;

use crate::error::{self, McpErrorBody};
use crate::types::Filter;
use crate::validate;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    pub query: String,
    #[serde(default = "default_k")]
    pub k: usize,
    #[serde(default = "default_alpha")]
    #[allow(dead_code)]
    pub alpha: f64,
    #[serde(default)]
    pub filter: Option<Filter>,
}

fn default_k() -> usize {
    10
}

fn default_alpha() -> f64 {
    0.5
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SemanticHit {
    pub chunk_id: String,
    pub chunk_text: String,
    pub file: String,
    pub score: f32,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hits: Option<Vec<SemanticHit>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_coverage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_embedded: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_chunks: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    fn ok(result: &argyph_core::SemanticResult) -> Self {
        let coverage = if result.total_chunks > 0 {
            result.total_embedded as f64 / result.total_chunks as f64
        } else {
            0.0
        };
        Self {
            hits: Some(
                result
                    .hits
                    .iter()
                    .map(|h| SemanticHit {
                        chunk_id: h.chunk_id.clone(),
                        chunk_text: h.chunk_text.clone(),
                        file: h.file.clone(),
                        score: h.score,
                        source: h.source.clone(),
                    })
                    .collect(),
            ),
            index_coverage: Some(coverage),
            total_embedded: Some(result.total_embedded),
            total_chunks: Some(result.total_chunks),
            error: None,
        }
    }

    fn err(body: McpErrorBody) -> Self {
        Self {
            hits: None,
            index_coverage: None,
            total_embedded: None,
            total_chunks: None,
            error: Some(body),
        }
    }
}

pub async fn handle(
    supervisor: &Arc<Supervisor>,
    _root: &Utf8PathBuf,
    request: Request,
) -> Response {
    if !supervisor.get_tier_state().await.is_ready() {
        return Response::err(error::index_not_ready());
    }

    let k = validate::clamp_u64(request.k as u64, 1, 100) as usize;
    let filter = request.filter.map(|f| SearchFilter {
        language: f.languages.and_then(|v| v.into_iter().next()),
        paths_glob: f.paths_glob.and_then(|v| v.into_iter().next()),
        exclude_glob: f.exclude_glob.and_then(|v| v.into_iter().next()),
        file_ids: None,
    });

    let index = supervisor.index();
    match index
        .search_semantic(&request.query, k, filter.as_ref())
        .await
    {
        Ok(result) => Response::ok(&result),
        Err(e) => Response::err(error::internal(e.to_string())),
    }
}
