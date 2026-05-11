use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;

use crate::error::McpErrorBody;

#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
pub struct Request {}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TierInfo {
    pub ready: bool,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Tiers {
    pub files: TierInfo,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WatcherInfo {
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiers: Option<Tiers>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub watcher: Option<WatcherInfo>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    pub fn ok(root: &Utf8PathBuf, file_count: u64, ready: bool) -> Self {
        Self {
            root: Some(root.as_str().to_string()),
            schema_version: Some(1),
            protocol_version: Some(argyph_core::Index::protocol_version().to_string()),
            tiers: Some(Tiers {
                files: TierInfo {
                    ready,
                    count: file_count,
                },
            }),
            watcher: Some(WatcherInfo { active: false }),
            error: None,
        }
    }

    #[allow(dead_code)]
    pub fn err(body: McpErrorBody) -> Self {
        Self {
            root: None,
            schema_version: None,
            protocol_version: None,
            tiers: None,
            watcher: None,
            error: Some(body),
        }
    }
}

pub async fn handle(supervisor: &Arc<Supervisor>, root: &Utf8PathBuf) -> Response {
    let tier_state = supervisor.get_tier_state().await;
    let ready = tier_state.is_ready();

    let file_count = supervisor
        .index()
        .status()
        .await
        .map(|s| s.file_count)
        .unwrap_or(0);

    Response::ok(root, file_count, ready)
}
