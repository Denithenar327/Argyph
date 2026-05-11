use camino::Utf8PathBuf;

/// Half-open byte range `[start, end)` in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// Stable identifier for a symbol within a codebase.
///
/// Formed from the file path, symbol name, and byte range so it remains
/// stable across re-indexes unless the symbol itself moves or is renamed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolId(String);

impl SymbolId {
    #[must_use]
    pub fn new(file: &Utf8PathBuf, name: &str, start: usize) -> Self {
        Self(format!("{file}::{name}::{start}"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn from_raw(raw: String) -> Self {
        Self(raw)
    }
}

impl std::fmt::Display for SymbolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The kind of a code symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Impl,
    Class,
    Module,
    Variable,
    TypeAlias,
    Constant,
    Interface,
    Macro,
    Static,
}

impl SymbolKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Class => "class",
            Self::Module => "module",
            Self::Variable => "variable",
            Self::TypeAlias => "type_alias",
            Self::Constant => "constant",
            Self::Interface => "interface",
            Self::Macro => "macro",
            Self::Static => "static",
        }
    }
}

/// A single code symbol extracted from a source file.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub file: Utf8PathBuf,
    pub range: ByteRange,
    /// The symbol's signature text (e.g. function parameters, class declaration).
    pub signature: Option<String>,
    /// Parent symbol ID, if this symbol is nested inside another.
    pub parent: Option<SymbolId>,
}

/// Content-addressed identifier for a chunk.
///
/// Computed as the BLAKE3 hash of whitespace-normalized chunk text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkId(pub [u8; 32]);

impl ChunkId {
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let normalized = normalize_chunk_text(text);
        Self(blake3::hash(normalized.as_bytes()).into())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for ChunkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// The kind of a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    /// A function or method body.
    FunctionBody,
    /// A type definition (struct, enum, trait, class, interface).
    TypeDef,
    /// Top-level code that doesn't fit into a named construct.
    TopLevel,
    /// A character-based fallback split (for oversized nodes).
    Fallback,
}

/// An AST-aware chunk of source text, ready for embedding.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: ChunkId,
    pub file: Utf8PathBuf,
    pub range: ByteRange,
    pub text: String,
    pub kind: ChunkKind,
    pub language: argyph_fs::Language,
}

/// A raw import statement, unresolved.
///
/// Resolution into cross-file edges is the responsibility of `argyph-graph`.
#[derive(Debug, Clone)]
pub struct Import {
    /// The raw import text.
    pub raw: String,
    /// The module path being imported (split on `.` or `/`).
    pub module_path: Vec<String>,
    /// Specific items imported, if any.
    pub items: Vec<String>,
    /// Byte range of the import statement in the source file.
    pub range: ByteRange,
}

/// The result of parsing a single file.
#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub symbols: Vec<Symbol>,
    pub chunks: Vec<Chunk>,
    pub imports: Vec<Import>,
}

/// Compute a normalized text for content-addressed chunk IDs.
fn normalize_chunk_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}
