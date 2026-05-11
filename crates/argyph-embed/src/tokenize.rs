use std::path::Path;

use tokenizers::utils::padding::pad_encodings;
use tokenizers::{PaddingParams, Tokenizer, TruncationDirection};

use crate::error::{EmbedError, Result};

pub struct BertTokenizer {
    inner: Tokenizer,
}

pub struct TokenizedBatch {
    pub input_ids: Vec<i64>,
    pub attention_mask: Vec<i64>,
    pub seq_len: usize,
}

impl BertTokenizer {
    pub fn from_file(path: &Path) -> Result<Self> {
        let inner = Tokenizer::from_file(path).map_err(|e| {
            EmbedError::Config(format!("failed to load tokenizer from {}: {e}", path.display()))
        })?;
        Ok(Self { inner })
    }

    pub fn encode_batch(&self, texts: &[String], max_len: usize) -> Result<TokenizedBatch> {
        if texts.is_empty() {
            return Err(EmbedError::EmptyInput);
        }

        let mut encodings = self.inner.encode_batch(texts.to_vec(), true).map_err(|e| {
            EmbedError::Config(format!("tokenization failed: {e}"))
        })?;

        for enc in &mut encodings {
            if enc.len() > max_len {
                enc.truncate(max_len, 0, TruncationDirection::Right);
            }
        }

        if !encodings.is_empty() {
            let pad = PaddingParams::default();
            pad_encodings(&mut encodings, &pad).map_err(|e| {
                EmbedError::Config(format!("padding failed: {e}"))
            })?;
        }

        let seq_len = encodings.iter().map(|e| e.len()).max().unwrap_or(0);
        let batch_size = texts.len();
        let mut input_ids = Vec::with_capacity(batch_size * seq_len);
        let mut attention_mask = Vec::with_capacity(batch_size * seq_len);

        for enc in &encodings {
            let ids = enc.get_ids();
            let mask = enc.get_attention_mask();
            for i in 0..seq_len {
                input_ids.push(*ids.get(i).unwrap_or(&0) as i64);
                attention_mask.push(*mask.get(i).unwrap_or(&0) as i64);
            }
        }

        Ok(TokenizedBatch {
            input_ids,
            attention_mask,
            seq_len,
        })
    }

    pub fn mean_pool(
        last_hidden_state: &[f32],
        attention_mask: &[i64],
        batch_size: usize,
        seq_len: usize,
        hidden_size: usize,
    ) -> Vec<Vec<f32>> {
        let mut result = vec![vec![0.0f32; hidden_size]; batch_size];

        for (b, out_vec) in result.iter_mut().enumerate() {
            let mut sum = vec![0.0f32; hidden_size];
            let mut count = 0.0f32;

            for s in 0..seq_len {
                if attention_mask[b * seq_len + s] != 0 {
                    let offset = (b * seq_len + s) * hidden_size;
                    for (h, sum_val) in sum.iter_mut().take(hidden_size).enumerate() {
                        *sum_val += last_hidden_state[offset + h];
                    }
                    count += 1.0;
                }
            }

            if count > 0.0 {
                for (h, out_val) in out_vec.iter_mut().enumerate() {
                    *out_val = sum[h] / count;
                }
            }
        }

        for out_vec in &mut result {
            let norm: f32 = out_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for val in out_vec.iter_mut() {
                    *val /= norm;
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_pool_l2_normalized() {
        let last_hidden = vec![
            1.0_f32, 2.0, 3.0, 4.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
        ];
        let mask = vec![1_i64, 0, 1, 1];
        let result = BertTokenizer::mean_pool(&last_hidden, &mask, 1, 4, 3);

        assert_eq!(result.len(), 1);
        let norm: f32 = result[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.001,
            "expected L2 norm approx 1.0, got {norm}"
        );
    }

    #[test]
    fn mean_pool_multi_batch() {
        let last_hidden = vec![
            1.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0, 3.0,
            0.0, 0.0,
        ];
        let mask = vec![1_i64, 1, 1, 1, 0, 0, 1, 1, 1];
        let result = BertTokenizer::mean_pool(&last_hidden, &mask, 2, 3, 3);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 3);
        assert_eq!(result[1].len(), 3);

        let norm0: f32 = result[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm1: f32 = result[1].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm0 - 1.0).abs() < 0.001);
        assert!((norm1 - 1.0).abs() < 0.001);
    }

    #[test]
    fn mean_pool_empty_safe() {
        let last_hidden = vec![];
        let mask = vec![];
        let result = BertTokenizer::mean_pool(&last_hidden, &mask, 0, 0, 0);
        assert!(result.is_empty());
    }
}
