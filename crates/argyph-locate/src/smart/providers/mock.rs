//! Scriptable mock model for tests.

use crate::smart::model::{LocateModel, LocateModelError, Message, ModelStep};
use async_trait::async_trait;
use std::sync::Mutex;

pub struct MockModel {
    script: Mutex<std::collections::VecDeque<ModelStep>>,
    pub call_log: Mutex<Vec<Vec<Message>>>,
}

impl MockModel {
    pub fn new(steps: Vec<ModelStep>) -> Self {
        Self {
            script: Mutex::new(steps.into()),
            call_log: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl LocateModel for MockModel {
    async fn step(&self, messages: &[Message]) -> Result<ModelStep, LocateModelError> {
        self.call_log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(messages.to_vec());
        self.script
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .ok_or_else(|| LocateModelError::Provider("MockModel script exhausted".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smart::model::Role;

    #[tokio::test]
    async fn mock_returns_scripted_steps_in_order() {
        let mock = MockModel::new(vec![
            ModelStep::ToolCall {
                id: "1".into(),
                name: "locate".into(),
                arguments: serde_json::json!({}),
            },
            ModelStep::Final {
                selected_node_ids: vec!["n1".into()],
                reasoning_summary: "done".into(),
            },
        ]);
        let msgs = vec![Message {
            role: Role::User,
            content: "hi".into(),
            tool_call_id: None,
            tool_name: None,
        }];
        let first = mock.step(&msgs).await.unwrap();
        assert!(matches!(first, ModelStep::ToolCall { .. }));
        let second = mock.step(&msgs).await.unwrap();
        assert!(matches!(second, ModelStep::Final { .. }));
    }

    #[tokio::test]
    async fn mock_records_calls() {
        let mock = MockModel::new(vec![ModelStep::Final {
            selected_node_ids: vec![],
            reasoning_summary: "".into(),
        }]);
        let _ = mock.step(&[]).await.unwrap();
        assert_eq!(
            mock.call_log
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .len(),
            1
        );
    }
}
