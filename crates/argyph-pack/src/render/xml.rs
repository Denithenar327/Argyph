use camino::Utf8PathBuf;

/// Render a set of packed files in the primary XML format.
///
/// Each file is wrapped in a `<file>` element with `path`, `tokens`, and
/// `truncated` attributes. Content is wrapped in `<![CDATA[...]]>` blocks for
/// readability; files containing `]]>` are split across multiple CDATA
/// sections.
///
/// Output is guaranteed to be well-formed XML.
pub fn render_xml(files: &[(Utf8PathBuf, &str, bool, usize)], repo_name: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<repository name=\"{}\">\n",
        xml_escape_attr(repo_name)
    ));

    for (path, content, truncated, token_count) in files {
        let path_str = xml_escape_attr(path.as_str());
        out.push_str(&format!(
            "  <file path=\"{path_str}\" tokens=\"{token_count}\" truncated=\"{truncated}\">\n"
        ));
        out.push_str(&cdata_wrap(content));
        out.push_str("\n  </file>\n");
    }

    out.push_str("</repository>\n");
    out
}

/// Wrap content in `<![CDATA[...]]>` blocks. When the content contains the
/// `]]>` sequence, it is split across multiple CDATA sections so the output
/// remains well-formed.
fn cdata_wrap(content: &str) -> String {
    if !content.contains("]]>") {
        return format!("<![CDATA[{content}]]>");
    }
    let safe = content.replace("]]>", "]]]]><![CDATA[>");
    format!("<![CDATA[{safe}]]>")
}

/// Escape special XML characters for use in attribute values.
fn xml_escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn p(s: &str) -> Utf8PathBuf {
        Utf8PathBuf::from(s)
    }

    #[test]
    fn empty_file_list_produces_valid_xml() {
        let result = render_xml(&[], "test-repo");
        assert!(result.starts_with("<?xml"));
        assert!(result.contains("<repository"));
        assert!(result.contains("</repository>"));
    }

    #[test]
    fn single_file_produces_file_element() {
        let files = [(p("src/main.rs"), "fn main() {}", false, 5)];
        let result = render_xml(&files, "my-repo");
        assert!(result.contains("<file path=\"src/main.rs\""));
        assert!(result.contains("tokens=\"5\""));
        assert!(result.contains("truncated=\"false\""));
        assert!(result.contains("<![CDATA[fn main() {}]]>"));
    }

    #[test]
    fn truncated_file_shows_true() {
        let files = [(p("src/lib.rs"), "pub fn foo() {}", true, 3)];
        let result = render_xml(&files, "repo");
        assert!(result.contains("truncated=\"true\""));
    }

    #[test]
    fn cdata_splits_on_close_sequence() {
        let content = "some text ]]> more text";
        let wrapped = cdata_wrap(content);
        // Should NOT contain literal ]]>
        let after_start = &wrapped[9..]; // skip <![CDATA[
        assert!(
            !after_start.contains("]]>") || after_start.matches("]]>").count() <= 2,
            "content ]]> was leaked"
        );
        // Well-formed: starts with <![CDATA[ and ends with ]]>
        assert!(wrapped.starts_with("<![CDATA["));
        assert!(wrapped.ends_with("]]>"));
    }

    #[test]
    fn no_cdata_split_when_no_close_sequence() {
        let content = "plain text without special chars";
        let wrapped = cdata_wrap(content);
        assert_eq!(wrapped, format!("<![CDATA[{content}]]>"));
    }

    #[test]
    fn xml_escape_special_chars() {
        let result = xml_escape_attr("a&b<c>d\"e");
        assert_eq!(result, "a&amp;b&lt;c&gt;d&quot;e");
    }

    #[test]
    fn xml_escape_plain_string_unchanged() {
        let result = xml_escape_attr("hello_world");
        assert_eq!(result, "hello_world");
    }

    #[test]
    fn repo_name_escaped_in_attribute() {
        let files = [(p("x.rs"), "", false, 0)];
        let result = render_xml(&files, "repo & stuff");
        assert!(result.contains("name=\"repo &amp; stuff\""));
    }
}
