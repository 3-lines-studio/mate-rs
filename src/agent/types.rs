use crate::message::{Message, ReasoningDetail};
use crate::provider::{ChatClient, StreamToolCall, Usage};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Event {
    pub kind: EventKind,
    pub subagent: String,
    pub subagent_id: String,
}

#[derive(Debug, Clone)]
pub enum EventKind {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallStart {
        id: String,
        name: String,
        args: String,
    },
    ToolResult {
        id: String,
        name: String,
        args: String,
        result: String,
        duration: String,
    },
    ToolError {
        id: String,
        name: String,
        error: String,
        duration: String,
    },
    Error(String),
    Retry(String),
    RetryAvailable(String),
    AgentDone(String),
    Usage(Usage),
    CompactingStart,
}

impl Event {
    pub fn text_delta(delta: &str) -> Self {
        Event {
            kind: EventKind::TextDelta(delta.into()),
            subagent: String::new(),
            subagent_id: String::new(),
        }
    }
    pub fn reasoning_delta(delta: &str) -> Self {
        Event {
            kind: EventKind::ReasoningDelta(delta.into()),
            subagent: String::new(),
            subagent_id: String::new(),
        }
    }
    pub fn tool_call_start(tc: &StreamToolCall) -> Self {
        Event {
            kind: EventKind::ToolCallStart {
                id: tc.id.clone(),
                name: tc.name.clone(),
                args: tc.arguments.clone(),
            },
            subagent: String::new(),
            subagent_id: String::new(),
        }
    }
    pub fn tool_result_ev(tc: &StreamToolCall, result: &str, duration: &str) -> Self {
        Event {
            kind: EventKind::ToolResult {
                id: tc.id.clone(),
                name: tc.name.clone(),
                args: tc.arguments.clone(),
                result: result.into(),
                duration: duration.into(),
            },
            subagent: String::new(),
            subagent_id: String::new(),
        }
    }
    pub fn tool_error_ev(tc: &StreamToolCall, msg: &str, duration: &str) -> Self {
        Event {
            kind: EventKind::ToolError {
                id: tc.id.clone(),
                name: tc.name.clone(),
                error: msg.into(),
                duration: duration.into(),
            },
            subagent: String::new(),
            subagent_id: String::new(),
        }
    }
    pub fn error_msg(msg: &str) -> Self {
        Event {
            kind: EventKind::Error(msg.into()),
            subagent: String::new(),
            subagent_id: String::new(),
        }
    }
    pub fn retry_ev(msg: &str) -> Self {
        Event {
            kind: EventKind::Retry(msg.into()),
            subagent: String::new(),
            subagent_id: String::new(),
        }
    }
    pub fn retry_available(msg: &str) -> Self {
        Event {
            kind: EventKind::RetryAvailable(msg.into()),
            subagent: String::new(),
            subagent_id: String::new(),
        }
    }
    pub fn agent_done(finish_reason: &str) -> Self {
        Event {
            kind: EventKind::AgentDone(finish_reason.into()),
            subagent: String::new(),
            subagent_id: String::new(),
        }
    }
    pub fn usage_ev(usage: Usage) -> Self {
        Event {
            kind: EventKind::Usage(usage),
            subagent: String::new(),
            subagent_id: String::new(),
        }
    }
}

#[derive(Clone)]
pub struct SubagentDef {
    pub id: String,
    pub description: String,
    pub client: Arc<dyn ChatClient>,
    pub registry: Arc<crate::tools::Registry>,
    pub system_prompt: String,
    pub model_name: String,
}

#[derive(Clone)]
pub(super) struct SubagentTurn {
    pub msgs: Vec<Message>,
    pub subagent: String,
    pub tool_call_id: String,
}

pub(super) struct PendingTool {
    pub call: StreamToolCall,
    pub result: String,
    pub duration: String,
    #[allow(dead_code)]
    pub had_error: bool,
}

pub(super) struct RoundResult {
    pub content: String,
    pub reasoning: String,
    pub reasoning_details: Vec<ReasoningDetail>,
    pub tool_calls: Vec<StreamToolCall>,
    pub finish_reason: String,
}

#[derive(Clone)]
pub(super) struct CompactionState {
    pub compaction_client: Option<Arc<dyn ChatClient>>,
    pub compacted_summary: String,
    pub compacted_up_to: String,
}

#[derive(Clone)]
pub(super) struct SubagentState {
    pub subagents: HashMap<String, SubagentDef>,
    pub subagent_turns: Vec<SubagentTurn>,
    pub is_subagent: bool,
}

pub(super) struct DelegateData {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub cost: f64,
    pub msgs: Vec<Message>,
    pub subagent_id: String,
    pub tool_call_id: String,
}
