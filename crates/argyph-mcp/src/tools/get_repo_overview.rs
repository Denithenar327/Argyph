use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;

use crate::error::{self, McpErrorBody};
use crate::validate;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    #[serde(default = "default_max_tree_depth")]
    pub max_tree_depth: u64,
}

fn default_max_tree_depth() -> u64 {
    3
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<LanguageSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_points: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readme_excerpt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LanguageSummary {
    pub name: String,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GitInfo {
    pub branch: String,
    pub head_short: String,
    pub dirty: bool,
}

impl Response {
    fn ok(overview: argyph_core::RepoOverview) -> Self {
        Self {
            languages: Some(
                overview
                    .languages
                    .into_iter()
                    .map(|l| LanguageSummary {
                        name: l.name,
                        files: l.files,
                    })
                    .collect(),
            ),
            entry_points: Some(overview.entry_points),
            readme_excerpt: Some(overview.readme_excerpt),
            tree: Some(overview.tree),
            git: overview.git.map(|g| GitInfo {
                branch: g.branch,
                head_short: g.head_short,
                dirty: g.dirty,
            }),
            error: None,
        }
    }

    fn err(body: McpErrorBody) -> Self {
        Self {
            languages: None,
            entry_points: None,
            readme_excerpt: None,
            tree: None,
            git: None,
            error: Some(body),
        }
    }
}

pub async fn handle(
    supervisor: &Arc<Supervisor>,
    root: &Utf8PathBuf,
    request: Request,
) -> Response {
    let depth = validate::clamp_u64(request.max_tree_depth, 1, 6);
    let index = supervisor.index();
    match index.overview(root, depth).await {
        Ok(overview) => Response::ok(overview),
        Err(_e) => Response::err(error::index_not_ready()),
    }
}
