use std::io::BufRead;
use std::sync::Arc;

use camino::Utf8PathBuf;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use argyph_core::Supervisor;

use crate::error::{self, McpErrorBody};
use crate::types::Filter;
use crate::validate;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Request {
    pub pattern: String,

    #[serde(default)]
    pub regex: bool,

    #[serde(default)]
    pub case_sensitive: bool,

    #[serde(default = "default_max_results")]
    pub max_results: u64,

    #[serde(default)]
    pub filter: Option<Filter>,
}

fn default_max_results() -> u64 {
    100
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchHit {
    pub file: String,
    pub line: u64,
    pub column: u64,
    #[serde(rename = "match")]
    pub match_text: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hits: Option<Vec<SearchHit>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorBody>,
}

impl Response {
    pub fn ok(hits: Vec<SearchHit>, truncated: bool) -> Self {
        Self {
            hits: Some(hits),
            truncated: Some(truncated),
            error: None,
        }
    }

    pub fn err(body: McpErrorBody) -> Self {
        Self {
            hits: None,
            truncated: None,
            error: Some(body),
        }
    }
}

pub async fn handle(
    supervisor: &Arc<Supervisor>,
    root: &Utf8PathBuf,
    request: Request,
) -> Response {
    if !supervisor.get_tier_state().await.is_ready() {
        return Response::err(error::index_not_ready());
    }

    let max_results = validate::clamp_u64(request.max_results, 1, 1000);

    let re = build_pattern(&request.pattern, request.regex, request.case_sensitive);
    let re = match re {
        Ok(r) => r,
        Err(e) => return Response::err(error::internal(e)),
    };

    let index = supervisor.index();
    let files = match index.list_files().await {
        Ok(f) => f,
        Err(_e) => return Response::err(error::index_not_ready()),
    };

    let files: Vec<_> = files
        .into_iter()
        .filter(|f| match &request.filter {
            Some(filt) => {
                let path = f.path.as_str();
                let globs_ok = filt
                    .paths_glob
                    .as_ref()
                    .is_none_or(|globs| globs.iter().any(|g| glob_match(g, path)));
                let excludes_ok = filt
                    .exclude_glob
                    .as_ref()
                    .is_none_or(|globs| !globs.iter().any(|g| glob_match(g, path)));
                globs_ok && excludes_ok
            }
            None => true,
        })
        .collect();

    let mut hits = Vec::new();
    'outer: for entry in &files {
        let file_path = root.join(entry.path.as_str());
        let f = match std::fs::File::open(file_path.as_str()) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = std::io::BufReader::new(f);
        for (line_no, line_result) in reader.lines().enumerate() {
            let line = match line_result {
                Ok(l) => l,
                Err(_) => continue,
            };
            for mat in re.find_iter(&line) {
                hits.push(SearchHit {
                    file: entry.path.as_str().to_string(),
                    line: (line_no + 1) as u64,
                    column: (mat.start() + 1) as u64,
                    match_text: mat.as_str().to_string(),
                    context_before: vec![],
                    context_after: vec![],
                });
                if hits.len() >= max_results as usize {
                    break 'outer;
                }
            }
        }
    }

    let total: usize = files
        .iter()
        .filter_map(|f| {
            let fp = root.join(f.path.as_str());
            std::fs::read_to_string(fp.as_str())
                .ok()
                .map(|c| re.find_iter(&c).count())
        })
        .sum();
    let truncated = total > max_results as usize;

    Response::ok(hits, truncated)
}

fn build_pattern(pattern: &str, regex: bool, case_sensitive: bool) -> Result<Regex, String> {
    let mut builder = regex::RegexBuilder::new(pattern);
    builder.case_insensitive(!case_sensitive);
    if !regex {
        let escaped = regex::escape(pattern);
        builder = regex::RegexBuilder::new(&escaped);
    }
    builder.build().map_err(|e| format!("invalid pattern: {e}"))
}

fn glob_match(glob: &str, path: &str) -> bool {
    let cleaned = glob.trim_start_matches('!');
    if let Ok(re) = glob_to_regex(cleaned) {
        re.is_match(path)
    } else {
        path.contains(cleaned)
    }
}

fn glob_to_regex(glob: &str) -> Result<Regex, String> {
    let mut pattern = String::from("^");
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                pattern.push_str(".*");
                i += 1;
            }
            '*' => pattern.push_str("[^/]*"),
            '?' => pattern.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '{' | '}' | '[' | ']' | '\\' => {
                pattern.push('\\');
                pattern.push(chars[i]);
            }
            c => pattern.push(c),
        }
        i += 1;
    }
    pattern.push('$');
    Regex::new(&pattern).map_err(|e| format!("invalid glob '{glob}': {e}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_rg_format_hit() {
        let hit = SearchHit {
            file: "src/main.rs".into(),
            line: 10,
            column: 5,
            match_text: "fn main()".into(),
            context_before: vec![],
            context_after: vec![],
        };
        assert_eq!(hit.file, "src/main.rs");
        assert_eq!(hit.line, 10);
    }
}
