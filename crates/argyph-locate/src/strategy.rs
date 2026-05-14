use crate::path::Locator;
use crate::types::Strategy;

/// The routing plan produced by the strategy dispatcher.
#[derive(Debug, Clone)]
pub enum Plan {
    /// Direct path lookup: resolve a structural path to a specific node.
    StructuralPath { locator: Locator },
    /// FTS over structural nodes (Tier 1.5 index).
    StructuralSearch { query: String },
    /// Pure semantic search (Tier 2 only).
    Semantic { query: String },
    /// Hybrid BM25 + vector search (Tier 2).
    Hybrid { query: String },
    /// Semantic search scoped to a specific structural node's byte range.
    ScopedSemantic { locator: Locator, query: String },
}

/// Map a [`Plan`] to its [`Strategy`] for the response.
pub fn strategy_of(plan: &Plan) -> Strategy {
    match plan {
        Plan::StructuralPath { .. } => Strategy::StructuralPath,
        Plan::StructuralSearch { .. } => Strategy::StructuralSearch,
        Plan::Semantic { .. } => Strategy::Semantic,
        Plan::Hybrid { .. } => Strategy::Hybrid,
        Plan::ScopedSemantic { .. } => Strategy::ScopedSemantic,
    }
}

/// Choose the best strategy given the available inputs and index readiness.
///
/// # Rules
/// - If `path` is present and `query` is absent → `StructuralPath`
/// - If `query` is present and `path` is absent:
///     - `has_tier2` && not just a short name → `Hybrid`
///     - `has_tier2` && single word/short name → `StructuralSearch` first
///     - `!has_tier2` → `StructuralSearch`
/// - If both `path` and `query` are present → `ScopedSemantic`
pub fn plan(query: Option<&str>, path: Option<&str>, has_tier2: bool) -> Plan {
    match (path, query) {
        (Some(raw_path), None) => {
            let locator = crate::path::parse(raw_path);
            Plan::StructuralPath { locator }
        }
        (None, Some(q)) => {
            let short = q.split_whitespace().count() <= 2;
            if has_tier2 && !short {
                Plan::Hybrid {
                    query: q.to_string(),
                }
            } else if has_tier2 && short {
                Plan::Semantic {
                    query: q.to_string(),
                }
            } else {
                Plan::StructuralSearch {
                    query: q.to_string(),
                }
            }
        }
        (Some(raw_path), Some(q)) => {
            let locator = crate::path::parse(raw_path);
            Plan::ScopedSemantic {
                locator,
                query: q.to_string(),
            }
        }
        (None, None) => unreachable!("caller validates at least one of query/path is present"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn path_only_is_structural_path() {
        let p = plan(None, Some("/doc/Design"), false);
        assert!(matches!(p, Plan::StructuralPath { .. }));
        assert_eq!(strategy_of(&p), Strategy::StructuralPath);
    }

    #[test]
    fn query_only_no_tier2_is_structural_search() {
        let p = plan(Some("error handling"), None, false);
        assert!(matches!(p, Plan::StructuralSearch { .. }));
        assert_eq!(strategy_of(&p), Strategy::StructuralSearch);
    }

    #[test]
    fn query_only_tier2_short_is_semantic() {
        let p = plan(Some("login"), None, true);
        assert!(matches!(p, Plan::Semantic { .. }));
        assert_eq!(strategy_of(&p), Strategy::Semantic);
    }

    #[test]
    fn query_only_tier2_long_is_hybrid() {
        let p = plan(Some("how does authentication work"), None, true);
        assert!(matches!(p, Plan::Hybrid { .. }));
        assert_eq!(strategy_of(&p), Strategy::Hybrid);
    }

    #[test]
    fn path_and_query_is_scoped_semantic() {
        let p = plan(Some("/doc/Design"), Some("authentication"), true);
        assert!(matches!(p, Plan::ScopedSemantic { .. }));
        assert_eq!(strategy_of(&p), Strategy::ScopedSemantic);
    }
}
