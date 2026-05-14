//! Optional in-process retrieval subagent.
//! See spec §5 (specs/2026-05-13-precise-locate-design.md).

pub mod model;
pub mod tools;
pub mod validate;
pub mod prompts;
pub mod providers;

#[allow(clippy::module_inception)]
mod loop_;
pub use loop_::{run, SmartError, SmartRequest, SmartResponse};

pub use model::{LocateModel, LocateModelError, Message, ModelStep, Role};
pub use tools::SubToolCtx;

#[cfg(feature = "smart")]
use std::sync::Arc;

#[cfg(feature = "smart")]
pub fn build_model(
    provider: &str,
    model: &str,
    endpoint: Option<String>,
) -> Result<Arc<dyn LocateModel>, LocateModelError> {
    match provider {
        "openai" => Ok(Arc::new(providers::openai::OpenAiModel::from_env(model.into(), endpoint)?)),
        "anthropic" => Ok(Arc::new(providers::anthropic::AnthropicModel::from_env(model.into(), endpoint)?)),
        "ollama" | "local" => Ok(Arc::new(providers::ollama::OllamaModel::new(model.into(), endpoint))),
        other => Err(LocateModelError::Provider(format!("unknown provider `{other}`"))),
    }
}

#[cfg(not(feature = "smart"))]
pub fn build_model(
    _provider: &str,
    _model: &str,
    _endpoint: Option<String>,
) -> Result<std::sync::Arc<dyn LocateModel>, LocateModelError> {
    Err(LocateModelError::Provider(
        "smart feature not compiled in this build".into(),
    ))
}