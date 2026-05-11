use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;

use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;

use crate::error::{self, McpErrorBody};
use crate::validate;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    #[serde(default = "default_max_tree_depth")]
    pub max_tree_depth: u64,
}

fn default_max_tree_depth() -> u64 {
    3
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LanguageSummary {
    pub name: String,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GitInfo {
    pub branch: String,
    pub head_short: String,
    pub dirty: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<LanguageSummary>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_points: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub readme_excerpt: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitInfo>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    pub fn ok(
        languages: Vec<LanguageSummary>,
        entry_points: Vec<String>,
        readme_excerpt: String,
        tree: String,
        git: Option<GitInfo>,
    ) -> Self {
        Self {
            languages: Some(languages),
            entry_points: Some(entry_points),
            readme_excerpt: Some(readme_excerpt),
            tree: Some(tree),
            git,
            error: None,
        }
    }

    pub fn err(body: McpErrorBody) -> Self {
        Self {
            languages: None,
            entry_points: None,
            readme_excerpt: None,
            tree: None,
            git: None,
            error: Some(body),
        }
    }
}

pub async fn handle(
    supervisor: &Arc<Supervisor>,
    root: &Utf8PathBuf,
    request: Request,
) -> Response {
    let index = supervisor.index();
    let files = match index.list_files().await {
        Ok(f) => f,
        Err(_e) => return Response::err(error::index_not_ready()),
    };

    let depth = validate::clamp_u64(request.max_tree_depth, 1, 6) as usize;
    let tree = build_tree(&files, depth);

    let mut lang_counts: HashMap<String, u64> = HashMap::new();
    for f in &files {
        if let Some(lang) = &f.language {
            *lang_counts.entry(lang.to_string()).or_default() += 1;
        }
    }
    let mut languages: Vec<LanguageSummary> = lang_counts
        .into_iter()
        .map(|(name, count)| LanguageSummary { name, files: count })
        .collect();
    languages.sort_by(|a, b| b.files.cmp(&a.files));

    let entry_points = ["src/main.rs", "src/lib.rs", "main.rs", "lib.rs"]
        .iter()
        .filter(|p| files.iter().any(|f| f.path.as_str() == **p))
        .map(|s| s.to_string())
        .collect();

    let readme_excerpt = read_readme(root);
    let git = get_git_info(root);

    Response::ok(languages, entry_points, readme_excerpt, tree, git)
}

fn build_tree(files: &[argyph_fs::FileEntry], depth: usize) -> String {
    let mut paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    paths.sort();
    paths.truncate(500);
    let mut out = String::new();
    let mut prev: Vec<&str> = vec![];
    for path in &paths {
        let parts: Vec<&str> = path.split('/').collect();
        let common = prev.iter().zip(&parts).filter(|(a, b)| a == b).count();
        if common < depth {
            for (i, part) in parts.iter().enumerate().skip(common).take(depth - common) {
                let indent = "  ".repeat(i);
                out.push_str(&format!("{indent}{part}/\n"));
            }
        }
        prev = parts;
    }
    out
}

fn read_readme(root: &camino::Utf8Path) -> String {
    for name in &["README.md", "README", "readme.md"] {
        let path = root.join(name);
        if let Ok(content) = std::fs::read_to_string(path.as_str()) {
            return content.lines().take(10).collect::<Vec<_>>().join("\n");
        }
    }
    String::new()
}

fn get_git_info(root: &camino::Utf8Path) -> Option<GitInfo> {
    let git_dir = root.join(".git");
    if !git_dir.exists() {
        return None;
    }
    let run = |args: &[&str]| -> Option<String> {
        Command::new("git")
            .args(args)
            .current_dir(root.as_str())
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let branch = run(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    let head_short = run(&["rev-parse", "--short", "HEAD"])?;
    let dirty = Command::new("git")
        .args(["diff", "--quiet"])
        .current_dir(root.as_str())
        .status()
        .ok()
        .map(|s| !s.success())?;
    Some(GitInfo {
        branch,
        head_short,
        dirty,
    })
}
