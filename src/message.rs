use serde::{Deserialize, Serialize, Serializer};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reasoning_content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_details: Vec<ReasoningDetail>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool_call_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool_duration: String,
}

impl Serialize for Message {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let pad =
            self.role == Role::Assistant && self.content.is_empty() && self.tool_calls.is_empty();
        let mut st = serializer.serialize_struct("Message", 8)?;
        st.serialize_field("role", &self.role)?;
        if pad {
            st.serialize_field("content", " ")?;
        } else if !self.content.is_empty() {
            st.serialize_field("content", &self.content)?;
        } else {
            st.skip_field("content")?;
        }
        if !self.reasoning_content.is_empty() {
            st.serialize_field("reasoning_content", &self.reasoning_content)?;
        } else {
            st.skip_field("reasoning_content")?;
        }
        if !self.reasoning_details.is_empty() {
            st.serialize_field("reasoning_details", &self.reasoning_details)?;
        } else {
            st.skip_field("reasoning_details")?;
        }
        if !self.tool_calls.is_empty() {
            st.serialize_field("tool_calls", &self.tool_calls)?;
        } else {
            st.skip_field("tool_calls")?;
        }
        if !self.tool_call_id.is_empty() {
            st.serialize_field("tool_call_id", &self.tool_call_id)?;
        } else {
            st.skip_field("tool_call_id")?;
        }
        if !self.name.is_empty() {
            st.serialize_field("name", &self.name)?;
        } else {
            st.skip_field("name")?;
        }
        if !self.tool_duration.is_empty() {
            st.serialize_field("tool_duration", &self.tool_duration)?;
        } else {
            st.skip_field("tool_duration")?;
        }
        st.end()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningDetail {
    #[serde(rename = "type")]
    pub detail_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub format: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub signature: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

impl From<crate::provider::StreamToolCall> for ToolCall {
    fn from(tc: crate::provider::StreamToolCall) -> Self {
        ToolCall {
            id: tc.id,
            call_type: "function".into(),
            function: ToolCallFunction {
                name: tc.name,
                arguments: tc.arguments,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub def_type: String,
    pub function: ToolDefFunction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefFunction {
    pub name: String,
    pub description: String,
    pub parameters: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_empty_assistant() {
        let m = Message {
            role: Role::Assistant,
            content: String::new(),
            reasoning_content: String::new(),
            reasoning_details: vec![],
            tool_calls: vec![],
            tool_call_id: String::new(),
            name: String::new(),
            tool_duration: String::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let out: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(out["content"].as_str().unwrap(), " ");
    }

    #[test]
    fn test_serialize_assistant_with_content() {
        let m = Message {
            role: Role::Assistant,
            content: "hello".to_string(),
            reasoning_content: String::new(),
            reasoning_details: vec![],
            tool_calls: vec![],
            tool_call_id: String::new(),
            name: String::new(),
            tool_duration: String::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let out: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(out["content"].as_str().unwrap(), "hello");
    }

    #[test]
    fn test_serialize_assistant_with_tool_calls() {
        let m = Message {
            role: Role::Assistant,
            content: String::new(),
            reasoning_content: String::new(),
            reasoning_details: vec![],
            tool_calls: vec![ToolCall {
                id: "1".to_string(),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: "bash".to_string(),
                    arguments: r#"{"cmd":"ls"}"#.to_string(),
                },
            }],
            tool_call_id: String::new(),
            name: String::new(),
            tool_duration: String::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let out: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(out.get("content").is_none());
        let tc = out["tool_calls"].as_array().unwrap();
        assert_eq!(tc.len(), 1);
    }

    #[test]
    fn test_serialize_system_message() {
        let m = Message {
            role: Role::System,
            content: "you are helpful".to_string(),
            reasoning_content: String::new(),
            reasoning_details: vec![],
            tool_calls: vec![],
            tool_call_id: String::new(),
            name: String::new(),
            tool_duration: String::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let out: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(out["role"].as_str().unwrap(), "system");
        assert_eq!(out["content"].as_str().unwrap(), "you are helpful");
    }

    #[test]
    fn test_serialize_tool_message() {
        let m = Message {
            role: Role::Tool,
            content: "result".to_string(),
            reasoning_content: String::new(),
            reasoning_details: vec![],
            tool_calls: vec![],
            tool_call_id: "call_1".to_string(),
            name: "bash".to_string(),
            tool_duration: String::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let out: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(out["role"].as_str().unwrap(), "tool");
        assert_eq!(out["tool_call_id"].as_str().unwrap(), "call_1");
    }

    #[test]
    fn test_round_trip() {
        let m = Message {
            role: Role::Assistant,
            content: "response".to_string(),
            reasoning_content: "thinking...".to_string(),
            reasoning_details: vec![],
            tool_calls: vec![ToolCall {
                id: "t1".to_string(),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: "read_file".to_string(),
                    arguments: r#"{"path":"f.go"}"#.to_string(),
                },
            }],
            tool_call_id: String::new(),
            name: String::new(),
            tool_duration: String::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let round: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(round.role, m.role);
        assert_eq!(round.content, m.content);
        assert_eq!(round.reasoning_content, m.reasoning_content);
        assert_eq!(round.tool_calls.len(), 1);
        assert_eq!(round.tool_calls[0].id, "t1");
    }

    #[test]
    fn test_serialize_empty_user_message() {
        let m = Message {
            role: Role::User,
            content: String::new(),
            reasoning_content: String::new(),
            reasoning_details: vec![],
            tool_calls: vec![],
            tool_call_id: String::new(),
            name: String::new(),
            tool_duration: String::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let out: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(out["role"].as_str().unwrap(), "user");
    }

    #[test]
    fn test_serialize_reasoning_details() {
        let m = Message {
            role: Role::Assistant,
            content: "response".to_string(),
            reasoning_content: String::new(),
            reasoning_details: vec![
                ReasoningDetail {
                    detail_type: "reasoning.text".to_string(),
                    id: "r1".to_string(),
                    format: "anthropic-claude-v1".to_string(),
                    text: "thinking...".to_string(),
                    signature: String::new(),
                    summary: String::new(),
                    data: String::new(),
                },
                ReasoningDetail {
                    detail_type: "reasoning.encrypted".to_string(),
                    id: String::new(),
                    format: String::new(),
                    text: String::new(),
                    signature: "sig123".to_string(),
                    summary: String::new(),
                    data: "blob".to_string(),
                },
            ],
            tool_calls: vec![],
            tool_call_id: String::new(),
            name: String::new(),
            tool_duration: String::new(),
        };
        let json = serde_json::to_string(&m).unwrap();
        let round: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(round.reasoning_details.len(), 2);
        assert_eq!(round.reasoning_details[0].detail_type, "reasoning.text");
        assert_eq!(round.reasoning_details[0].text, "thinking...");
        assert_eq!(round.reasoning_details[0].id, "r1");
        assert_eq!(round.reasoning_details[0].format, "anthropic-claude-v1");
        assert_eq!(
            round.reasoning_details[1].detail_type,
            "reasoning.encrypted"
        );
        assert_eq!(round.reasoning_details[1].data, "blob");
        assert_eq!(round.reasoning_details[1].signature, "sig123");
    }
}
