use crate::services::agent::{
    ActionBatchRequest, ActionBatchResponse, ActionRequest, ActionResponse,
};

pub trait AgentClient {
    fn act(&self, request: &ActionRequest) -> Result<ActionResponse, String>;

    fn act_batch(&self, request: &ActionBatchRequest) -> Result<ActionBatchResponse, String>;
}
