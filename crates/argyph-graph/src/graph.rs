use std::collections::HashMap;

use camino::Utf8Path;

use crate::edge::{Edge, EdgeKind};
use crate::selector::SymbolSelector;

#[derive(Debug, Clone)]
pub struct SymbolOutline {
    pub name: String,
    pub kind: String,
    pub signature: Option<String>,
    pub range: (u64, u64),
    pub children: Vec<SymbolOutline>,
}

#[derive(Debug)]
pub struct Graph {
    edges: Vec<Edge>,
    by_from: HashMap<String, Vec<usize>>,
    by_to: HashMap<String, Vec<usize>>,
    by_kind: HashMap<EdgeKind, Vec<usize>>,
}

impl Graph {
    #[must_use]
    pub fn new(edges: Vec<Edge>) -> Self {
        let mut by_from: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_to: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_kind: HashMap<EdgeKind, Vec<usize>> = HashMap::new();

        for (i, edge) in edges.iter().enumerate() {
            by_from
                .entry(edge.from.as_str().to_string())
                .or_default()
                .push(i);
            by_to
                .entry(edge.to.as_str().to_string())
                .or_default()
                .push(i);
            by_kind.entry(edge.kind).or_default().push(i);
        }

        Self {
            edges,
            by_from,
            by_to,
            by_kind,
        }
    }

    #[must_use]
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    #[must_use]
    pub fn count_by_kind(&self, kind: EdgeKind) -> usize {
        self.by_kind.get(&kind).map_or(0, |v| v.len())
    }

    pub fn find_definition(&self, name: &str, file: Option<&Utf8Path>) -> Vec<&Edge> {
        let def_indexes = self.by_kind.get(&EdgeKind::Defines);
        let Some(def_indexes) = def_indexes else {
            return Vec::new();
        };

        def_indexes
            .iter()
            .map(|&i| &self.edges[i])
            .filter(|e| {
                let id_str = e.to.as_str();
                let contains_name = id_str.contains(&format!("::{name}::"));
                let matches_file = file.is_none_or(|f| {
                    let f_str = f.as_str();
                    id_str.starts_with(f_str)
                        || f_str.ends_with(id_str.split("::").next().unwrap_or(""))
                });
                contains_name && matches_file
            })
            .collect()
    }

