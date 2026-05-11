use std::collections::HashSet;
use std::sync::Arc;

use camino::Utf8Path;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;

use crate::error::{self, McpErrorBody};
use crate::tools::common;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    pub file: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imports: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_by: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    fn ok(imports: Vec<String>, imported_by: Vec<String>) -> Self {
        Self {
            imports: Some(imports),
            imported_by: Some(imported_by),
            error: None,
        }
    }
    fn err(body: McpErrorBody) -> Self {
        Self {
            imports: None,
            imported_by: None,
            error: Some(body),
        }
    }
}

pub async fn handle(
    supervisor: &Arc<Supervisor>,
    _root: &camino::Utf8PathBuf,
    request: Request,
) -> Response {
    if supervisor.get_tier_state().await.tier_number() < 2 {
        return Response::err(error::index_not_ready());
    }

    let file = Utf8Path::new(&request.file);
    let file_prefix = format!("{file}::");

    let index = supervisor.index();
    match index.get_imports(file).await {
        Ok(edges) => {
            let mut imports = HashSet::new();
            let mut imported_by = HashSet::new();
            for e in &edges {
                if e.from.as_str().starts_with(&file_prefix) {
                    let (imported_file, _, _) = common::parse_sid(e.to.as_str());
                    imports.insert(imported_file.to_string());
                }
                if e.to.as_str().starts_with(&file_prefix) {
                    let (importer_file, _, _) = common::parse_sid(e.from.as_str());
                    imported_by.insert(importer_file.to_string());
                }
            }
            let mut imports: Vec<String> = imports.into_iter().collect();
            imports.sort();
            let mut imported_by: Vec<String> = imported_by.into_iter().collect();
            imported_by.sort();
            Response::ok(imports, imported_by)
        }
        Err(e) => Response::err(error::internal(e.to_string())),
    }
}
