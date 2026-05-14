use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;
use tracing;

use crate::error::{EmbedError, Result};
use crate::model_hashes;

const BGE_SMALL_MODEL_ID: &str = "bge-small-en-v1.5";
const HF_BASE: &str = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main";

const ONNX_FILENAME: &str = "model.onnx";
const TOKENIZER_FILENAME: &str = "tokenizer.json";

#[derive(Debug)]
pub struct ModelFiles {
    pub onnx_path: PathBuf,
    pub tokenizer_path: PathBuf,
}

impl ModelFiles {
    pub async fn ensure_available(model_id: &str, cache_dir: Option<&Path>) -> Result<ModelFiles> {
        if model_id != BGE_SMALL_MODEL_ID {
            return Err(EmbedError::Config(format!(
                "unknown local model: {model_id}"
            )));
        }

        let cache = cache_dir
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_cache_dir);
        let model_dir = cache.join(model_id);

        let onnx_path = model_dir.join(ONNX_FILENAME);
        let tokenizer_path = model_dir.join(TOKENIZER_FILENAME);

        if Self::needs_download(&model_dir).await {
            tracing::info!(
                model_id = %model_id,
                cache_dir = %model_dir.display(),
                "downloading local model files"
            );

            tokio::fs::create_dir_all(&model_dir).await.map_err(|e| {
                EmbedError::Config(format!(
                    "failed to create cache dir {}: {e}",
                    model_dir.display()
                ))
            })?;

            Self::download_and_verify(
                &format!("{HF_BASE}/onnx/{ONNX_FILENAME}"),
                &onnx_path,
                model_hashes::BGE_SMALL_ONNX_SHA256,
            )
            .await?;

            Self::download_and_verify(
                &format!("{HF_BASE}/{TOKENIZER_FILENAME}"),
                &tokenizer_path,
                model_hashes::BGE_SMALL_TOKENIZER_SHA256,
            )
            .await?;

            tracing::info!(
                model_id = %model_id,
                "model files downloaded and verified"
            );
        }

        Ok(ModelFiles {
            onnx_path: model_dir.join(ONNX_FILENAME),
            tokenizer_path: model_dir.join(TOKENIZER_FILENAME),
        })
    }

    fn default_cache_dir() -> PathBuf {
        let home = dirs_next().unwrap_or_else(|| PathBuf::from("."));
        home.join(".cache").join("argyph").join("models")
    }

    async fn needs_download(model_dir: &Path) -> bool {
        let onnx = model_dir.join(ONNX_FILENAME);
        let tok = model_dir.join(TOKENIZER_FILENAME);

        let onnx_ok = Self::file_hash_matches(&onnx, model_hashes::BGE_SMALL_ONNX_SHA256).await;
        let tok_ok = Self::file_hash_matches(&tok, model_hashes::BGE_SMALL_TOKENIZER_SHA256).await;

        !(onnx_ok && tok_ok)
    }

    async fn file_hash_matches(path: &Path, expected_hex: &str) -> bool {
        match tokio::fs::read(path).await {
            Ok(data) => {
                use sha2::Digest;
                let hash = sha2::Sha256::digest(&data);
                let hex = hex::encode(hash);
                hex == expected_hex
            }
            Err(_) => false,
        }
    }

    async fn download_and_verify(url: &str, dest: &Path, expected_sha256: &str) -> Result<()> {
        let tmp = dest.with_extension("tmp");

        tracing::info!(%url, "downloading");
        let response = reqwest::get(url)
            .await
            .map_err(|e| EmbedError::Config(format!("failed to download {url}: {e}")))?;

        if !response.status().is_success() {
            return Err(EmbedError::Config(format!(
                "download failed for {url}: HTTP {}",
                response.status().as_u16()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| EmbedError::Config(format!("failed to read response for {url}: {e}")))?;

        {
            use sha2::Digest;
            let hash = sha2::Sha256::digest(&bytes);
            let hex = hex::encode(hash);
            if hex != expected_sha256 {
                return Err(EmbedError::Config(format!(
                    "SHA-256 mismatch for {url}: expected {expected_sha256}, got {hex}"
                )));
            }
        }

        let mut f = tokio::fs::File::create(&tmp).await.map_err(|e| {
            EmbedError::Config(format!("failed to create temp file {}: {e}", tmp.display()))
        })?;
        f.write_all(&bytes).await.map_err(|e| {
            EmbedError::Config(format!("failed to write temp file {}: {e}", tmp.display()))
        })?;
        f.flush().await.map_err(|e| {
            EmbedError::Config(format!("failed to flush temp file {}: {e}", tmp.display()))
        })?;
        drop(f);

        tokio::fs::rename(&tmp, dest).await.map_err(|e| {
            EmbedError::Config(format!(
                "failed to rename {} -> {}: {e}",
                tmp.display(),
                dest.display()
            ))
        })?;

        tracing::info!(%url, "verified and cached");
        Ok(())
    }
}

fn dirs_next() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .or({
            #[cfg(target_os = "windows")]
            {
                let drive = std::env::var("HOMEDRIVE").unwrap_or_default();
                let path = std::env::var("HOMEPATH").unwrap_or_default();
                if drive.is_empty() || path.is_empty() {
                    None
                } else {
                    Some(format!("{drive}{path}"))
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                None
            }
        })
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unknown_model_id_returns_config_error() {
        let result = ModelFiles::ensure_available("unknown-model", None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            EmbedError::Config(msg) => assert!(msg.contains("unknown")),
            other => panic!("expected Config error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn needs_download_true_for_empty_dir() {
        let dir = std::env::temp_dir().join("argyph_test_empty");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ModelFiles::needs_download(&dir).await);
    }
}