    pub fn find_references<'a>(&'a self, sel: &SymbolSelector) -> Vec<&'a Edge> {
        let target_ids = self.resolve_selector(sel);
        self.edges_matching(target_ids, EdgeKind::References)
    }

    pub fn callers<'a>(&'a self, sel: &SymbolSelector) -> Vec<&'a Edge> {
        let target_ids = self.resolve_selector(sel);
        self.edges_matching(target_ids, EdgeKind::Calls)
    }

    pub fn callees<'a>(&'a self, sel: &SymbolSelector) -> Vec<&'a Edge> {
        let source_ids = self.resolve_selector(sel);
        self.edges_matching_from(source_ids, EdgeKind::Calls)
    }

    pub fn imports_of<'a>(&'a self, file: &Utf8Path) -> Vec<&'a Edge> {
        let import_indexes = self.by_kind.get(&EdgeKind::Imports);
        let Some(import_indexes) = import_indexes else {
            return Vec::new();
        };
        let file_str = file.as_str();
        import_indexes
            .iter()
            .map(|&i| &self.edges[i])
            .filter(|e| {
                let id_str = e.from.as_str();
                id_str.starts_with(file_str)
                    || file_str.ends_with(id_str.split("::").next().unwrap_or(""))
            })
            .collect()
    }

    pub fn imported_by<'a>(&'a self, sel: &SymbolSelector) -> Vec<&'a Edge> {
        let target_ids = self.resolve_selector(sel);
        self.edges_matching_from(target_ids, EdgeKind::ImportedBy)
    }

    pub fn outline(&self, file: &Utf8Path) -> Vec<SymbolOutline> {
        let def_indexes = self.by_kind.get(&EdgeKind::Defines);
        let Some(def_indexes) = def_indexes else {
            return Vec::new();
        };

        let file_str = file.as_str();
        def_indexes
            .iter()
            .map(|&i| &self.edges[i])
            .filter(|e| {
                let id_str = e.from.as_str();
                id_str.starts_with(file_str)
                    || file_str.ends_with(id_str.split("::").next().unwrap_or(""))
            })
            .map(|e| {
                let id_str = e.from.as_str();
                let name = id_str.split("::").nth(1).unwrap_or("?");
                SymbolOutline {
                    name: name.to_string(),
                    kind: "symbol".to_string(),
                    signature: None,
                    range: (0, 0),
                    children: Vec::new(),
                }
            })
            .collect()
    }

    fn resolve_selector(&self, sel: &SymbolSelector) -> Vec<String> {
        match sel {
            SymbolSelector::ById(id) => vec![id.as_str().to_string()],
            SymbolSelector::ByName { file, name } => {
                let prefix = format!("{file}::{name}::");
                self.edges
                    .iter()
                    .filter(|e| e.to.as_str().starts_with(&prefix))
                    .map(|e| e.to.as_str().to_string())
                    .collect()
            }
            SymbolSelector::Qualified(qn) => self
                .edges
                .iter()
                .filter(|e| e.to.as_str().contains(qn.as_str()))
                .map(|e| e.to.as_str().to_string())
                .collect(),
        }
    }

    fn edges_matching(&self, target_ids: Vec<String>, kind: EdgeKind) -> Vec<&Edge> {
        let mut results = Vec::new();
        for tid in &target_ids {
            if let Some(indexes) = self.by_to.get(tid) {
                for &i in indexes {
                    let edge = &self.edges[i];
                    if edge.kind == kind {
                        results.push(edge);
                    }
                }
            }
        }
        results
    }

    fn edges_matching_from(&self, source_ids: Vec<String>, kind: EdgeKind) -> Vec<&Edge> {
        let mut results = Vec::new();
        for sid in &source_ids {
            if let Some(indexes) = self.by_from.get(sid) {
                for &i in indexes {
                    let edge = &self.edges[i];
                    if edge.kind == kind {
                        results.push(edge);
                    }
                }
            }
        }
        results
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::edge::{Confidence, Edge, EdgeKind};
    use argyph_parse::SymbolId;

    #[test]
    fn new_graph_is_queryable() {
        let edges = vec![Edge {
            from: SymbolId::new(&camino::Utf8PathBuf::from("src/lib.rs"), "fn_a", 0),
            to: SymbolId::new(&camino::Utf8PathBuf::from("src/lib.rs"), "fn_a", 0),
            kind: EdgeKind::Defines,
            confidence: Confidence::Resolved,
        }];
        let graph = Graph::new(edges);
        assert_eq!(graph.edges().len(), 1);
        assert_eq!(graph.count_by_kind(EdgeKind::Defines), 1);
        assert_eq!(graph.count_by_kind(EdgeKind::Calls), 0);
    }

    #[test]
    fn find_callers_and_callees() {
        let from_id = SymbolId::new(&camino::Utf8PathBuf::from("src/lib.rs"), "caller", 10);
        let to_id = SymbolId::new(&camino::Utf8PathBuf::from("src/lib.rs"), "callee", 50);
        let edges = vec![Edge {
            from: from_id.clone(),
            to: to_id.clone(),
            kind: EdgeKind::Calls,
            confidence: Confidence::Heuristic,
        }];
        let graph = Graph::new(edges);

        let callers = graph.callers(&SymbolSelector::ById(to_id.clone()));
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].from, from_id);

        let callees = graph.callees(&SymbolSelector::ById(from_id));
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].to, to_id);
    }

    #[test]
    fn find_references_by_name() {
        let from_id = SymbolId::new(&camino::Utf8PathBuf::from("src/lib.rs"), "caller", 10);
        let to_id = SymbolId::new(&camino::Utf8PathBuf::from("src/lib.rs"), "callee", 50);
        let edges = vec![Edge {
            from: from_id.clone(),
            to: to_id.clone(),
            kind: EdgeKind::References,
            confidence: Confidence::Heuristic,
        }];
        let graph = Graph::new(edges);

        let refs = graph.find_references(&SymbolSelector::ById(to_id));
        assert_eq!(refs.len(), 1);
    }
}
