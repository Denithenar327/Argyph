use argyph_parse::SymbolId;
use camino::Utf8PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SymbolSelector {
    ById(SymbolId),
    ByName { file: Utf8PathBuf, name: String },
    Qualified(String),
}
