//! Validate the model's final span selection against the history of spans
//! returned by `locate` calls made earlier in the same loop.

use crate::types::Span;
use std::collections::HashMap;

pub fn span_node_id(s: &Span) -> String {
    s.node_id.clone()
}

#[derive(Default)]
pub struct SpanHistory {
    by_id: HashMap<String, Span>,
}

impl SpanHistory {
    pub fn record(&mut self, span: Span) {
        let id = span_node_id(&span);
        self.by_id.insert(id, span);
    }

    pub fn record_many(&mut self, spans: impl IntoIterator<Item = Span>) {
        for s in spans {
            self.record(s);
        }
    }

    pub fn resolve(&self, selected: &[String]) -> Result<Vec<Span>, Vec<String>> {
        let mut out = Vec::with_capacity(selected.len());
        let mut missing = Vec::new();
        for id in selected {
            match self.by_id.get(id) {
                Some(s) => out.push(s.clone()),
                None => missing.push(id.clone()),
            }
        }
        if missing.is_empty() {
            Ok(out)
        } else {
            Err(missing)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::types::{ExpandTo, Span};

    fn fake(file: &str, start: u32, end: u32) -> Span {
        Span {
            node_id: format!("{file}:{start}:{end}"),
            file: file.into(),
            byte_range: (start, end),
            line_range: (1, 1),
            kind: "MdSection".into(),
            path: vec![],
            content: "x".into(),
            score: 1.0,
            truncated: false,
            expand_to: ExpandTo {
                parent: None,
                file: None,
            },
        }
    }

    #[test]
    fn resolves_known_ids() {
        let mut h = SpanHistory::default();
        h.record(fake("a.md", 0, 10));
        let ids = vec!["a.md:0:10".to_string()];
        let r = h.resolve(&ids).unwrap();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn rejects_fabricated_ids() {
        let h = SpanHistory::default();
        let ids = vec!["fake:0:0".to_string()];
        let missing = h.resolve(&ids).unwrap_err();
        assert_eq!(missing, vec!["fake:0:0".to_string()]);
    }

    #[test]
    fn partial_failure_lists_only_missing() {
        let mut h = SpanHistory::default();
        h.record(fake("a.md", 0, 10));
        let ids = vec!["a.md:0:10".into(), "fake:0:0".into()];
        let missing = h.resolve(&ids).unwrap_err();
        assert_eq!(missing, vec!["fake:0:0"]);
    }
}
