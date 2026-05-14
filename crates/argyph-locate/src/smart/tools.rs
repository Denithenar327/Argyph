//! In-process dispatch for the four read-only sub-tools the model may invoke.
//! Hardcoded allowlist — model cannot escape this set.

use crate::types::{Request as LocateRequest, Response as LocateResponse};
use argyph_embed::Embedder;
use argyph_store::Store;
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

fn outline_to_json(outlines: &[argyph_graph::graph::SymbolOutline]) -> serde_json::Value {
    fn node_to_json(o: &argyph_graph::graph::SymbolOutline) -> serde_json::Value {
        serde_json::json!({
            "name": o.name,
            "kind": o.kind,
            "range": o.range,
            "children": o.children.iter().map(node_to_json).collect::<Vec<_>>(),
        })
    }
    serde_json::Value::Array(outlines.iter().map(node_to_json).collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "name", content = "arguments", rename_all = "snake_case")]
pub enum SubTool {
    Locate(LocateRequest),
    ReadFileRange {
        file: String,
        byte_range: (u32, u32),
    },
    GetSymbolOutline {
        file: String,
    },
    GetRepoOverview {},
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum SubToolOutput {
    Locate(LocateResponse),
    ReadFileRange {
        file: String,
        content: String,
        truncated: bool,
    },
    SymbolOutline {
        file: String,
        outline: serde_json::Value,
    },
    RepoOverview {
        overview: serde_json::Value,
    },
}

pub struct SubToolCtx {
    pub store: Arc<dyn Store>,
    pub embedder: Arc<dyn Embedder>,
    pub root: PathBuf,
}

pub async fn dispatch(
    ctx: &SubToolCtx,
    name: &str,
    args: &serde_json::Value,
    max_bytes_per_read: u32,
) -> anyhow::Result<SubToolOutput> {
    match name {
        "locate" => {
            let req: LocateRequest = serde_json::from_value(args.clone())?;
            let resp =
                crate::locate(ctx.store.clone(), ctx.embedder.clone(), &ctx.root, req).await?;
            Ok(SubToolOutput::Locate(resp))
        }
        "read_file_range" => {
            let file = args["file"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("read_file_range: missing `file`"))?
                .to_string();
            let start = args["byte_range"][0]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("read_file_range: bad byte_range[0]"))? as u32;
            let end = args["byte_range"][1]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("read_file_range: bad byte_range[1]"))? as u32;
            let capped_end = std::cmp::min(end, start.saturating_add(max_bytes_per_read));
            let full_path = ctx.root.join(&file);
            let content = std::fs::read_to_string(&full_path).unwrap_or_default();
            let byte_content = content.as_bytes();
            let start_usize = start as usize;
            let capped_usize = (capped_end as usize).min(byte_content.len());
            let slice = if start_usize < byte_content.len() {
                String::from_utf8_lossy(&byte_content[start_usize..capped_usize]).into_owned()
            } else {
                String::new()
            };
            Ok(SubToolOutput::ReadFileRange {
                file,
                content: slice,
                truncated: capped_end < end,
            })
        }
        "get_symbol_outline" => {
            let file = args["file"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("get_symbol_outline: missing `file`"))?
                .to_string();
            let file_path = Utf8Path::new(&file);
            let outline = ctx.store.get_symbol_outline(file_path).await?;
            Ok(SubToolOutput::SymbolOutline {
                file,
                outline: outline_to_json(&outline),
            })
        }
        "get_repo_overview" => {
            // TODO: Store does not expose repo_overview; requires Supervisor/Index from argyph-core.
            anyhow::bail!("get_repo_overview: not yet implemented in locate-smart context")
        }
        other => {
            anyhow::bail!(
                "LOCATE_SMART_DISABLED_TOOL: model tried to call `{other}` which is not in the allowlist"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unknown_tool_rejection_message() {
        let err = anyhow::anyhow!(
            "LOCATE_SMART_DISABLED_TOOL: model tried to call `delete_repo` which is not in the allowlist"
        );
        assert!(err.to_string().contains("LOCATE_SMART_DISABLED_TOOL"));
    }
}