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