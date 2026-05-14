use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub files: Option<Vec<String>>,
    #[serde(default = "default_max_results")]
    pub max_results: u8,
    #[serde(default = "default_max_bytes")]
    pub max_bytes_per_span: u32,
}

fn default_max_results() -> u8 {
    3
}

fn default_max_bytes() -> u32 {
    4096
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub spans: Vec<Span>,
    pub strategy_used: Strategy,
    pub index_coverage: IndexCoverage,
}

#[derive(Debug, Clone, Serialize)]
pub struct Span {
    pub node_id: String,
    pub file: String,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub kind: String,
    pub path: Vec<String>,
    pub content: String,
    pub score: f32,
    pub truncated: bool,
    pub expand_to: ExpandTo,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpandTo {
    pub parent: Option<ExpandTarget>,
    pub file: Option<ExpandTarget>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpandTarget {
    pub node_id: Option<String>,
    pub label: Option<String>,
    pub bytes: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    StructuralPath,
    StructuralSearch,
    Semantic,
    Hybrid,
    ScopedSemantic,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexCoverage {
    pub tier_1_5: String,
    pub tier_2: String,
}
