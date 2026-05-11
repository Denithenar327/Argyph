use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::{SearchFilter, Supervisor};

use crate::error::{self, McpErrorBody};
use crate::types::Filter;
use crate::validate;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    pub pattern: String,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default = "default_max_results")]
    pub max_results: u64,
    #[serde(default)]
    pub filter: Option<Filter>,
}

fn default_max_results() -> u64 {
    100
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchHit {
    pub file: String,
    pub line: u64,
    pub column: u64,
    #[serde(rename = "match")]
    pub match_text: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hits: Option<Vec<SearchHit>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    fn ok(result: argyph_core::SearchResult) -> Self {
        Self {
            hits: Some(
                result
                    .hits
                    .into_iter()
                    .map(|h| SearchHit {
                        file: h.file.to_string(),
                        line: h.line,
                        column: h.column,
                        match_text: h.match_text,
                        context_before: vec![],
                        context_after: vec![],
                    })
                    .collect(),
            ),
            truncated: Some(result.truncated),
            error: None,
        }
    }

    fn err(body: McpErrorBody) -> Self {
        Self {
            hits: None,
            truncated: None,
            error: Some(body),
        }
    }
}

pub async fn handle(
    supervisor: &Arc<Supervisor>,
    root: &Utf8PathBuf,
    request: Request,
) -> Response {
    if !supervisor.get_tier_state().await.is_ready() {
        return Response::err(error::index_not_ready());
    }

    let max_results = validate::clamp_u64(request.max_results, 1, 1000);
    let filter = request.filter.map(|f| SearchFilter {
        paths_glob: f.paths_glob,
        exclude_glob: f.exclude_glob,
    });

    let index = supervisor.index();
    match index
        .search_text(
            root,
            &request.pattern,
            request.regex,
            request.case_sensitive,
            max_results,
            filter,
        )
        .await
    {
        Ok(result) => Response::ok(result),
        Err(e) => Response::err(error::internal(e.to_string())),
    }
}
