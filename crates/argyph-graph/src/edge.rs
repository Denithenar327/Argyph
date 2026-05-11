use argyph_parse::SymbolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    Defines,
    References,
    Calls,
    Imports,
    ImportedBy,
    Implements,
    Inherits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Confidence {
    Resolved,
    Heuristic,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: SymbolId,
    pub to: SymbolId,
    pub kind: EdgeKind,
    pub confidence: Confidence,
}
