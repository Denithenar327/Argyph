//! Loop-driver tests using MockModel.
//! These exercise the bounded ReAct loop without requiring live infrastructure.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use argyph_locate::smart::providers::mock::MockModel;
use argyph_locate::smart::{run, ModelStep, SmartError, SmartRequest, SubToolCtx};
use std::sync::Arc;

fn build_ctx() -> SubToolCtx {
    let store = argyph_store::SqliteStore::open_in_memory().unwrap();
    let embedder = argyph_embed::NullEmbedder::new();
    SubToolCtx {
        store: Arc::new(store) as Arc<dyn argyph_store::Store>,
        embedder: Arc::new(embedder) as Arc<dyn argyph_embed::Embedder>,
        root: std::path::PathBuf::from("/tmp/argyph-smart-test"),
    }
}

#[tokio::test]
async fn final_with_no_calls_returns_empty_spans() {
    let model = Arc::new(MockModel::new(vec![ModelStep::Final {
        selected_node_ids: vec![],
        reasoning_summary: "nothing matched".into(),
    }]));
    let req = SmartRequest {
        query: "x".into(),
        max_steps: 4,
        max_output_tokens: 1024,
    };
    let resp = run(model, build_ctx(), req).await.unwrap();
    assert_eq!(resp.spans.len(), 0);
    assert_eq!(resp.steps_taken, 1);
}

#[tokio::test]
async fn fabricated_node_ids_are_rejected() {
    let model = Arc::new(MockModel::new(vec![ModelStep::Final {
        selected_node_ids: vec!["fake:0:0".into()],
        reasoning_summary: "bad".into(),
    }]));
    let req = SmartRequest {
        query: "x".into(),
        max_steps: 4,
        max_output_tokens: 1024,
    };
    let err = run(model, build_ctx(), req).await.unwrap_err();
    match err {
        SmartError::FabricatedNodeIds(ids) => assert_eq!(ids, vec!["fake:0:0"]),
        other => panic!("wrong error: {other:?}"),
    }
}

#[tokio::test]
async fn budget_exceeded_returns_error() {
    let mut script = Vec::new();
    for i in 0..10 {
        script.push(ModelStep::ToolCall {
            id: i.to_string(),
            name: "get_repo_overview".into(),
            arguments: serde_json::json!({}),
        });
    }
    let model = Arc::new(MockModel::new(script));
    let req = SmartRequest {
        query: "x".into(),
        max_steps: 3,
        max_output_tokens: 1024,
    };
    let err = run(model, build_ctx(), req).await.unwrap_err();
    assert!(matches!(
        err,
        SmartError::BudgetExceeded { steps_taken: 3, .. }
    ));
}
