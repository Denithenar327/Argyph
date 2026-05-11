use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct Filter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths_glob: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_glob: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SymbolSelector {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_hint: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_hint: Option<String>,
}
