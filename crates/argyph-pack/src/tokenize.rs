use crate::PackError;

/// Estimates token counts using the `cl100k_base` tokenizer (GPT-4 / Claude
/// compatible). Accuracy is within ~5% across non-OpenAI providers; the budget
/// is a soft guarantee.
pub struct TokenCounter {
    bpe: tiktoken_rs::CoreBPE,
}

impl TokenCounter {
    /// Create a new token counter backed by the `cl100k_base` encoding.
    ///
    /// # Errors
    ///
    /// Returns [`PackError::Io`] if the tokenizer cannot be loaded (should only
    /// happen if the `tiktoken-rs` data files are missing).
    pub fn new() -> Result<Self, PackError> {
        let bpe = tiktoken_rs::cl100k_base().map_err(|e| PackError::Io(e.to_string()))?;
        Ok(Self { bpe })
    }

    /// Count the number of tokens in a UTF-8 string.
    pub fn count(&self, text: &str) -> usize {
        self.bpe.encode_ordinary(text).len()
    }

    /// Count the number of tokens in a byte slice, handling non-UTF-8 data
    /// gracefully via lossy conversion.
    pub fn count_bytes(&self, text: &[u8]) -> usize {
        let s = String::from_utf8_lossy(text);
        self.count(&s)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_is_zero_tokens() {
        let tc = TokenCounter::new().unwrap();
        assert_eq!(tc.count(""), 0);
    }

    #[test]
    fn simple_english_sentence() {
        let tc = TokenCounter::new().unwrap();
        let n = tc.count("Hello, world!");
        assert!((3..=6).contains(&n), "expected 3-6 tokens, got {n}");
    }

    #[test]
    fn rust_function_body() {
        let tc = TokenCounter::new().unwrap();
        let code = "fn main() {\n    println!(\"Hello\");\n}\n";
        let n = tc.count(code);
        assert!(n > 5, "expected >5 tokens for a small function, got {n}");
        assert!(n < 30, "expected <30 tokens, got {n}");
    }

    #[test]
    fn count_bytes_falls_back_to_lossy() {
        let tc = TokenCounter::new().unwrap();
        // Invalid UTF-8 bytes
        let n = tc.count_bytes(&[0x48, 0x65, 0x6c, 0x6c, 0x6f]);
        assert!(n > 0, "expected >0 tokens for 'Hello' bytes");
    }

    #[test]
    fn longer_text_is_more_tokens() {
        let tc = TokenCounter::new().unwrap();
        let short = tc.count("fn");
        let long = tc.count("fn foo(x: i32) -> i32 { x + 1 }");
        assert!(long > short, "longer text should have more tokens");
    }

    #[test]
    fn token_count_is_repeatable() {
        let tc = TokenCounter::new().unwrap();
        let text = "fn factorial(n: u64) -> u64 { if n <= 1 { 1 } else { n * factorial(n - 1) } }";
        let n1 = tc.count(text);
        let n2 = tc.count(text);
        assert_eq!(n1, n2);
    }
}
