use super::{byte_to_line_range, line_starts, NodeKind, StructuralNode};
use serde_json::Value;
use std::collections::BTreeMap;

fn find_in(haystack: &str, needle: &str, start: usize) -> Option<usize> {
    haystack[start..].find(needle).map(|off| start + off)
}

fn skip_ws(source: &str, pos: usize) -> usize {
    let mut p = pos;
    for ch in source[p..].chars() {
        if ch != ' ' && ch != '\n' && ch != '\r' && ch != '\t' {
            break;
        }
        p += ch.len_utf8();
    }
    p
}

fn find_value_end(source: &str, start: usize) -> usize {
    let mut p = start;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;

    for ch in source[p..].chars() {
        if escaped {
            escaped = false;
            p += ch.len_utf8();
            continue;
        }
        if ch == '\\' && in_string {
            escaped = true;
            p += ch.len_utf8();
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            p += ch.len_utf8();
            continue;
        }
        if in_string {
            p += ch.len_utf8();
            continue;
        }
        match ch {
            '{' | '[' => {
                depth += 1;
            }
            '}' | ']' => {
                if depth == 0 {
                    return p + ch.len_utf8();
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                return p;
            }
            _ => {}
        }
        p += ch.len_utf8();
    }
    source.len()
}

#[allow(clippy::too_many_arguments)]
fn walk_value(
    source: &str,
    value: &Value,
    path: &[String],
    pos: &mut usize,
    file_id: u64,
    ls: &[usize],
    nodes: &mut Vec<StructuralNode>,
    parent_id: Option<super::NodeId>,
    depth: u32,
) {
    match value {
        Value::Object(map) => {
            *pos = skip_ws(source, *pos);
            if !source[*pos..].starts_with('{') {
                if let Some(brace) = find_in(source, "{", *pos) {
                    *pos = brace;
                } else {
                    return;
                }
            }
            *pos += 1;
            let mut sorted: BTreeMap<&String, &Value> = BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k, v);
            }
            for (key, val) in sorted {
                *pos = skip_ws(source, *pos);
                let quoted = format!("\"{key}\"");
                if let Some(k_start) = find_in(source, &quoted, *pos) {
                    *pos = k_start + quoted.len();
                    *pos = skip_ws(source, *pos);
                    if source[*pos..].starts_with(':') {
                        *pos += 1;
                    }
                    *pos = skip_ws(source, *pos);

                    let mut val_start = *pos;
                    let val_end = find_value_end(source, val_start);
                    *pos = val_end;
                    *pos = skip_ws(source, *pos);
                    if source[*pos..].starts_with(',') {
                        *pos += 1;
                    }

                    let mut child_path = path.to_vec();
                    child_path.push(key.clone());

                    let id = StructuralNode::make_id(file_id, NodeKind::JsonKey, &child_path);
                    let (line_s, line_e) = byte_to_line_range(ls, k_start, val_end);

                    nodes.push(StructuralNode {
                        id,
                        file_id,
                        kind: NodeKind::JsonKey,
                        label: key.clone(),
                        path: child_path.clone(),
                        byte_range: (k_start, val_end),
                        line_range: (line_s, line_e),
                        parent: parent_id,
                        depth,
                    });

                    walk_value(
                        source,
                        val,
                        &child_path,
                        &mut val_start,
                        file_id,
                        ls,
                        nodes,
                        Some(id),
                        depth + 1,
                    );
                }
            }
        }
        Value::Array(arr) => {
            *pos = skip_ws(source, *pos);
            if !source[*pos..].starts_with('[') {
                if let Some(bracket) = find_in(source, "[", *pos) {
                    *pos = bracket;
                } else {
                    return;
                }
            }
            *pos += 1;
            for (idx, item) in arr.iter().enumerate() {
                *pos = skip_ws(source, *pos);
                let mut item_start = *pos;
                let item_end = find_value_end(source, item_start);
                *pos = item_end;
                *pos = skip_ws(source, *pos);
                if source[*pos..].starts_with(',') {
                    *pos += 1;
                }

                let mut child_path = path.to_vec();
                child_path.push(idx.to_string());

                let id = StructuralNode::make_id(file_id, NodeKind::JsonKey, &child_path);
                let (line_s, line_e) = byte_to_line_range(ls, item_start, item_end);

                nodes.push(StructuralNode {
                    id,
                    file_id,
                    kind: NodeKind::JsonKey,
                    label: format!("[{idx}]"),
                    path: child_path.clone(),
                    byte_range: (item_start, item_end),
                    line_range: (line_s, line_e),
                    parent: parent_id,
                    depth,
                });

                walk_value(
                    source,
                    item,
                    &child_path,
                    &mut item_start,
                    file_id,
                    ls,
                    nodes,
                    Some(id),
                    depth + 1,
                );
            }
        }
        _ => {}
    }
}

fn assign_parents(nodes: &mut [StructuralNode]) {
    let n = nodes.len();
    for i in 0..n {
        let my_range = nodes[i].byte_range;
        let my_depth = nodes[i].depth;
        let my_path = nodes[i].path.clone();
        for j in (0..i).rev() {
            let other_range = nodes[j].byte_range;
            if other_range.0 <= my_range.0
                && my_range.1 <= other_range.1
                && nodes[j].depth < my_depth
                && my_path.starts_with(&nodes[j].path)
                && nodes[j].path.len() + 1 == nodes[i].path.len()
            {
                nodes[i].parent = Some(nodes[j].id);
                break;
            }
        }
    }
}

/// Parse a JSON source into structural nodes.
pub fn parse(file_id: u64, source: &str) -> Vec<StructuralNode> {
    let ls = line_starts(source);
    let value: Value = match serde_json::from_str(source) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut nodes = Vec::new();
    let mut pos = 0;
    walk_value(
        source,
        &value,
        &[],
        &mut pos,
        file_id,
        &ls,
        &mut nodes,
        None,
        0,
    );
    assign_parents(&mut nodes);
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural::NodeKind;

    const SAMPLE: &str = r#"{
  "database": {
    "host": "localhost",
    "port": 5432
  },
  "items": [1, 2, 3]
}"#;

    #[test]
    fn extracts_top_level_keys() {
        let nodes = parse(1, SAMPLE);
        let top: Vec<&StructuralNode> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::JsonKey && n.parent.is_none())
            .collect();
        let labels: Vec<&str> = top.iter().map(|n| n.label.as_str()).collect();
        assert!(
            labels.contains(&"database"),
            "should find database key, got {labels:?}"
        );
        assert!(
            labels.contains(&"items"),
            "should find items key, got {labels:?}"
        );
    }

    #[test]
    fn extracts_nested_key() {
        let nodes = parse(1, SAMPLE);
        let host = nodes
            .iter()
            .find(|n| n.label == "host" && n.kind == NodeKind::JsonKey)
            .unwrap();
        assert!(host.parent.is_some(), "host should have a parent");
        let parent = nodes.iter().find(|n| n.id == host.parent.unwrap()).unwrap();
        assert_eq!(parent.label, "database");
    }

    #[test]
    fn extracts_array_index() {
        let nodes = parse(1, SAMPLE);
        let array_nodes: Vec<&StructuralNode> = nodes
            .iter()
            .filter(|n| n.label == "[0]" || n.label == "[1]" || n.label == "[2]")
            .collect();
        assert_eq!(array_nodes.len(), 3, "expected 3 array element nodes");
    }
}
