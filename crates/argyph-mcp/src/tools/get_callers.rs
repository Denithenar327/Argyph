use std::collections::HashMap;
use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;
use argyph_graph::edge::Edge;

use crate::error::{self, McpErrorBody};
use crate::tools::common;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    #[serde(default)]
    pub symbol_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub language_hint: Option<String>,
    #[serde(default)]
    pub file_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CallSite {
    pub file: String,
    pub range: (u64, u64),
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CallerInfo {
    pub symbol_id: String,
    pub name: String,
    pub kind: String,
    pub location: CallSite,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CallerEntry {
    pub caller: CallerInfo,
    pub call_sites: Vec<CallSite>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callers: Option<Vec<CallerEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    fn ok(callers: Vec<CallerEntry>) -> Self {
        Self {
            callers: Some(callers),
            error: None,
        }
    }
    fn err(body: McpErrorBody) -> Self {
        Self {
            callers: None,
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

    let sel = match common::resolve_selector(&request.symbol_id, &request.name, &request.file_hint)
    {
        Ok(s) => s,
        Err(e) => return Response::err(e),
    };

    let index = supervisor.index();
    match index.get_callers(&sel).await {
        Ok(edges) => Response::ok(group_edges_by_from(&edges)),
        Err(e) => Response::err(error::internal(e.to_string())),
    }
}

fn group_edges_by_from(edges: &[Edge]) -> Vec<CallerEntry> {
    let mut by_caller: HashMap<String, (CallerInfo, Vec<CallSite>)> = HashMap::new();
    for e in edges {
        let id_str = e.from.as_str();
        let (file, name, start) = common::parse_sid(id_str);
        let key = format!("{file}::{name}");
        let entry = by_caller.entry(key).or_insert_with(|| {
            (
                CallerInfo {
                    symbol_id: id_str.to_string(),
                    name: name.to_string(),
                    kind: "function".to_string(),
                    location: CallSite {
                        file: file.to_string(),
                        range: (start as u64, start.saturating_add(1) as u64),
                    },
                },
                Vec::new(),
            )
        });
        entry.1.push(CallSite {
            file: file.to_string(),
            range: (start as u64, start.saturating_add(1) as u64),
        });
    }
    by_caller
        .into_values()
        .map(|(caller, sites)| CallerEntry {
            caller,
            call_sites: sites,
        })
        .collect()
}
