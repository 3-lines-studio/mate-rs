use crate::message::{Message, ReasoningDetail, ToolDef};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub thinking_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub effort: String,
    #[serde(skip_serializing_if = "is_zero_i32")]
    pub max_tokens: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

fn is_zero_i32(n: &i32) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub cc_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ttl: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDef>,
    pub stream: bool,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub max_tokens: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reasoning_effort: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub route: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "provider")]
    pub provider_prefs: Option<ProviderPreferences>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPreferences {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_fallbacks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_parameters: Option<bool>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data_collection: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub only: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quantizations: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sort: String,
}

#[derive(Debug, Clone)]
pub struct ModelProfile {
    pub context_window: i32,
    pub max_output_tokens: i32,
    pub thinking_type: String,
    pub reasoning_effort: String,
    pub reasoning_max_tokens: i32,
    pub open_router: bool,
    pub input_price: f64,
    pub cached_input_price: f64,
    pub output_price: f64,
    pub fallback_models: Vec<String>,
    pub route: String,
    pub provider_prefs: Option<ProviderPreferences>,
    pub prompt_cache: bool,
    pub prompt_cache_ttl: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTokensDetails {
    pub cached_tokens: i32,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub cache_write_tokens: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionTokensDetails {
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub reasoning_tokens: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    #[serde(default)]
    pub prompt_cache_hit_tokens: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub cost: f64,
}

fn is_zero_f64(n: &f64) -> bool {
    *n == 0.0
}

#[derive(Debug, Clone)]
pub struct StreamEvent {
    pub event_type: String,
    pub delta: String,
    pub reasoning_delta: String,
    pub reasoning_details: Vec<ReasoningDetail>,
    pub tool_call: Option<StreamToolCall>,
    pub usage: Option<Usage>,
    pub finish_reason: String,
    pub error: Option<ProviderError>,
}

#[derive(Debug, Clone)]
pub struct StreamToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct ProviderError {
    pub status_code: u16,
    pub body: String,
}

impl ProviderError {
    pub fn retryable(&self) -> bool {
        self.status_code == 429 || self.status_code >= 500
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unexpected status {}: {}", self.status_code, self.body)
    }
}

impl std::error::Error for ProviderError {}

#[derive(Clone)]
pub struct Client {
    base_url: String,
    model: String,
    api_key: String,
    http_client: reqwest::Client,
    debug: bool,
    extra_headers: HashMap<String, String>,
    pub profile: ModelProfile,
}

#[async_trait::async_trait]
pub trait ChatClient: Send + Sync {
    async fn chat(
        &self,
        req: ChatRequest,
    ) -> Result<mpsc::Receiver<StreamEvent>, ProviderError>;
    fn model(&self) -> &str;
    fn context_window(&self) -> i32;
    fn pricing(&self) -> (f64, f64, f64);
}

impl Client {
    pub fn new(base_url: &str, model: &str, api_key: &str, profile: ModelProfile) -> Self {
        Client {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
            http_client: reqwest::Client::new(),
            debug: false,
            extra_headers: HashMap::new(),
            profile,
        }
    }

    pub fn set_debug(&mut self, enabled: bool) {
        self.debug = enabled;
    }

    pub fn set_extra_headers(&mut self, headers: HashMap<String, String>) {
        self.extra_headers = headers;
    }
}

#[async_trait::async_trait]
impl ChatClient for Client {
    async fn chat(
        &self,
        mut req: ChatRequest,
    ) -> Result<mpsc::Receiver<StreamEvent>, ProviderError> {
        req.model = self.model.clone();
        req.stream = true;
        req.stream_options = Some(StreamOptions {
            include_usage: true,
        });
        if req.max_tokens <= 0 {
            req.max_tokens = self.profile.max_output_tokens;
        }
        if self.profile.open_router {
            if !self.profile.reasoning_effort.is_empty() || self.profile.reasoning_max_tokens > 0
            {
                req.reasoning = Some(ReasoningConfig {
                    effort: self.profile.reasoning_effort.clone(),
                    max_tokens: self.profile.reasoning_max_tokens,
                    exclude: None,
                    enabled: None,
                });
            }
        } else {
            if !self.profile.thinking_type.is_empty() {
                req.thinking = Some(ThinkingConfig {
                    thinking_type: self.profile.thinking_type.clone(),
                });
            }
            if !self.profile.reasoning_effort.is_empty() {
                req.reasoning_effort = self.profile.reasoning_effort.clone();
            }
        }
        if !self.profile.fallback_models.is_empty() {
            req.models = self.profile.fallback_models.clone();
        }
        if !self.profile.route.is_empty() {
            req.route = self.profile.route.clone();
        }
        if let Some(prefs) = &self.profile.provider_prefs {
            req.provider_prefs = Some(prefs.clone());
        }

        if self.profile.prompt_cache {
            let mut cc = CacheControl {
                cc_type: "ephemeral".to_string(),
                ttl: String::new(),
            };
            if self.profile.prompt_cache_ttl == "1h" {
                cc.ttl = "1h".to_string();
            }
            req.cache_control = Some(cc);
        }
        if !self.profile.open_router {
            req.session_id = String::new();
        }

        for msg in &mut req.messages {
            msg.tool_duration = String::new();
        }

        let body = serde_json::to_vec(&req).map_err(|e| ProviderError {
            status_code: 0,
            body: format!("marshal request: {}", e),
        })?;

        if self.debug {
            eprintln!("chat request body: {}", String::from_utf8_lossy(&body));
        }

        let url = format!("{}/chat/completions", self.base_url);
        let mut http_req = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("User-Agent", "mate/1.0")
            .body(body);

        for (k, v) in &self.extra_headers {
            http_req = http_req.header(k.as_str(), v.as_str());
        }

        let resp = http_req.send().await.map_err(|e| ProviderError {
            status_code: 0,
            body: format!("http request: {}", e),
        })?;

        if resp.status().as_u16() != 200 {
            let status = resp.status().as_u16();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(ProviderError {
                status_code: status,
                body: resp_body,
            });
        }

        let (tx, rx) = mpsc::channel(10);
        let debug = self.debug;

        tokio::spawn(async move {
            read_stream(resp, tx, debug).await;
        });

        Ok(rx)
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn context_window(&self) -> i32 {
        self.profile.context_window
    }

    fn pricing(&self) -> (f64, f64, f64) {
        (
            self.profile.input_price,
            self.profile.cached_input_price,
            self.profile.output_price,
        )
    }
}

#[derive(Debug, Deserialize)]
struct StreamError {
    code: Option<i32>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChunkChoiceDeltaReasoningDetail {
    #[serde(rename = "type")]
    rd_type: String,
    id: Option<String>,
    format: Option<String>,
    index: Option<i32>,
    text: Option<String>,
    signature: Option<String>,
    summary: Option<String>,
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChunkChoiceDeltaToolCallFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChunkChoiceDeltaToolCall {
    index: Option<i32>,
    id: Option<String>,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    tc_type: Option<String>,
    function: Option<StreamChunkChoiceDeltaToolCallFunction>,
}

#[derive(Debug, Deserialize)]
struct StreamChunkChoiceDelta {
    content: Option<String>,
    reasoning: Option<String>,
    reasoning_content: Option<String>,
    reasoning_details: Option<Vec<StreamChunkChoiceDeltaReasoningDetail>>,
    tool_calls: Option<Vec<StreamChunkChoiceDeltaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct StreamChunkChoice {
    delta: Option<StreamChunkChoiceDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Option<Vec<StreamChunkChoice>>,
    usage: Option<Usage>,
    error: Option<StreamError>,
}

async fn read_stream(resp: reqwest::Response, tx: mpsc::Sender<StreamEvent>, debug: bool) {
    use futures_util::StreamExt;

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    let mut finish_reason = String::new();
    let mut tool_calls: HashMap<i32, StreamToolCall> = HashMap::new();
    let mut reasoning_details: HashMap<i32, ReasoningDetail> = HashMap::new();
    let mut reasoning_detail_order: Vec<i32> = Vec::new();
    let mut next_detail_idx: i32 = 0;

    while let Some(chunk_result) = stream.next().await {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(_) => {
                let _ = tx
                    .send(StreamEvent {
                        event_type: String::new(),
                        delta: String::new(),
                        reasoning_delta: String::new(),
                        reasoning_details: vec![],
                        tool_call: None,
                        usage: None,
                        finish_reason: String::new(),
                        error: Some(ProviderError {
                            status_code: 0,
                            body: "stream read error".to_string(),
                        }),
                    })
                    .await;
                return;
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].to_string();
            buffer = buffer[line_end + 1..].to_string();

            if debug {
                eprintln!("stream line: {}", line);
            }

            let line = line.trim_end_matches('\r').to_string();

            if line.is_empty() {
                continue;
            }

            let data = if let Some(d) = line.strip_prefix("data: ") {
                d.to_string()
            } else {
                continue;
            };

            if data == "[DONE]" {
                if debug {
                    eprintln!("stream done, pending_tool_calls: {}", tool_calls.len());
                }
                break;
            }

            let chunk: StreamChunk = match serde_json::from_str(&data) {
                Ok(c) => c,
                Err(e) => {
                    if debug {
                        eprintln!("stream unmarshal error: {} data: {}", e, data);
                    }
                    continue;
                }
            };

            if let Some(err) = &chunk.error {
                let msg = format!(
                    "provider error {}: {}",
                    err.code.unwrap_or(0),
                    err.message.as_deref().unwrap_or("unknown")
                );
                let _ = tx
                    .send(StreamEvent {
                        event_type: String::new(),
                        delta: String::new(),
                        reasoning_delta: String::new(),
                        reasoning_details: vec![],
                        tool_call: None,
                        usage: None,
                        finish_reason: String::new(),
                        error: Some(ProviderError {
                            status_code: 500,
                            body: msg,
                        }),
                    })
                    .await;
                return;
            }

            if let Some(usage) = &chunk.usage {
                let mut usage = usage.clone();
                if let Some(ref details) = usage.prompt_tokens_details {
                    if usage.prompt_cache_hit_tokens == 0 {
                        usage.prompt_cache_hit_tokens = details.cached_tokens;
                    }
                }
                let _ = tx
                    .send(StreamEvent {
                        event_type: "usage".to_string(),
                        delta: String::new(),
                        reasoning_delta: String::new(),
                        reasoning_details: vec![],
                        tool_call: None,
                        usage: Some(usage),
                        finish_reason: String::new(),
                        error: None,
                    })
                    .await;
            }

            if let Some(choices) = &chunk.choices {
                for choice in choices {
                    if let Some(fr) = &choice.finish_reason {
                        if !fr.is_empty() {
                            finish_reason = fr.clone();
                        }
                    }

                    if let Some(delta) = &choice.delta {
                        let mut detail_delta = false;

                        if let Some(rd_list) = &delta.reasoning_details {
                            for rd in rd_list {
                                let idx = if let Some(i) = rd.index {
                                    i
                                } else {
                                    loop {
                                        if !reasoning_details.contains_key(&next_detail_idx) {
                                            let idx = next_detail_idx;
                                            next_detail_idx += 1;
                                            break idx;
                                        }
                                        next_detail_idx += 1;
                                    }
                                };

                                let entry = reasoning_details.entry(idx).or_insert_with(|| {
                                    reasoning_detail_order.push(idx);
                                    ReasoningDetail {
                                        detail_type: rd.rd_type.clone(),
                                        id: String::new(),
                                        format: String::new(),
                                        text: String::new(),
                                        signature: String::new(),
                                        summary: String::new(),
                                        data: String::new(),
                                    }
                                });

                                if !rd.rd_type.is_empty() {
                                    entry.detail_type = rd.rd_type.clone();
                                }
                                if let Some(ref id) = rd.id {
                                    entry.id = id.clone();
                                }
                                if let Some(ref fmt) = rd.format {
                                    entry.format = fmt.clone();
                                }
                                if let Some(ref text) = rd.text {
                                    entry.text.push_str(text);
                                    let _ = tx
                                        .send(StreamEvent {
                                            event_type: "reasoning_delta".to_string(),
                                            delta: String::new(),
                                            reasoning_delta: text.clone(),
                                            reasoning_details: vec![],
                                            tool_call: None,
                                            usage: None,
                                            finish_reason: String::new(),
                                            error: None,
                                        })
                                        .await;
                                    detail_delta = true;
                                }
                                if let Some(ref sig) = rd.signature {
                                    entry.signature = sig.clone();
                                }
                                if let Some(ref summary) = rd.summary {
                                    entry.summary.push_str(summary);
                                    let _ = tx
                                        .send(StreamEvent {
                                            event_type: "reasoning_delta".to_string(),
                                            delta: String::new(),
                                            reasoning_delta: summary.clone(),
                                            reasoning_details: vec![],
                                            tool_call: None,
                                            usage: None,
                                            finish_reason: String::new(),
                                            error: None,
                                        })
                                        .await;
                                    detail_delta = true;
                                }
                                if let Some(ref d) = rd.data {
                                    entry.data = d.clone();
                                }
                            }
                        }

                        if !detail_delta {
                            if let Some(ref reasoning) = delta.reasoning {
                                if !reasoning.is_empty() {
                                    let _ = tx
                                        .send(StreamEvent {
                                            event_type: "reasoning_delta".to_string(),
                                            delta: String::new(),
                                            reasoning_delta: reasoning.clone(),
                                            reasoning_details: vec![],
                                            tool_call: None,
                                            usage: None,
                                            finish_reason: String::new(),
                                            error: None,
                                        })
                                        .await;
                                }
                            } else if let Some(ref rc) = delta.reasoning_content {
                                if !rc.is_empty() {
                                    let _ = tx
                                        .send(StreamEvent {
                                            event_type: "reasoning_delta".to_string(),
                                            delta: String::new(),
                                            reasoning_delta: rc.clone(),
                                            reasoning_details: vec![],
                                            tool_call: None,
                                            usage: None,
                                            finish_reason: String::new(),
                                            error: None,
                                        })
                                        .await;
                                }
                            }
                        }

                        if let Some(ref content) = delta.content {
                            if !content.is_empty() {
                                let _ = tx
                                    .send(StreamEvent {
                                        event_type: "text_delta".to_string(),
                                        delta: content.clone(),
                                        reasoning_delta: String::new(),
                                        reasoning_details: vec![],
                                        tool_call: None,
                                        usage: None,
                                        finish_reason: String::new(),
                                        error: None,
                                    })
                                    .await;
                            }
                        }

                        if let Some(tc_list) = &delta.tool_calls {
                            for tc in tc_list {
                                let idx = tc.index.unwrap_or(0);
                                let entry = tool_calls.entry(idx).or_insert_with(|| StreamToolCall {
                                    id: String::new(),
                                    name: String::new(),
                                    arguments: String::new(),
                                });

                                if let Some(ref id) = tc.id {
                                    if !id.is_empty() {
                                        entry.id = id.clone();
                                    }
                                }
                                if let Some(ref func) = tc.function {
                                    if let Some(ref name) = func.name {
                                        entry.name = name.clone();
                                    }
                                    if let Some(ref args) = func.arguments {
                                        entry.arguments.push_str(args);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Emit accumulated tool calls
    for tc in tool_calls.values() {
        if !tc.name.is_empty() {
            if debug {
                eprintln!(
                    "tool call name={} id={} args={}",
                    tc.name, tc.id, tc.arguments
                );
            }
            let _ = tx
                .send(StreamEvent {
                    event_type: "tool_call".to_string(),
                    delta: String::new(),
                    reasoning_delta: String::new(),
                    reasoning_details: vec![],
                    tool_call: Some(tc.clone()),
                    usage: None,
                    finish_reason: String::new(),
                    error: None,
                })
                .await;
        }
    }

    if !finish_reason.is_empty() {
        let _ = tx
            .send(StreamEvent {
                event_type: "finish_reason".to_string(),
                delta: String::new(),
                reasoning_delta: String::new(),
                reasoning_details: vec![],
                tool_call: None,
                usage: None,
                finish_reason: finish_reason.clone(),
                error: None,
            })
            .await;
    }

    if !reasoning_detail_order.is_empty() {
        let mut merged: Vec<ReasoningDetail> = Vec::new();
        for idx in &reasoning_detail_order {
            if let Some(detail) = reasoning_details.get(idx) {
                merged.push(detail.clone());
            }
        }
        let _ = tx
            .send(StreamEvent {
                event_type: "reasoning_details".to_string(),
                delta: String::new(),
                reasoning_delta: String::new(),
                reasoning_details: merged,
                tool_call: None,
                usage: None,
                finish_reason: String::new(),
                error: None,
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Message, Role};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_provider_error_retryable() {
        assert!(ProviderError {
            status_code: 429,
            body: "rate limit".into()
        }
        .retryable());
        assert!(ProviderError {
            status_code: 500,
            body: "internal".into()
        }
        .retryable());
        assert!(ProviderError {
            status_code: 503,
            body: "unavailable".into()
        }
        .retryable());
        assert!(!ProviderError {
            status_code: 400,
            body: "bad request".into()
        }
        .retryable());
        assert!(!ProviderError {
            status_code: 404,
            body: "not found".into()
        }
        .retryable());
    }

    #[test]
    fn test_provider_error_display() {
        let err = ProviderError {
            status_code: 500,
            body: "server error".into(),
        };
        assert_eq!(format!("{}", err), "unexpected status 500: server error");
    }

    #[test]
    fn test_client_pricing() {
        let c = Client::new(
            "http://localhost",
            "m",
            "k",
            ModelProfile {
                context_window: 0,
                max_output_tokens: 0,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 3.0,
                cached_input_price: 0.3,
                output_price: 15.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );
        let (input, cached, output) = c.pricing();
        assert_eq!(input, 3.0);
        assert_eq!(cached, 0.3);
        assert_eq!(output, 15.0);
    }

    #[test]
    fn test_client_model_and_context() {
        let c = Client::new(
            "http://localhost",
            "gpt-4",
            "k",
            ModelProfile {
                context_window: 128000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );
        assert_eq!(c.model(), "gpt-4");
        assert_eq!(c.context_window(), 128000);
    }

    #[tokio::test]
    async fn test_chat_request_json_shape() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\n",
            ))
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "test-model",
            "test-key",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![Message {
                role: Role::User,
                content: "hi".to_string(),
                reasoning_content: String::new(),
                reasoning_details: vec![],
                tool_calls: vec![],
                tool_call_id: String::new(),
                name: String::new(),
                tool_duration: String::new(),
            }],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let rx = c
            .chat(req)
            .await
            .unwrap();

        drop(rx);
    }

    #[tokio::test]
    async fn test_chat_sets_stream_options() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("data: [DONE]\n\n"))
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "test-model",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 100,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let rx = c
            .chat(req)
            .await
            .unwrap();
        drop(rx);
    }

    #[tokio::test]
    async fn test_chat_non_200_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "test-model",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let result = c
            .chat(req)
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status_code, 500);
    }

    #[tokio::test]
    async fn test_chat_openrouter_reasoning_config() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("data: [DONE]\n\n"))
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "openai/gpt-4o",
            "k",
            ModelProfile {
                context_window: 128000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: "high".to_string(),
                reasoning_max_tokens: 2000,
                open_router: true,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let rx = c
            .chat(req)
            .await
            .unwrap();
        drop(rx);
    }

    #[tokio::test]
    async fn test_chat_non_openrouter_thinking_config() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("data: [DONE]\n\n"))
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "claude-sonnet-4-20250514",
            "k",
            ModelProfile {
                context_window: 200000,
                max_output_tokens: 4096,
                thinking_type: "enabled".to_string(),
                reasoning_effort: "high".to_string(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: true,
                prompt_cache_ttl: "1h".to_string(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: "session-123".to_string(),
        };

        let rx = c
            .chat(req)
            .await
            .unwrap();
        drop(rx);
    }

    #[tokio::test]
    async fn test_sse_text_delta() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut got_text = false;
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "text_delta" && ev.delta == "hello" {
                got_text = true;
            }
        }
        assert!(got_text, "expected text_delta with 'hello'");
    }

    #[tokio::test]
    async fn test_sse_reasoning_content_delta() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"thinking...\"}}]}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut got = false;
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "reasoning_delta" && ev.reasoning_delta == "thinking..." {
                got = true;
            }
        }
        assert!(got);
    }

    #[tokio::test]
    async fn test_sse_reasoning_delta_fallback() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"reasoning\":\"think\"}}]}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut got = false;
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "reasoning_delta" && ev.reasoning_delta == "think" {
                got = true;
            }
        }
        assert!(got);
    }

    #[tokio::test]
    async fn test_sse_reasoning_details_prefer_over_fallback() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"reasoning\":\"fallback\",\"reasoning_content\":\"fallback\",\"reasoning_details\":[{\"type\":\"reasoning.text\",\"index\":0,\"text\":\"detail\"}]}}]}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut deltas: Vec<String> = Vec::new();
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "reasoning_delta" {
                deltas.push(ev.reasoning_delta);
            }
        }
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0], "detail");
    }

