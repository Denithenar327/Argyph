use crate::tokenize::TokenCounter;

/// Truncate file content to fit within `max_tokens`, keeping only the first
/// N lines whose cumulative token count stays under the budget. A truncation
/// marker comment is appended when truncation occurs.
///
/// Returns `(truncated_content, actual_token_count)`. When even the marker
/// exceeds `max_tokens`, returns an empty string with a count of 0 (callers
/// should treat this as an omitted file).
pub fn truncate_file(
    content: &str,
    max_tokens: usize,
    counter: &TokenCounter,
) -> (String, usize) {
    let full_count = counter.count(content);
    if full_count <= max_tokens {
        return (content.to_string(), full_count);
    }

    let marker = "\n// ... [truncated] ...\n";
    let marker_tokens = counter.count(marker);

    if marker_tokens >= max_tokens {
        return (String::new(), 0);
    }

    let available = max_tokens.saturating_sub(marker_tokens);

    let mut cumulative = 0usize;
    let mut kept_lines: Vec<&str> = Vec::new();

    for line in content.lines() {
        let line_tokens = counter.count(line);
        if cumulative + line_tokens + 1 > available {
            break;
        }
        kept_lines.push(line);
        cumulative += line_tokens + 1;
    }

    if kept_lines.is_empty() {
        return (marker.to_string(), marker_tokens);
    }

    let mut result = kept_lines.join("\n");
    result.push_str(marker);
    let count = counter.count(&result);
    (result, count)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn no_truncation_when_under_budget() {
        let tc = TokenCounter::new().unwrap();
        let content = "fn main() {}\n";
        let budget = tc.count(content) + 10;
        let (result, count) = truncate_file(content, budget, &tc);
        assert_eq!(result, content);
        assert!(count <= budget);
    }

    #[test]
    fn truncates_when_over_budget() {
        let tc = TokenCounter::new().unwrap();
        let content: String = (0..50)
            .map(|i| format!("fn function_{i}() {{ /* body */ }}"))
            .collect::<Vec<_>>()
            .join("\n");
        let full = tc.count(&content);
        let budget = full / 2;
        let (result, count) = truncate_file(&content, budget, &tc);
        assert!(count <= budget, "count {count} exceeds budget {budget}");
        assert!(result.contains("[truncated"));
        assert!(result.len() < content.len());
    }

    #[test]
    fn marker_only_when_no_lines_fit() {
        let tc = TokenCounter::new().unwrap();
        let content = "a very long line that would take many tokens to encode etc etc\n";
        let marker = "\n// ... [truncated] ...\n";
        let marker_tokens = tc.count(marker);
        let (result, _count) = truncate_file(content, marker_tokens + 5, &tc);
        assert!(result.contains("[truncated"));
    }

    #[test]
    fn budget_smaller_than_marker_returns_empty() {
        let tc = TokenCounter::new().unwrap();
        let content = "x".repeat(1000);
        let (result, count) = truncate_file(&content, 0, &tc);
        assert_eq!(result, "");
        assert_eq!(count, 0);
    }

    #[test]
    fn truncation_is_deterministic() {
        let tc = TokenCounter::new().unwrap();
        let content: String = (0..20)
            .map(|i| format!("pub fn foo_{i}() -> i32 {{ {i} }}"))
            .collect::<Vec<_>>()
            .join("\n");
        let full = tc.count(&content);
        let budget = full / 2;
        let (r1, c1) = truncate_file(&content, budget, &tc);
        let (r2, c2) = truncate_file(&content, budget, &tc);
        assert_eq!(r1, r2);
        assert_eq!(c1, c2);
    }
}
