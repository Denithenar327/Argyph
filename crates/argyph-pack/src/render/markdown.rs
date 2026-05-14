use camino::Utf8PathBuf;

/// Render a set of packed files in the human-readable markdown format.
///
/// Each file produces a level-2 heading with path and metadata, followed
/// by a fenced code block using the detected language from the file
/// extension.
pub fn render_markdown(files: &[(Utf8PathBuf, &str, bool, usize)], repo_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Repository: {repo_name}\n\n"));

    for (path, content, truncated, token_count) in files {
        let lang = lang_from_ext(path);
        out.push_str(&format!(
            "## File: {path} (tokens: {token_count}, truncated: {truncated})\n\n"
        ));
        out.push_str(&format!("```{lang}\n{content}\n```\n\n"));
    }

    out
}

/// Map a file extension to a markdown code-fence language identifier.
fn lang_from_ext(path: &camino::Utf8Path) -> &str {
    match path.extension().unwrap_or("") {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" => "javascript",
        "jsx" => "jsx",
        "py" | "pyi" | "pyx" => "python",
        "md" | "mdx" => "markdown",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "sql" => "sql",
        "sh" | "bash" | "zsh" => "bash",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => "cpp",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        "fs" | "fsx" => "fsharp",
        "scala" | "sc" => "scala",
        "xml" | "svg" => "xml",
        "r" => "r",
        "lua" => "lua",
        "zig" => "zig",
        "ex" | "exs" => "elixir",
        "erl" | "hrl" => "erlang",
        "hs" => "haskell",
        "ml" | "mli" => "ocaml",
        "dockerfile" => "dockerfile",
        "makefile" | "mk" => "makefile",
        "cmake" => "cmake",
        "proto" => "protobuf",
        "graphql" | "gql" => "graphql",
        "vue" => "vue",
        "svelte" => "svelte",
        "tf" | "tfvars" => "terraform",
        _ => "",
    }
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
    fn empty_file_list_produces_only_header() {
        let result = render_markdown(&[], "my-repo");
        assert_eq!(result.trim(), "# Repository: my-repo");
    }

    #[test]
    fn single_file_has_heading_and_code_block() {
        let files = [(p("src/main.rs"), "fn main() {}", false, 4)];
        let result = render_markdown(&files, "test");
        assert!(result.contains("## File: src/main.rs (tokens: 4, truncated: false)"));
        assert!(result.contains("```rust"));
        assert!(result.contains("fn main() {}"));
        assert!(result.contains("```"));
    }

    #[test]
    fn truncated_flag_is_written() {
        let files = [(p("lib.ts"), "export const x = 1;", true, 3)];
        let result = render_markdown(&files, "repo");
        assert!(result.contains("truncated: true"));
    }

    #[test]
    fn multiple_files_in_order() {
        let files = [(p("a.rs"), "// a", false, 1), (p("b.py"), "# b", false, 1)];
        let result = render_markdown(&files, "repo");
        let a_pos = result.find("a.rs").unwrap();
        let b_pos = result.find("b.py").unwrap();
        assert!(a_pos < b_pos);
    }

    #[test]
    fn lang_detection_common_extensions() {
        assert_eq!(lang_from_ext(camino::Utf8Path::new("foo.rs")), "rust");
        assert_eq!(lang_from_ext(camino::Utf8Path::new("foo.ts")), "typescript");
        assert_eq!(lang_from_ext(camino::Utf8Path::new("foo.py")), "python");
        assert_eq!(lang_from_ext(camino::Utf8Path::new("foo.md")), "markdown");
        assert_eq!(lang_from_ext(camino::Utf8Path::new("foo.toml")), "toml");
        assert_eq!(lang_from_ext(camino::Utf8Path::new("foo.json")), "json");
    }

    #[test]
    fn unknown_extension_returns_empty() {
        assert_eq!(lang_from_ext(camino::Utf8Path::new("foo.unknown_ext")), "");
        assert_eq!(lang_from_ext(camino::Utf8Path::new("no_extension")), "");
    }
}
