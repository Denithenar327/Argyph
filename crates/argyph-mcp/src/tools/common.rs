use camino::Utf8PathBuf;

use argyph_graph::selector::SymbolSelector;
use argyph_parse::SymbolId;

use crate::error::McpErrorBody;

pub fn resolve_selector(
    symbol_id: &Option<String>,
    name: &Option<String>,
    file_hint: &Option<String>,
) -> Result<SymbolSelector, McpErrorBody> {
    if let Some(id) = symbol_id {
        let sid = SymbolId::from_raw(id.clone());
        return Ok(SymbolSelector::ById(sid));
    }
    if let Some(name) = name {
        if let Some(file) = file_hint {
            return Ok(SymbolSelector::ByName {
                file: Utf8PathBuf::from(file.as_str()),
                name: name.clone(),
            });
        }
        return Ok(SymbolSelector::Qualified(name.clone()));
    }
    Err(crate::error::internal(
        "Either symbol_id or name must be provided".to_string(),
    ))
}

pub fn parse_sid(id: &str) -> (&str, &str, usize) {
    let rest = id.rsplit_once("::").unwrap_or((id, ""));
    let (prefix, start_str) = rest;
    let start: usize = start_str.parse().unwrap_or(0);
    let (file, name) = prefix.rsplit_once("::").unwrap_or((prefix, "?"));
    (file, name, start)
}
