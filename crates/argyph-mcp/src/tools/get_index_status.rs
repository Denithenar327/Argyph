use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;

use crate::error::McpErrorBody;

#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
pub struct Request {}

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

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TierInfo {
    pub ready: bool,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Tiers {
    pub files: TierInfo,
    pub symbols: TierInfo,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WatcherInfo {
    pub active: bool,
}

impl Response {
    pub fn ok(
        root: &str,
        file_count: u64,
        files_ready: bool,
        symbols_ready: bool,
        symbol_count: u64,
    ) -> Self {
        Self {
            root: Some(root.to_string()),
            schema_version: Some(1),
            protocol_version: Some(argyph_core::Index::protocol_version().to_string()),
            tiers: Some(Tiers {
                files: TierInfo {
                    ready: files_ready,
                    count: file_count,
                },
                symbols: TierInfo {
                    ready: symbols_ready,
                    count: symbol_count,
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
    let tier_num = tier_state.tier_number();
    let files_ready = tier_num >= 1;
    let symbols_ready = tier_num >= 2;

    let index = supervisor.index();
    let file_count = index.status().await.map(|s| s.file_count).unwrap_or(0);

    let symbol_count = tier_state.symbol_count();

    Response::ok(
        root.as_str(),
        file_count,
        files_ready,
        symbols_ready,
        symbol_count,
    )
}
