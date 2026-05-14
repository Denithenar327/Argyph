use super::{byte_to_line_range, line_starts, NodeKind, StructuralNode};
use serde_yaml::Value;

fn indent_of(source: &str, pos: usize) -> usize {
    let line_start = source[..pos].rfind('\n').map_or(0, |l| l + 1);
    let line = &source[line_start..pos];
    line.chars().take_while(|c| *c == ' ').count()
}

fn find_yaml_sibling(source: &str, key: &str, start: usize) -> Option<usize> {
    let indent = indent_of(source, start);
    let rest = &source[start..];
    let mut pos = 0;
    for line in rest.lines() {
        let line_indent = line.chars().take_while(|c| *c == ' ').count();
        if line_indent == indent {
            let trimmed = line.trim_start();
            if let Some(colon_pos) = trimmed.find(':') {
                let candidate = trimmed[..colon_pos].trim_end();
                if candidate == key {
                    return Some(start + pos + (line.len() - trimmed.len()));
                }
            }
        }
        pos += line.len() + 1;
        if pos >= rest.len() {
            break;
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn walk_mapping(
    source: &str,
    mapping: &serde_yaml::Mapping,
    path: &[String],
    start_scan: usize,
    file_id: u64,
    ls: &[usize],
    nodes: &mut Vec<StructuralNode>,
    parent_id: Option<super::NodeId>,
    depth: u32,
) {
    let mut scan_pos = start_scan;

    for (key_val, val) in mapping {
        let key = match key_val {
            Value::String(s) => s.clone(),
            other => format!("{other:?}"),
        };

        let key_start = find_yaml_sibling(source, &key, scan_pos).unwrap_or(scan_pos);
        scan_pos = key_start + key.len();

        let val_end = match val {
            Value::Mapping(_) => {
                let after_colon = &source[scan_pos..];
                if let Some(nl) = after_colon.find('\n') {
                    scan_pos + nl + 1
                } else {
                    source.len()
                }
            }
            _ => {
                let rest = &source[scan_pos..];
                rest.find('\n').map_or(source.len(), |nl| scan_pos + nl)
            }
        };

        let mut child_path = path.to_vec();
        child_path.push(key.clone());

        let id = StructuralNode::make_id(file_id, NodeKind::YamlKey, &child_path);
        let (line_s, line_e) = byte_to_line_range(ls, key_start, val_end);

        nodes.push(StructuralNode {
            id,
            file_id,
            kind: NodeKind::YamlKey,
            label: key.clone(),
            path: child_path.clone(),
            byte_range: (key_start, val_end),
            line_range: (line_s, line_e),
            parent: parent_id,
            depth,
        });

        if let Value::Mapping(inner) = val {
            let inner_start = val_end;
            walk_mapping(
                source,
                inner,
                &child_path,
                inner_start,
                file_id,
                ls,
                nodes,
                Some(id),
                depth + 1,
            );
        }

        scan_pos = val_end;
    }
}

fn assign_parents(nodes: &mut [StructuralNode]) {
    let n = nodes.len();
    for i in 0..n {
        let my_range = nodes[i].byte_range;
        let my_depth = nodes[i].depth;
        for j in (0..i).rev() {
            let other_range = nodes[j].byte_range;
            if other_range.0 <= my_range.0
                && my_range.1 <= other_range.1
                && nodes[j].depth < my_depth
            {
                nodes[i].parent = Some(nodes[j].id);
                break;
            }
        }
    }
}

/// Parse a YAML source into structural nodes.
pub fn parse(file_id: u64, source: &str) -> Vec<StructuralNode> {
    let ls = line_starts(source);
    let value: Value = match serde_yaml::from_str(source) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut nodes = Vec::new();
    if let Value::Mapping(map) = &value {
        walk_mapping(source, map, &[], 0, file_id, &ls, &mut nodes, None, 0);
    }
    assign_parents(&mut nodes);
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural::NodeKind;

    const SAMPLE: &str = "server:\n  host: localhost\n  port: 8080\nlogging:\n  level: info\n";

    #[test]
    fn extracts_top_level_keys() {
        let nodes = parse(1, SAMPLE);
        let top: Vec<&StructuralNode> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::YamlKey && n.parent.is_none())
            .collect();
        let labels: Vec<&str> = top.iter().map(|n| n.label.as_str()).collect();
        assert!(
            labels.contains(&"server"),
            "should find server, got {labels:?}"
        );
        assert!(
            labels.contains(&"logging"),
            "should find logging, got {labels:?}"
        );
    }

    #[test]
    fn extracts_nested_keys() {
        let nodes = parse(1, SAMPLE);
        let host = nodes
            .iter()
            .find(|n| n.label == "host" && n.kind == NodeKind::YamlKey)
            .unwrap();
        assert!(host.parent.is_some());
        let parent = nodes.iter().find(|n| n.id == host.parent.unwrap()).unwrap();
        assert_eq!(parent.label, "server");

        let level = nodes
            .iter()
            .find(|n| n.label == "level" && n.kind == NodeKind::YamlKey)
            .unwrap();
        assert!(level.parent.is_some());
        let parent = nodes
            .iter()
            .find(|n| n.id == level.parent.unwrap())
            .unwrap();
        assert_eq!(parent.label, "logging");
    }
}