    #[tokio::test]
    async fn test_sse_tool_call_accumulation() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"bash\",\"arguments\":\"{\\\"cmd\"}}]}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\":\\\"ls\\\"}\"}]}}]}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut got_tool_call = false;
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "tool_call"
                && ev.tool_call.as_ref().map_or(false, |tc| tc.name == "bash")
            {
                got_tool_call = true;
            }
        }
        assert!(got_tool_call);
    }

    #[tokio::test]
    async fn test_sse_finish_reason() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n"),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut got = false;
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "finish_reason" && ev.finish_reason == "stop" {
                got = true;
            }
        }
        assert!(got);
    }

    #[tokio::test]
    async fn test_sse_usage() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut got = false;
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "usage"
                && ev.usage.as_ref().map_or(false, |u| u.prompt_tokens == 10)
            {
                got = true;
            }
        }
        assert!(got);
    }

    #[tokio::test]
    async fn test_sse_usage_cached_tokens_fallback() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"usage\":{\"prompt_tokens\":15,\"completion_tokens\":8,\"total_tokens\":23,\"prompt_tokens_details\":{\"cached_tokens\":9}}}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        while let Some(ev) = rx.recv().await {
            if ev.event_type == "usage" {
                let u = ev.usage.unwrap();
                assert_eq!(u.prompt_cache_hit_tokens, 9);
                assert!(u.prompt_tokens_details.is_some());
                assert_eq!(u.prompt_tokens_details.unwrap().cached_tokens, 9);
                return;
            }
        }
        panic!("expected usage event");
    }

    #[tokio::test]
    async fn test_sse_usage_explicit_field_wins() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15,\"prompt_cache_hit_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":7}}}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        while let Some(ev) = rx.recv().await {
            if ev.event_type == "usage" {
                assert_eq!(ev.usage.unwrap().prompt_cache_hit_tokens, 3);
                return;
            }
        }
        panic!("expected usage event");
    }

    #[tokio::test]
    async fn test_sse_usage_no_cache_field() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":16,\"total_tokens\":26}}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        while let Some(ev) = rx.recv().await {
            if ev.event_type == "usage" {
                assert_eq!(ev.usage.unwrap().prompt_cache_hit_tokens, 0);
                return;
            }
        }
        panic!("expected usage event");
    }

    #[tokio::test]
    async fn test_sse_reasoning_details_text_merge() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"type\":\"text\",\"index\":0,\"text\":\"hello \"}]}}]}\n\ndata: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"type\":\"text\",\"index\":0,\"text\":\"world\"}]}}]}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut merged: Vec<ReasoningDetail> = Vec::new();
        let mut got_delta = false;
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "reasoning_delta" {
                got_delta = true;
            }
            if ev.event_type == "reasoning_details" {
                merged = ev.reasoning_details;
            }
        }
        assert!(got_delta);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].detail_type, "text");
        assert_eq!(merged[0].text, "hello world");
    }

    #[tokio::test]
    async fn test_sse_reasoning_details_encrypted() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.encrypted\",\"index\":0,\"data\":\"encblob\",\"id\":\"r1\",\"format\":\"anthropic-claude-v1\"}]}}]}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut merged = Vec::new();
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "reasoning_details" {
                merged = ev.reasoning_details;
            }
        }
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].detail_type, "reasoning.encrypted");
        assert_eq!(merged[0].data, "encblob");
        assert_eq!(merged[0].id, "r1");
        assert_eq!(merged[0].format, "anthropic-claude-v1");
    }

    #[tokio::test]
    async fn test_sse_reasoning_details_no_index_auto_assigns() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\"a\"},{\"type\":\"reasoning.encrypted\",\"data\":\"x\"}]}}]}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut merged = Vec::new();
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "reasoning_details" {
                merged = ev.reasoning_details;
            }
        }
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].text, "a");
        assert_eq!(merged[1].data, "x");
    }

    #[tokio::test]
    async fn test_sse_done_sentinel() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("data: [DONE]\n\n"))
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        while rx.recv().await.is_some() {}
    }

    #[tokio::test]
    async fn test_sse_error_in_chunk() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"error\":{\"code\":500,\"message\":\"Provider disconnected\"}}\n\n",
                ),
            )
            .mount(&mock_server)
            .await;

        let c = Client::new(
            &mock_server.uri(),
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 4096,
                thinking_type: String::new(),
                reasoning_effort: String::new(),
                reasoning_max_tokens: 0,
                open_router: false,
                input_price: 0.0,
                cached_input_price: 0.0,
                output_price: 0.0,
                fallback_models: vec![],
                route: String::new(),
                provider_prefs: None,
                prompt_cache: false,
                prompt_cache_ttl: String::new(),
            },
        );

        let req = ChatRequest {
            model: String::new(),
            messages: vec![],
            tools: vec![],
            stream: false,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };

        let mut rx = c
            .chat(req)
            .await
            .unwrap();

        let mut got_error = false;
        while let Some(ev) = rx.recv().await {
            if ev.error.is_some() {
                got_error = true;
                assert!(ev.event_type.is_empty());
                let err = ev.error.unwrap();
                assert!(err.body.contains("500"));
                assert!(err.body.contains("Provider disconnected"));
            }
        }
        assert!(got_error);
    }

    #[tokio::test]
    async fn test_chat_request_models_route_json() {
        let req = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "hi".to_string(),
                reasoning_content: String::new(),
                reasoning_details: vec![],
                tool_calls: vec![],
                tool_call_id: String::new(),
                name: String::new(),
                tool_duration: String::new(),
            }],
            tools: vec![],
            stream: true,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![
                "anthropic/claude-sonnet-4".to_string(),
                "openai/gpt-4o".to_string(),
            ],
            route: "fallback".to_string(),
            provider_prefs: Some(ProviderPreferences {
                order: vec![],
                allow_fallbacks: None,
                require_parameters: None,
                data_collection: String::new(),
                only: vec![],
                ignore: vec![],
                quantizations: vec![],
                sort: "throughput".to_string(),
            }),
            cache_control: None,
            session_id: String::new(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: ChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.models.len(), 2);
        assert_eq!(decoded.models[0], "anthropic/claude-sonnet-4");
        assert_eq!(decoded.route, "fallback");
        assert!(decoded.provider_prefs.is_some());
        assert_eq!(decoded.provider_prefs.unwrap().sort, "throughput");

        let simple = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: "hi".to_string(),
                reasoning_content: String::new(),
                reasoning_details: vec![],
                tool_calls: vec![],
                tool_call_id: String::new(),
                name: String::new(),
                tool_duration: String::new(),
            }],
            tools: vec![],
            stream: true,
            max_tokens: 0,
            stream_options: None,
            thinking: None,
            reasoning_effort: String::new(),
            reasoning: None,
            models: vec![],
            route: String::new(),
            provider_prefs: None,
            cache_control: None,
            session_id: String::new(),
        };
        let simple_json = serde_json::to_string(&simple).unwrap();
        assert!(!simple_json.contains("\"models\""));
    }
}
