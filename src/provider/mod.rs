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
pub enum StreamEvent {
    TextDelta { delta: String },
    ReasoningDelta { delta: String },
    ReasoningDetails { details: Vec<ReasoningDetail> },
    ToolCall { call: StreamToolCall },
    Usage { usage: Usage },
    FinishReason { reason: String },
    Error { error: ProviderError },
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
        self.status_code == 0 || self.status_code == 429 || self.status_code >= 500
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
    pub profile: ModelProfile,
}

#[async_trait::async_trait]
pub trait ChatClient: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<mpsc::Receiver<StreamEvent>, ProviderError>;
    fn model(&self) -> &str;
    fn context_window(&self) -> i32;
    fn pricing(&self) -> (f64, f64, f64);

    fn cost_for(&self, usage: &Usage) -> f64 {
        if usage.cost > 0.0 {
            return usage.cost;
        }
        let (in_p, cache_p, out_p) = self.pricing();
        if in_p == 0.0 && cache_p == 0.0 && out_p == 0.0 {
            return 0.0;
        }
        let cached = usage.prompt_cache_hit_tokens.min(usage.prompt_tokens);
        let non_cached = usage.prompt_tokens - cached;
        non_cached as f64 * in_p / 1e6
            + cached as f64 * cache_p / 1e6
            + usage.completion_tokens as f64 * out_p / 1e6
    }
}

impl Client {
    pub fn new(base_url: &str, model: &str, api_key: &str, profile: ModelProfile) -> Self {
        Client {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
            http_client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            debug: false,
            profile,
        }
    }

    pub fn set_debug(&mut self, enabled: bool) {
        self.debug = enabled;
    }

    pub fn debug_enabled(&self) -> bool {
        self.debug
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
            if !self.profile.reasoning_effort.is_empty() || self.profile.reasoning_max_tokens > 0 {
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
        let http_req = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("User-Agent", "mate/1.0")
            .body(body);

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
                    .send(StreamEvent::Error {
                        error: ProviderError {
                            status_code: 0,
                            body: "stream read error".to_string(),
                        },
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
                    .send(StreamEvent::Error {
                        error: ProviderError {
                            status_code: 500,
                            body: msg,
                        },
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
                let _ = tx.send(StreamEvent::Usage { usage }).await;
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
                                        .send(StreamEvent::ReasoningDelta {
                                            delta: text.clone(),
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
                                        .send(StreamEvent::ReasoningDelta {
                                            delta: summary.clone(),
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
                                        .send(StreamEvent::ReasoningDelta {
                                            delta: reasoning.clone(),
                                        })
                                        .await;
                                }
                            } else if let Some(ref rc) = delta.reasoning_content {
                                if !rc.is_empty() {
                                    let _ = tx
                                        .send(StreamEvent::ReasoningDelta { delta: rc.clone() })
                                        .await;
                                }
                            }
                        }

                        if let Some(ref content) = delta.content {
                            if !content.is_empty() {
                                let _ = tx
                                    .send(StreamEvent::TextDelta {
                                        delta: content.clone(),
                                    })
                                    .await;
                            }
                        }

                        if let Some(tc_list) = &delta.tool_calls {
                            for tc in tc_list {
                                let idx = tc.index.unwrap_or(0);
                                let entry =
                                    tool_calls.entry(idx).or_insert_with(|| StreamToolCall {
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

    // Emit accumulated tool calls (in index order for deterministic output)
    let mut tc_keys: Vec<&i32> = tool_calls.keys().collect();
    tc_keys.sort();
    for k in tc_keys {
        let tc = &tool_calls[k];
        if !tc.name.is_empty() {
            if debug {
                eprintln!(
                    "tool call name={} id={} args={}",
                    tc.name, tc.id, tc.arguments
                );
            }
            let _ = tx.send(StreamEvent::ToolCall { call: tc.clone() }).await;
        }
    }

    if !finish_reason.is_empty() {
        let _ = tx
            .send(StreamEvent::FinishReason {
                reason: finish_reason.clone(),
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
            .send(StreamEvent::ReasoningDetails { details: merged })
            .await;
    }
}

#[cfg(test)]
mod tests;
