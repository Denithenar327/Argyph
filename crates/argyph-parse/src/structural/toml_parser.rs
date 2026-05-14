use super::{byte_to_line_range, line_starts, NodeKind, StructuralNode};

#[allow(dead_code, unused_imports)]
use toml::Spanned;

fn find_toml_key_span(source: &str, key: &str, start: usize) -> Option<(usize, usize)> {
    let rest = &source[start..];
    let pattern = format!("{key} =");
    rest.find(&pattern).map(|off| {
        let abs = start + off;
        let end = abs + key.len();
        (abs, end)
    })
}

fn find_line_starting_with(source: &str, prefix: &str, start: usize) -> Option<usize> {
    let rest = &source[start..];
    let mut pos = 0;
    for line in rest.lines() {
        if line.trim_start().starts_with(prefix) {
            return Some(start + pos);
        }
        pos += line.len() + 1;
        if pos >= rest.len() {
            break;
        }
    }
    None
}

fn next_section_or_eof(source: &str, start: usize) -> usize {
    let rest = &source[start..];
    let mut pos = 0;
    for line in rest.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') && !trimmed.starts_with("[[") {
            return start + pos;
        }
        pos += line.len() + 1;
        if pos >= rest.len() {
            break;
        }
    }
    source.len()
}

fn assign_parents(nodes: &mut [StructuralNode]) {
    for i in 0..nodes.len() {
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

/// Parse a TOML source into structural nodes.
pub fn parse(file_id: u64, source: &str) -> Vec<StructuralNode> {
    let ls = line_starts(source);
    let table: toml::Table = match toml::from_str(source) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let mut nodes = Vec::new();
    let mut scan_pos = 0usize;

    let mut section_order: Vec<(String, usize, usize)> = Vec::new();
    {
        let rest = source;
        let mut pos = 0;
        for line in rest.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('[') && !trimmed.starts_with("[[") {
                let section_name = trimmed
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .trim()
                    .to_string();
                let start = source[..pos].rfind('\n').map_or(0, |n| n + 1);
                section_order.push((section_name, start, pos));
            }
            pos += line.len() + 1;
            if pos >= source.len() {
                break;
            }
        }
    }

    for (key, value) in &table {
        if value.is_table() {
            let section_start =
                find_line_starting_with(source, &format!("[{key}]"), 0).unwrap_or(scan_pos);
            let section_end = next_section_or_eof(source, section_start + 1);

            let path = vec![key.clone()];
            let id = StructuralNode::make_id(file_id, NodeKind::TomlKey, &path);
            let (line_s, line_e) = byte_to_line_range(&ls, section_start, section_end);

            nodes.push(StructuralNode {
                id,
                file_id,
                kind: NodeKind::TomlKey,
                label: key.clone(),
                path: path.clone(),
                byte_range: (section_start, section_end),
                line_range: (line_s, line_e),
                parent: None,
                depth: 0,
            });

            if let Some(inner) = value.as_table() {
                let inner_scan = section_start + key.len() + 3;
                let mut inner_pos = inner_scan;
                for (ik, _iv) in inner {
                    if let Some((k_start, k_end)) = find_toml_key_span(source, ik, inner_pos) {
                        let line_end = source[k_end..]
                            .find('\n')
                            .map_or(source.len(), |off| k_end + off);
                        let mut child_path = path.clone();
                        child_path.push(ik.clone());

                        let cid = StructuralNode::make_id(file_id, NodeKind::TomlKey, &child_path);
                        let (cl_s, cl_e) = byte_to_line_range(&ls, k_start, line_end);
                        nodes.push(StructuralNode {
                            id: cid,
                            file_id,
                            kind: NodeKind::TomlKey,
                            label: ik.clone(),
                            path: child_path,
                            byte_range: (k_start, line_end),
                            line_range: (cl_s, cl_e),
                            parent: None,
                            depth: 1,
                        });
                        inner_pos = line_end + 1;
                    }
                }
            }
        } else if let Some((k_start, k_end)) = find_toml_key_span(source, key, scan_pos) {
            let line_end = source[k_end..]
                .find('\n')
                .map_or(source.len(), |off| k_end + off);
            let path = vec![key.clone()];
            let id = StructuralNode::make_id(file_id, NodeKind::TomlKey, &path);
            let (line_s, line_e) = byte_to_line_range(&ls, k_start, line_end);

            nodes.push(StructuralNode {
                id,
                file_id,
                kind: NodeKind::TomlKey,
                label: key.clone(),
                path,
                byte_range: (k_start, line_end),
                line_range: (line_s, line_e),
                parent: None,
                depth: 0,
            });
            scan_pos = line_end + 1;
        }
    }

    assign_parents(&mut nodes);
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural::NodeKind;

    const SAMPLE: &str =
        "title = \"x\"\n\n[server]\nhost = \"localhost\"\nport = 8080\n\n[logging]\nlevel = \"info\"\n";

    #[test]
    fn extracts_top_level_bare_key() {
        let nodes = parse(1, SAMPLE);
        let title = nodes.iter().find(|n| n.label == "title");
        assert!(title.is_some(), "should find bare key 'title'");
        assert_eq!(title.unwrap().kind, NodeKind::TomlKey);
    }

    #[test]
    fn extracts_section_and_keys() {
        let nodes = parse(1, SAMPLE);
        let server = nodes
            .iter()
            .find(|n| n.label == "server" && n.kind == NodeKind::TomlKey)
            .unwrap();
        let host = nodes
            .iter()
            .find(|n| n.label == "host" && n.kind == NodeKind::TomlKey)
            .unwrap();
        assert!(
            host.parent.is_some() || host.byte_range.0 >= server.byte_range.0,
            "host should be within server section"
        );
        let logging = nodes
            .iter()
            .find(|n| n.label == "logging" && n.kind == NodeKind::TomlKey)
            .unwrap();
        let level = nodes
            .iter()
            .find(|n| n.label == "level" && n.kind == NodeKind::TomlKey)
            .unwrap();
        assert!(
            level.parent.is_some() || level.byte_range.0 >= logging.byte_range.0,
            "level should be within logging section"
        );
    }
}
