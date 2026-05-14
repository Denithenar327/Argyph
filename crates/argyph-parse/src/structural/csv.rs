use super::{byte_to_line_range, line_starts, NodeKind, StructuralNode};

/// Parse a CSV source into structural nodes (header + rows).
pub fn parse(file_id: u64, source: &str) -> Vec<StructuralNode> {
    let ls = line_starts(source);
    let mut nodes = Vec::new();
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(source.as_bytes());

    if let Ok(headers) = reader.headers() {
        let header_labels: Vec<String> = headers.iter().map(|s| s.to_string()).collect();
        let header_end = source.find('\n').unwrap_or(source.len());
        let (line_s, line_e) = byte_to_line_range(&ls, 0, header_end);

        nodes.push(StructuralNode {
            id: StructuralNode::make_id(file_id, NodeKind::CsvHeader, &["header".into()]),
            file_id,
            kind: NodeKind::CsvHeader,
            label: header_labels.join(", "),
            path: vec!["header".into()],
            byte_range: (0, header_end),
            line_range: (line_s, line_e),
            parent: None,
            depth: 0,
        });
    }

    for record in reader.records().flatten() {
        if let Some(pos) = record.position().cloned() {
            let start = pos.byte() as usize;
            let rest = &source[start..];
            let line_end_offset = rest.find('\n').unwrap_or(rest.len());
            let end = start + line_end_offset;

            let idx = nodes.len();
            let label = record.iter().take(3).collect::<Vec<_>>().join(", ");
            let path = vec![format!("row_{idx}")];
            let id = StructuralNode::make_id(file_id, NodeKind::CsvRow, &path);
            let (line_s, line_e) = byte_to_line_range(&ls, start, end);

            nodes.push(StructuralNode {
                id,
                file_id,
                kind: NodeKind::CsvRow,
                label,
                path,
                byte_range: (start, end),
                line_range: (line_s, line_e),
                parent: None,
                depth: 0,
            });
        }
    }

    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural::NodeKind;

    const SAMPLE: &str = "id,name,email\n1,Alice,alice@x.com\n2,Bob,bob@x.com\n";

    #[test]
    fn extracts_header_and_rows() {
        let nodes = parse(1, SAMPLE);
        let header = nodes.iter().find(|n| n.kind == NodeKind::CsvHeader);
        assert!(header.is_some(), "should have a header node");
        let rows: Vec<&StructuralNode> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::CsvRow)
            .collect();
        assert_eq!(rows.len(), 2, "expected 2 data rows");
    }

    #[test]
    fn row_byte_range_is_correct() {
        let nodes = parse(1, SAMPLE);
        let row = nodes
            .iter()
            .find(|n| n.kind == NodeKind::CsvRow && n.label.contains("Alice"))
            .unwrap();
        let row_text = &SAMPLE[row.byte_range.0..row.byte_range.1];
        assert!(
            row_text.contains("Alice"),
            "row text should contain 'Alice', got: {row_text:?}"
        );
    }
}
