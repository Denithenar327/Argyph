use super::{byte_to_line_range, line_starts, NodeKind, StructuralNode};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

fn heading_level_num(level: &HeadingLevel) -> u32 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Parse a markdown source into structural nodes (sections and code blocks).
pub fn parse(file_id: u64, source: &str) -> Vec<StructuralNode> {
    let ls = line_starts(source);
    let events: Vec<(Event<'_>, std::ops::Range<usize>)> =
        Parser::new(source).into_offset_iter().collect();

    struct HeadingInfo {
        level: u32,
        title: String,
        start_byte: usize,
    }

    let mut headings: Vec<HeadingInfo> = Vec::new();
    let eof = events.last().map_or(source.len(), |(_, range)| range.end);

    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::Heading { level, .. }) = &events[i].0 {
            let h_level = heading_level_num(level);
            let start = events[i].1.start;
            let mut title = String::new();
            let mut j = i + 1;
            while j < events.len() && !matches!(&events[j].0, Event::End(TagEnd::Heading(..))) {
                match &events[j].0 {
                    Event::Text(t) | Event::Code(t) => title.push_str(t.as_ref()),
                    _ => {}
                }
                j += 1;
            }
            headings.push(HeadingInfo {
                level: h_level,
                title: title.clone(),
                start_byte: start,
            });
            i = j + 1;
        } else {
            i += 1;
        }
    }

    let mut nodes: Vec<StructuralNode> = Vec::new();

    for (idx, h) in headings.iter().enumerate() {
        let section_end = if idx + 1 < headings.len() {
            let mut end = eof;
            for next in &headings[idx + 1..] {
                if next.level <= h.level {
                    end = next.start_byte;
                    break;
                }
            }
            end
        } else {
            eof
        };

        let path = vec![h.title.clone()];
        let id = StructuralNode::make_id(file_id, NodeKind::MdSection, &path);
        let (line_s, line_e) = byte_to_line_range(&ls, h.start_byte, section_end);

        nodes.push(StructuralNode {
            id,
            file_id,
            kind: NodeKind::MdSection,
            label: h.title.clone(),
            path,
            byte_range: (h.start_byte, section_end),
            line_range: (line_s, line_e),
            parent: None,
            depth: h.level,
        });
    }

    i = 0;
    while i < events.len() {
        if let Event::Start(Tag::CodeBlock(kind)) = &events[i].0 {
            let info = match kind {
                pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.as_ref().to_string(),
                pulldown_cmark::CodeBlockKind::Indented => String::new(),
            };
            let start = events[i].1.start;
            let mut j = i + 1;
            while j < events.len() && !matches!(&events[j].0, Event::End(TagEnd::CodeBlock)) {
                j += 1;
            }
            let end = if j < events.len() {
                events[j].1.end
            } else {
                start
            };

            let label = if info.is_empty() {
                "code".to_string()
            } else {
                info.clone()
            };
            let path = vec![label.clone()];
            let id = StructuralNode::make_id(file_id, NodeKind::MdCodeBlock, &path);
            let (line_s, line_e) = byte_to_line_range(&ls, start, end);

            nodes.push(StructuralNode {
                id,
                file_id,
                kind: NodeKind::MdCodeBlock,
                label,
                path,
                byte_range: (start, end),
                line_range: (line_s, line_e),
                parent: None,
                depth: 0,
            });
            i = j + 1;
        } else {
            i += 1;
        }
    }

    assign_parents(&mut nodes);
    nodes
}

fn assign_parents(nodes: &mut [StructuralNode]) {
    let n = nodes.len();
    for i in 0..n {
        if nodes[i].kind != NodeKind::MdSection {
            continue;
        }
        let depth = nodes[i].depth;
        for j in (0..i).rev() {
            if nodes[j].kind == NodeKind::MdSection && nodes[j].depth < depth {
                nodes[i].parent = Some(nodes[j].id);
                break;
            }
        }
    }

    let parent_info: Vec<(usize, Option<super::NodeId>, u32)> = nodes
        .iter()
        .enumerate()
        .filter(|(_, child)| child.kind == NodeKind::MdCodeBlock)
        .map(|(ci, child)| {
            let parent = nodes
                .iter()
                .rev()
                .find(|n| n.kind == NodeKind::MdSection && n.byte_range.0 <= child.byte_range.0);
            let pid = parent.map(|p| p.id);
            let new_depth = parent.map_or(0, |p| p.depth + 1);
            (ci, pid, new_depth)
        })
        .collect();

    for (ci, pid, new_depth) in parent_info {
        nodes[ci].parent = pid;
        nodes[ci].depth = new_depth;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural::NodeKind;

    const SAMPLE: &str =
        "# Top\n\nintro\n\n## Sub A\n\nbody a\n\n## Sub B\n\n```rust\nfn x() {}\n```\n\nbody b\n";

    #[test]
    fn extracts_nested_headings() {
        let nodes = parse(1, SAMPLE);
        let sections: Vec<&StructuralNode> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::MdSection)
            .collect();
        assert!(sections.len() >= 3, "expected at least 3 sections");

        let top = sections.iter().find(|n| n.label == "Top").unwrap();
        assert_eq!(top.depth, 1);
        assert!(top.parent.is_none());

        let sub_a = sections.iter().find(|n| n.label == "Sub A").unwrap();
        assert_eq!(sub_a.depth, 2);
        assert_eq!(sub_a.parent, Some(top.id));

        let sub_b = sections.iter().find(|n| n.label == "Sub B").unwrap();
        assert_eq!(sub_b.depth, 2);
        assert_eq!(sub_b.parent, Some(top.id));
    }

    #[test]
    fn extracts_code_block() {
        let nodes = parse(1, SAMPLE);
        let code: Vec<&StructuralNode> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::MdCodeBlock)
            .collect();
        assert!(!code.is_empty(), "expected at least one code block");
        let cb = code[0];
        assert!(
            cb.label.contains("rust"),
            "label should mention rust: {}",
            cb.label
        );
    }

    #[test]
    fn section_byte_range_covers_body() {
        let nodes = parse(1, SAMPLE);
        let sub_a = nodes
            .iter()
            .find(|n| n.label == "Sub A" && n.kind == NodeKind::MdSection)
            .unwrap();
        let body_range = &SAMPLE[sub_a.byte_range.0..sub_a.byte_range.1];
        assert!(
            body_range.contains("body a"),
            "Section Sub A should cover 'body a', got: {body_range:?}"
        );
    }
}
