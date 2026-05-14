use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;
use argyph_pack::{PackFormat, PackInclude, PackRequest, PackScope};

use crate::error::{self, McpErrorBody};
use crate::validate;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default = "default_budget")]
    pub token_budget: u64,
    #[serde(default)]
    pub scope: Option<Scope>,
    #[serde(default)]
    pub include_tests: bool,
    #[serde(default)]
    pub include_docs: bool,
}

fn default_budget() -> u64 {
    100_000
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    All,
    Paths(Vec<String>),
    Symbol(String),
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FileMeta {
    pub path: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_included: Option<Vec<FileMeta>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_omitted: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_truncated: Option<Vec<FileMeta>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    fn ok(result: argyph_pack::PackResult) -> Self {
        let fmt = match result.format {
            PackFormat::Xml => "xml",
            PackFormat::Markdown => "markdown",
        };
        let trunc_set: std::collections::HashSet<_> = result.files_truncated.iter().collect();
        Self {
            content: Some(result.content),
            format: Some(fmt.to_string()),
            token_count: Some(result.token_count),
            files_included: Some(
                result
                    .files_included
                    .iter()
                    .map(|p| FileMeta {
                        path: p.to_string(),
                        truncated: trunc_set.contains(p),
                    })
                    .collect(),
            ),
            files_truncated: Some(
                result
                    .files_truncated
                    .iter()
                    .map(|p| FileMeta {
                        path: p.to_string(),
                        truncated: true,
                    })
                    .collect(),
            ),
            files_omitted: Some(result.files_omitted.iter().map(|p| p.to_string()).collect()),
            error: None,
        }
    }

    fn err(body: McpErrorBody) -> Self {
        Self {
            content: None,
            format: None,
            token_count: None,
            files_included: None,
            files_omitted: None,
            files_truncated: None,
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

    let scope = match request.scope.unwrap_or(Scope::All) {
        Scope::All => PackScope::All,
        Scope::Paths(paths) => PackScope::Paths(paths.into_iter().map(Utf8PathBuf::from).collect()),
        Scope::Symbol(name) => PackScope::Symbol(name),
    };

    let format = match request.format.as_deref().unwrap_or("xml") {
        "markdown" | "md" => PackFormat::Markdown,
        _ => PackFormat::Xml,
    };

    let budget = validate::clamp_u64(request.token_budget, 1000, 10_000_000) as usize;

    let req = PackRequest {
        scope,
        format,
        token_budget: budget,
        include: PackInclude {
            tests: request.include_tests,
            docs: request.include_docs,
        },
    };

    let index = supervisor.index();
    match index.pack(root, &req).await {
        Ok(result) => Response::ok(result),
        Err(e) => Response::err(error::internal(e.to_string())),
    }
}
