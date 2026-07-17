use crate::message::{Message, ReasoningDetail, Role, ToolCall, ToolCallFunction, ToolDef};
use crate::provider::{ChatClient, ChatRequest, ProviderError, StreamToolCall, Usage};
use crate::session::store::Store;
use crate::session::types::{compute_turn_id, turn_label};
use crate::session::{Session, Turn};
use crate::tools::Registry;
use chrono::Utc;
use rand::RngExt;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::Mutex as TokioMutex;

const TOOL_RULES_PROMPT: &str = r"CRITICAL TOOL RULES:
- Use tools directly — never describe what you'd do, execute it.
- Do not fabricate results.
- Non-delegate tool calls timeout after 120 seconds.
- Prefer `symbols` (kind: find/refs/list) over grep for symbol lookups in Rust, Go, TS/TSX/JSX, CSS.";

// ── Event ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Event {
    pub event_type: String,
    pub delta: String,
    pub tool_call_id: String,
    pub tool_call_name: String,
    pub tool_call_args: String,
    pub tool_result: String,
    pub tool_error: String,
    pub tool_duration: String,
    pub error: String,
    pub finish_reason: String,
    pub usage: Option<Usage>,
    pub subagent: String,
    pub subagent_id: String,
}

impl Event {
    pub fn text_delta(delta: &str) -> Self {
        Event {
            event_type: "text_delta".into(),
            delta: delta.into(),
            ..Default::default()
        }
    }
    pub fn reasoning_delta(delta: &str) -> Self {
        Event {
            event_type: "reasoning_delta".into(),
            delta: delta.into(),
            ..Default::default()
        }
    }
    pub fn tool_call_start(tc: &StreamToolCall) -> Self {
        Event {
            event_type: "tool_call_start".into(),
            tool_call_id: tc.id.clone(),
            tool_call_name: tc.name.clone(),
            tool_call_args: tc.arguments.clone(),
            ..Default::default()
        }
    }
    pub fn tool_result_ev(tc: &StreamToolCall, result: &str, duration: &str) -> Self {
        Event {
            event_type: "tool_result".into(),
            tool_call_id: tc.id.clone(),
            tool_call_name: tc.name.clone(),
            tool_call_args: tc.arguments.clone(),
            tool_result: result.into(),
            tool_duration: duration.into(),
            ..Default::default()
        }
    }
    pub fn tool_error_ev(tc: &StreamToolCall, msg: &str, duration: &str) -> Self {
        Event {
            event_type: "tool_error".into(),
            tool_call_id: tc.id.clone(),
            tool_call_name: tc.name.clone(),
            tool_error: msg.into(),
            tool_duration: duration.into(),
            ..Default::default()
        }
    }
    pub fn tool_error_deferred(tc: &StreamToolCall, msg: &str) -> Self {
        Event {
            event_type: "tool_error".into(),
            tool_call_id: tc.id.clone(),
            tool_call_name: tc.name.clone(),
            tool_error: msg.into(),
            ..Default::default()
        }
    }
    pub fn error_msg(msg: &str) -> Self {
        Event {
            event_type: "error".into(),
            error: msg.into(),
            ..Default::default()
        }
    }
    pub fn retry_ev(msg: &str) -> Self {
        Event {
            event_type: "retry".into(),
            error: msg.into(),
            ..Default::default()
        }
    }
    pub fn retry_available(msg: &str) -> Self {
        Event {
            event_type: "retry_available".into(),
            error: msg.into(),
            ..Default::default()
        }
    }
    pub fn agent_done(finish_reason: &str) -> Self {
        Event {
            event_type: "agent_done".into(),
            finish_reason: finish_reason.into(),
            ..Default::default()
        }
    }
    pub fn usage_ev(usage: Usage) -> Self {
        Event {
            event_type: "usage".into(),
            usage: Some(usage),
            ..Default::default()
        }
    }
}

// ── SubagentDef ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SubagentDef {
    pub id: String,
    pub description: String,
    pub client: Arc<dyn ChatClient>,
    pub registry: Arc<Registry>,
    pub system_prompt: String,
    pub model_name: String,
}

// ── Internal types ───────────────────────────────────────────────────────

#[derive(Clone)]
struct SubagentTurn {
    msgs: Vec<Message>,
    subagent: String,
    tool_call_id: String,
}

struct PendingTool {
    call: StreamToolCall,
    result: String,
    duration: String,
    #[allow(dead_code)]
    had_error: bool,
}

struct RoundResult {
    content: String,
    reasoning: String,
    reasoning_details: Vec<ReasoningDetail>,
    tool_calls: Vec<StreamToolCall>,
    finish_reason: String,
}

// ── AgentSession ─────────────────────────────────────────────────────────

pub struct AgentSession {
    store: Arc<TokioMutex<Store>>,
    sess: Session,
    tools: Arc<Registry>,
    client: Arc<dyn ChatClient>,
    system_msg: String,
    max_rounds: i32,
    cwd: String,

    total_prompt_tokens: i32,
    total_completion_tokens: i32,
    total_cost: f64,
    last_request_tokens: i32,
    last_total_tokens: i32,
    cached_tool_defs: Vec<ToolDef>,

    working_messages: Vec<Message>,

    cached_ancestry_msgs: Vec<Message>,
    cached_ancestry_turn_id: String,

    subagents: HashMap<String, SubagentDef>,
    subagent_turns: Vec<SubagentTurn>,

    is_subagent: bool,
    last_prompt: String,
    captured_msgs: Vec<Message>,

    compaction_client: Option<Arc<dyn ChatClient>>,
    compacting: bool,
    compacted_summary: String,
    compacted_up_to: String,
}

// ── build_system_prompt ──────────────────────────────────────────────────

pub fn build_system_prompt(
    system_md: &str,
    global_md: &str,
    local_md: &str,
    system_prefix: &str,
) -> String {
    let mut sb = String::new();
    if !system_md.is_empty() {
        sb.push_str(system_md);
        sb.push_str("\n\n");
    }
    if !system_prefix.is_empty() {
        sb.push_str(system_prefix);
        sb.push_str("\n\n");
    }
    sb.push_str(TOOL_RULES_PROMPT);
    if !global_md.is_empty() {
        sb.push_str("\n\n## User conventions\n");
        sb.push_str(global_md);
    }
    if !local_md.is_empty() {
        sb.push_str("\n\n## Project conventions\n");
        sb.push_str(local_md);
    }
    sb
}

// ── Constructor ──────────────────────────────────────────────────────────

impl AgentSession {
    pub fn new(
        store: Arc<TokioMutex<Store>>,
        sess: Session,
        client: Arc<dyn ChatClient>,
        registry: Arc<Registry>,
        system_prompt: String,
        max_rounds: i32,
        cwd: String,
    ) -> Self {
        let now = chrono::Local::now();
        let date_str = now.format("%Y-%m-%d (%A)").to_string();
        let system_msg = format!("CWD: {}\nDate: {}\n\n{}", cwd, date_str, system_prompt);
        let cached_tool_defs = registry.tool_defs();

        AgentSession {
            total_prompt_tokens: sess.prompt_tokens,
            total_completion_tokens: sess.completion_tokens,
            total_cost: sess.cost,
            last_total_tokens: sess.context_tokens,
            compacted_summary: sess.compacted_summary.clone(),
            compacted_up_to: sess.compacted_up_to.clone(),
            store,
            sess,
            tools: registry,
            client,
            system_msg,
            max_rounds,
            cwd,
            last_request_tokens: 0,
            cached_tool_defs,
            working_messages: Vec::new(),
            cached_ancestry_msgs: Vec::new(),
            cached_ancestry_turn_id: String::new(),
            subagents: HashMap::new(),
            subagent_turns: Vec::new(),
            is_subagent: false,
            last_prompt: String::new(),
            captured_msgs: Vec::new(),
            compaction_client: None,
            compacting: false,
        }
    }

    // ── Accessors ────────────────────────────────────────────────────────

    pub fn sess(&self) -> Session {
        self.sess.clone()
    }
    pub fn reload_from(&mut self, sess: Session) {
        self.total_prompt_tokens = sess.prompt_tokens;
        self.total_completion_tokens = sess.completion_tokens;
        self.total_cost = sess.cost;
        self.last_total_tokens = sess.context_tokens;
        self.compacted_summary = sess.compacted_summary.clone();
        self.compacted_up_to = sess.compacted_up_to.clone();
        self.sess = sess;
    }
    pub fn system_prompt(&self) -> &str {
        &self.system_msg
    }
    pub fn context_window(&self) -> i32 {
        self.client.context_window()
    }
    pub fn context_tokens(&self) -> i32 {
        self.last_total_tokens
    }
    pub fn tool_defs(&self) -> Vec<ToolDef> {
        self.cached_tool_defs.clone()
    }

    // ── Subagents / Compaction ───────────────────────────────────────────

    pub fn set_subagents(&mut self, defs: HashMap<String, SubagentDef>) {
        if defs.is_empty() {
            return;
        }
        let mut names: Vec<String> = defs.keys().cloned().collect();
        names.sort();
        let descriptions: HashMap<String, String> = defs
            .iter()
            .map(|(k, v)| (k.clone(), v.description.clone()))
            .collect();
        self.subagents = defs;
        self.cached_tool_defs
            .push(build_delegate_def(&names, &descriptions));
    }

    pub fn set_compaction_client(&mut self, client: Arc<dyn ChatClient>) {
        self.compaction_client = Some(client);
    }

    pub fn set_client(&mut self, client: Arc<dyn ChatClient>) {
        self.client = client;
    }

    // ── prompt / retry ───────────────────────────────────────────────────

    pub fn prompt(&mut self, user_text: &str) -> mpsc::Receiver<Event> {
        self.last_prompt = user_text.to_string();
        let (tx, rx) = mpsc::channel(100);
        let mut s = self.clone();
        let ut = user_text.to_string();
        tokio::spawn(async move {
            s.run_loop(&ut, &tx).await;
        });
        rx
    }

    pub fn retry(&self) -> Result<mpsc::Receiver<Event>, String> {
        if self.last_prompt.is_empty() {
            return Err("no prompt to retry".into());
        }
        let (tx, rx) = mpsc::channel(100);
        let mut s = self.clone();
        let ut = s.last_prompt.clone();
        tokio::spawn(async move {
            s.run_loop(&ut, &tx).await;
        });
        Ok(rx)
    }

    pub fn compact(&self) -> Result<mpsc::Receiver<Event>, String> {
        if self.compaction_client.is_none() {
            return Err("no compaction client configured".into());
        }
        let (tx, rx) = mpsc::channel(100);
        let mut s = self.clone();
        tokio::spawn(async move {
            s.compacting = true;
            let _ = tx
                .send(Event {
                    event_type: "compacting_start".into(),
                    ..Default::default()
                })
                .await;
            let turns = {
                let mut store = s.store.lock().await;
                store
                    .ancestry(&s.sess.id, &s.sess.current_turn)
                    .map_err(|e| e.to_string())
            };
            match turns {
                Ok(turns) => {
                    let _ = s.compact_ancestry(&turns, true).await;
                    let _ = tx.send(Event::agent_done("compacted")).await;
                }
                Err(e) => {
                    let _ = tx.send(Event::error_msg(&e)).await;
                }
            }
        });
        Ok(rx)
    }

    fn clone(&self) -> Self {
        AgentSession {
            store: self.store.clone(),
            sess: self.sess.clone(),
            tools: self.tools.clone(),
            client: self.client.clone(),
            system_msg: self.system_msg.clone(),
            max_rounds: self.max_rounds,
            cwd: self.cwd.clone(),
            total_prompt_tokens: self.total_prompt_tokens,
            total_completion_tokens: self.total_completion_tokens,
            total_cost: self.total_cost,
            last_request_tokens: self.last_request_tokens,
            last_total_tokens: self.last_total_tokens,
            cached_tool_defs: self.cached_tool_defs.clone(),
            working_messages: self.working_messages.clone(),
            cached_ancestry_msgs: self.cached_ancestry_msgs.clone(),
            cached_ancestry_turn_id: self.cached_ancestry_turn_id.clone(),
            subagents: self.subagents.clone(),
            subagent_turns: self.subagent_turns.clone(),
            is_subagent: self.is_subagent,
            last_prompt: self.last_prompt.clone(),
            captured_msgs: self.captured_msgs.clone(),
            compaction_client: self.compaction_client.clone(),
            compacting: self.compacting,
            compacted_summary: self.compacted_summary.clone(),
            compacted_up_to: self.compacted_up_to.clone(),
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    //  RUN LOOP
    // ══════════════════════════════════════════════════════════════════════

    async fn run_loop(&mut self, user_text: &str, events: &mpsc::Sender<Event>) {
        let loop_start = std::time::Instant::now();
        self.working_messages.clear();
        self.subagent_turns.clear();
        let parent_id = self.sess.current_turn.clone();

        self.working_messages.push(Message {
            role: Role::User,
            content: user_text.into(),
            reasoning_content: String::new(),
            reasoning_details: vec![],
            tool_calls: vec![],
            tool_call_id: String::new(),
            name: String::new(),
            tool_duration: String::new(),
        });

        let t0 = std::time::Instant::now();
        let ancestry_msgs = {
            let turns_result = {
                let mut store = self.store.lock().await;
                store
                    .ancestry(&self.sess.id, &parent_id)
                    .map_err(|e| e.to_string())
            };
            match turns_result {
                Ok(turns) => {
                    let ancestry_msgs = self.compact_ancestry(&turns, false).await;
                    log::debug!(
                        "agent phase phase=compact dur={:?} turns={}",
                        t0.elapsed(),
                        turns.len()
                    );
                    ancestry_msgs
                }
                Err(e) => {
                    log::warn!("ancestry load failed error={}", e);
                    Vec::new()
                }
            }
        };
        self.cached_ancestry_msgs = ancestry_msgs.clone();
        self.cached_ancestry_turn_id = parent_id.clone();

        for round in 1..=self.max_rounds {
            let req = self.build_request(&ancestry_msgs);
            let t0 = std::time::Instant::now();

            match self.stream_round(req, events).await {
                Ok(result) => {
                    let stream_dur = t0.elapsed();
                    if !result.tool_calls.is_empty() {
                        if !result.content.is_empty() {
                            self.working_messages.push(Message {
                                role: Role::Assistant,
                                content: result.content.clone(),
                                reasoning_content: result.reasoning.clone(),
                                reasoning_details: vec![],
                                tool_calls: vec![],
                                tool_call_id: String::new(),
                                name: String::new(),
                                tool_duration: String::new(),
                            });
                        }
                        let t0 = std::time::Instant::now();
                        let pending = self.execute_tools(&result.tool_calls, events).await;
                        log::debug!(
                            "agent phase phase=round round={} stream={:?} tools={:?} n_tools={}",
                            round,
                            stream_dur,
                            t0.elapsed(),
                            result.tool_calls.len()
                        );
                        let msg_reasoning = if result.content.is_empty() {
                            result.reasoning.clone()
                        } else {
                            String::new()
                        };
                        self.append_tool_messages(
                            &pending,
                            &msg_reasoning,
                            &result.reasoning_details,
                        );
                        continue;
                    }

                    if !result.content.is_empty()
                        || !result.reasoning.is_empty()
                        || !result.reasoning_details.is_empty()
                    {
                        self.working_messages.push(Message {
                            role: Role::Assistant,
                            content: result.content,
                            reasoning_content: result.reasoning,
                            reasoning_details: result.reasoning_details,
                            tool_calls: vec![],
                            tool_call_id: String::new(),
                            name: String::new(),
                            tool_duration: String::new(),
                        });
                    }
                    let t0 = std::time::Instant::now();
                    self.commit_turn(&parent_id).await;
                    log::debug!(
                        "agent phase phase=commit dur={:?} total={:?}",
                        t0.elapsed(),
                        loop_start.elapsed()
                    );

                    let fr = if result.finish_reason.is_empty() {
                        "stop"
                    } else {
                        &result.finish_reason
                    };
                    let _ = events.send(Event::agent_done(fr)).await;
                    return;
                }
                Err(e) => {
                    if e.retryable() {
                        let _ = events.send(Event::retry_available(&e.to_string())).await;
                    } else {
                        let _ = events.send(Event::error_msg(&e.to_string())).await;
                    }
                    return;
                }
            }
        }

        self.commit_turn(&parent_id).await;
        let _ = events.send(Event::agent_done("max_rounds")).await;
    }

    // ── build_request ────────────────────────────────────────────────────

    fn build_request(&self, ancestry_msgs: &[Message]) -> ChatRequest {
        let mut msgs = Vec::with_capacity(ancestry_msgs.len() + self.working_messages.len() + 2);
        msgs.push(Message {
            role: Role::System,
            content: self.system_msg.clone(),
            reasoning_content: String::new(),
            reasoning_details: vec![],
            tool_calls: vec![],
            tool_call_id: String::new(),
            name: String::new(),
            tool_duration: String::new(),
        });
        msgs.extend_from_slice(ancestry_msgs);
        msgs.extend_from_slice(&self.working_messages);

        ChatRequest {
            model: String::new(),
            messages: msgs,
            tools: self.cached_tool_defs.clone(),
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
            session_id: self.sess.id.clone(),
        }
    }

    // ── stream_round ─────────────────────────────────────────────────────

    async fn stream_round(
        &mut self,
        req: ChatRequest,
        events: &mpsc::Sender<Event>,
    ) -> Result<RoundResult, ProviderError> {
        const MAX_ATTEMPTS: usize = 5;
        const MAX_DELAY: Duration = Duration::from_secs(30);

        let mut rx = None;
        for attempt in 0..MAX_ATTEMPTS {
            match self.client.chat(req.clone()).await {
                Ok(r) => {
                    rx = Some(r);
                    break;
                }
                Err(e) => {
                    if !e.retryable() {
                        return Err(e);
                    }
                    if attempt < MAX_ATTEMPTS - 1 {
                        let shift = 1u64 << attempt;
                        let delay = Duration::from_secs(2 * shift).min(MAX_DELAY);
                        let jitter =
                            delay.as_secs_f64() * 0.25 * (2.0 * rand::rng().random::<f64>() - 1.0);
                        let delay = Duration::from_secs_f64(delay.as_secs_f64() + jitter);
                        let _ = events
                            .send(Event::retry_ev(&format!(
                                "Attempt {}/{} failed: {} — retrying in {}",
                                attempt + 1,
                                MAX_ATTEMPTS,
                                e,
                                format_duration(delay)
                            )))
                            .await;
                        tokio::time::sleep(delay).await;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        let mut rx = rx.ok_or_else(|| ProviderError {
            status_code: 0,
            body: "chat returned no stream".into(),
        })?;
        let mut result = RoundResult {
            content: String::new(),
            reasoning: String::new(),
            reasoning_details: vec![],
            tool_calls: vec![],
            finish_reason: String::new(),
        };

        while let Some(ev) = rx.recv().await {
            if let Some(ref err) = ev.error {
                return Err(err.clone());
            }

            match ev.event_type.as_str() {
                "text_delta" => {
                    result.content.push_str(&ev.delta);
                    let _ = events.send(Event::text_delta(&ev.delta)).await;
                }
                "reasoning_delta" => {
                    result.reasoning.push_str(&ev.reasoning_delta);
                    let _ = events
                        .send(Event::reasoning_delta(&ev.reasoning_delta))
                        .await;
                }
                "tool_call" => {
                    if let Some(ref tc) = ev.tool_call {
                        result.tool_calls.push(tc.clone());
                    }
                }
                "usage" => {
                    if let Some(ref usage) = ev.usage {
                        self.total_prompt_tokens += usage.prompt_tokens;
                        self.total_completion_tokens += usage.completion_tokens;
                        self.last_request_tokens = usage.prompt_tokens;
                        self.last_total_tokens = usage.total_tokens;
                        if self.last_total_tokens == 0 {
                            self.last_total_tokens = usage.prompt_tokens + usage.completion_tokens;
                        }
                        if usage.cost > 0.0 {
                            self.total_cost += usage.cost;
                        } else {
                            let (in_p, cache_p, out_p) = self.client.pricing();
                            if in_p > 0.0 || cache_p > 0.0 || out_p > 0.0 {
                                let cached = usage.prompt_cache_hit_tokens.min(usage.prompt_tokens);
                                let non_cached = usage.prompt_tokens - cached;
                                self.total_cost += non_cached as f64 * in_p / 1e6
                                    + cached as f64 * cache_p / 1e6
                                    + usage.completion_tokens as f64 * out_p / 1e6;
                            }
                        }
                        let _ = events.send(Event::usage_ev(usage.clone())).await;
                    }
                }
                "reasoning_details" => {
                    result.reasoning_details = ev.reasoning_details.clone();
                }
                "finish_reason" => {
                    result.finish_reason = ev.finish_reason.clone();
                }
                _ => {}
            }
        }
        Ok(result)
    }

    // ── execute_tools ────────────────────────────────────────────────────

    async fn execute_tools(
        &mut self,
        tool_calls: &[StreamToolCall],
        events: &mpsc::Sender<Event>,
    ) -> Vec<PendingTool> {
        let n = tool_calls.len();

        // Step 1: emit all tool_call_start FIRST
        for tc in tool_calls {
            let _ = events.send(Event::tool_call_start(tc)).await;
        }

        // Step 2: execute all tools concurrently via JoinSet
        let mut set = tokio::task::JoinSet::new();

        for (i, tc) in tool_calls.iter().enumerate() {
            let tc = tc.clone();
            let events = events.clone();

            if tc.name == "delegate" && !self.subagents.is_empty() {
                let subagents = self.subagents.clone();
                let store = self.store.clone();
                let sess_id = self.sess.id.clone();
                let max_rounds = self.max_rounds;
                let cwd = self.cwd.clone();

                set.spawn(async move {
                    let start = std::time::Instant::now();
                    let tc_clone = tc.clone();
                    let events_clone = events.clone();
                    let (result, dd) =
                        run_delegate(tc, subagents, store, sess_id, max_rounds, cwd, events).await;
                    let dur = format_duration(start.elapsed());
                    let _ = events_clone
                        .send(Event::tool_result_ev(&tc_clone, &result, &dur))
                        .await;
                    (i, result, dur, false, Some(dd))
                });
            } else {
                let tools = self.tools.clone();

                set.spawn(async move {
                    let start = std::time::Instant::now();
                    match tools.get(&tc.name) {
                        None => {
                            let msg = format!("Tool {} not found", tc.name);
                            (i, msg, String::new(), true, None)
                        }
                        Some(tool) => {
                            let args: serde_json::Value =
                                serde_json::from_str(&tc.arguments).unwrap_or_default();
                            let tool_result = tokio::time::timeout(
                                Duration::from_secs(120),
                                (tool.execute)(args),
                            )
                            .await;
                            let dur = format_duration(start.elapsed());
                            match tool_result {
                                Ok(Ok(result)) => {
                                    let final_result = result;
                                    let _ = events
                                        .send(Event::tool_result_ev(&tc, &final_result, &dur))
                                        .await;
                                    (i, final_result, dur, false, None)
                                }
                                Ok(Err(e)) => {
                                    let msg = format!("Tool {} error: {}", tc.name, e);
                                    (i, msg, dur, true, None)
                                }
                                Err(_) => {
                                    let msg = format!("Tool {} timed out after 120s", tc.name);
                                    (i, msg, dur, true, None)
                                }
                            }
                        }
                    }
                });
            }
        }

        // Collect results
        let mut pending: Vec<PendingTool> = (0..n)
            .map(|i| PendingTool {
                call: tool_calls[i].clone(),
                result: String::new(),
                duration: String::new(),
                had_error: false,
            })
            .collect();
        let mut deferred_errors: Vec<Event> = Vec::new();

        while let Some(result) = set.join_next().await {
            if let Ok((i, result_str, dur, is_error, delegate_data)) = result {
                pending[i].result = result_str;
                pending[i].duration = dur;
                pending[i].had_error = is_error;
                if is_error {
                    deferred_errors.push(Event::tool_error_ev(
                        &pending[i].call,
                        &pending[i].result,
                        &pending[i].duration,
                    ));
                }
                if let Some(dd) = delegate_data {
                    self.total_prompt_tokens += dd.prompt_tokens;
                    self.total_completion_tokens += dd.completion_tokens;
                    self.total_cost += dd.cost;
                    self.subagent_turns.push(SubagentTurn {
                        msgs: dd.msgs,
                        subagent: dd.subagent_id,
                        tool_call_id: dd.tool_call_id,
                    });
                }
            }
        }

        // Step 3: emit deferred errors AFTER all complete
        for ev in deferred_errors {
            let _ = events.send(ev).await;
        }

        pending
    }

    // ── append_tool_messages ─────────────────────────────────────────────

    fn append_tool_messages(
        &mut self,
        pending: &[PendingTool],
        reasoning: &str,
        details: &[ReasoningDetail],
    ) {
        let assistant_tool_calls: Vec<ToolCall> = pending
            .iter()
            .map(|pt| ToolCall {
                id: pt.call.id.clone(),
                call_type: "function".into(),
                function: ToolCallFunction {
                    name: pt.call.name.clone(),
                    arguments: pt.call.arguments.clone(),
                },
            })
            .collect();

        self.working_messages.push(Message {
            role: Role::Assistant,
            content: String::new(),
            reasoning_content: reasoning.into(),
            reasoning_details: details.to_vec(),
            tool_calls: assistant_tool_calls,
            tool_call_id: String::new(),
            name: String::new(),
            tool_duration: String::new(),
        });

        for pt in pending {
            let content = if pt.result.is_empty() {
                "(no output)".into()
            } else {
                pt.result.clone()
            };
            self.working_messages.push(Message {
                role: Role::Tool,
                content,
                reasoning_content: String::new(),
                reasoning_details: vec![],
                tool_calls: vec![],
                tool_call_id: pt.call.id.clone(),
                name: pt.call.name.clone(),
                tool_duration: pt.duration.clone(),
            });
        }
    }

    // ── commit_turn ──────────────────────────────────────────────────────

    async fn commit_turn(&mut self, parent_id: &str) {
        if self.is_subagent {
            self.captured_msgs = self.working_messages.clone();
            self.working_messages.clear();
            return;
        }

        let turn_id = compute_turn_id(parent_id, &self.working_messages);
        let turn = Turn {
            id: turn_id.clone(),
            parent_id: parent_id.into(),
            messages: self.working_messages.clone(),
            created_at: Utc::now(),
            subagent: String::new(),
            tool_call_id: String::new(),
        };

        {
            let mut store = self.store.lock().await;
            let _ = store.commit_turn(&self.sess.id, &turn);

            for st in &self.subagent_turns {
                let sub_id = compute_turn_id(&turn_id, &st.msgs);
                let sub_turn = Turn {
                    id: sub_id,
                    parent_id: turn_id.clone(),
                    messages: st.msgs.clone(),
                    created_at: Utc::now(),
                    subagent: st.subagent.clone(),
                    tool_call_id: st.tool_call_id.clone(),
                };
                let _ = store.commit_turn(&self.sess.id, &sub_turn);
            }
        }

        self.sess.current_turn = turn_id;
        self.cached_ancestry_turn_id.clear();
        self.sess.turn_count += 1;
        self.sess.prompt_tokens = self.total_prompt_tokens;
        self.sess.completion_tokens = self.total_completion_tokens;
        self.sess.total_tokens = self.total_prompt_tokens + self.total_completion_tokens;
        self.sess.context_tokens = self.last_total_tokens;
        self.sess.cost = self.total_cost;

        if !self.sess.named {
            let label = turn_label(&self.working_messages);
            if label != "(empty)" {
                let store = self.store.lock().await;
                store.set_name(&mut self.sess, &label);
            }
        }

        self.sess.compacted_summary = self.compacted_summary.clone();
        self.sess.compacted_up_to = self.compacted_up_to.clone();

        {
            let mut store = self.store.lock().await;
            let _ = store.save_meta(&self.sess);
        }

        self.working_messages.clear();
    }

    // ── compact_ancestry ─────────────────────────────────────────────────

    async fn compact_ancestry(&mut self, turns: &[Turn], force: bool) -> Vec<Message> {
        let compaction_client = match &self.compaction_client {
            Some(cc) => cc,
            None => return flatten_turns(turns),
        };

        let ctx_window = self.client.context_window();
        let above_threshold = force || self.last_total_tokens > ctx_window * 7 / 10;
        let keep_turns = compute_keep_turns(turns, ctx_window);

        if force && turns.len() <= 1 {
            return flatten_turns(turns);
        }

        let force_keep = if force { 0 } else { keep_turns };

        if self.compacted_summary.is_empty() {
            if !above_threshold {
                return flatten_turns(turns);
            }
            if turns.len() <= force_keep {
                return flatten_turns(turns);
            }

            let split = turns.len() - force_keep;
            let old = &turns[..split];
            let recent = &turns[split..];

            return match call_compaction(compaction_client, &build_summarization_prompt(old)).await
            {
                Ok(summary) => {
                    self.compacted_summary = summary.clone();
                    self.compacted_up_to = old[old.len() - 1].id.clone();
                    build_compacted_messages(&summary, &flatten_turns(recent))
                }
                Err(e) => {
                    log::warn!("compaction failed, using full ancestry error={}", e);
                    flatten_turns(turns)
                }
            };
        }

        let new_turns = turns_after_id(turns, &self.compacted_up_to);
        if new_turns.is_empty() {
            if turns.len() <= force_keep {
                return build_compacted_messages(&self.compacted_summary, &flatten_turns(turns));
            }
            let split = turns.len() - force_keep;
            let old = &turns[..split];
            let recent = &turns[split..];
            return match call_compaction(compaction_client, &build_summarization_prompt(old)).await
            {
                Ok(summary) => {
                    self.compacted_summary = summary.clone();
                    self.compacted_up_to = old[old.len() - 1].id.clone();
                    build_compacted_messages(&summary, &flatten_turns(recent))
                }
                Err(e) => {
                    log::warn!("compaction failed, reusing stale summary error={}", e);
                    build_compacted_messages(&self.compacted_summary, &flatten_turns(turns))
                }
            };
        }

        if !above_threshold {
            return build_compacted_messages(&self.compacted_summary, &flatten_turns(new_turns));
        }

        let keep_new = compute_keep_turns(new_turns, ctx_window);
        if new_turns.len() <= keep_new {
            return build_compacted_messages(&self.compacted_summary, &flatten_turns(new_turns));
        }

        let split = new_turns.len() - keep_new;
        let summary_turns = &new_turns[..split];
        let recent_turns = &new_turns[split..];

        match call_compaction(
            compaction_client,
            &build_incremental_summary_prompt(&self.compacted_summary, summary_turns),
        )
        .await
        {
            Ok(summary) => {
                self.compacted_summary = summary.clone();
                self.compacted_up_to = summary_turns[summary_turns.len() - 1].id.clone();
                build_compacted_messages(&summary, &flatten_turns(recent_turns))
            }
            Err(e) => {
                log::warn!("compaction failed, reusing stale summary error={}", e);
                build_compacted_messages(&self.compacted_summary, &flatten_turns(new_turns))
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
//  HELPERS
// ══════════════════════════════════════════════════════════════════════════

fn flatten_turns(turns: &[Turn]) -> Vec<Message> {
    let mut msgs = Vec::new();
    for t in turns {
        msgs.extend_from_slice(&t.messages);
    }
    msgs
}

fn build_compacted_messages(summary: &str, recent_msgs: &[Message]) -> Vec<Message> {
    let mut out = Vec::with_capacity(recent_msgs.len() + 1);
    out.push(Message {
        role: Role::System,
        content: summary.into(),
        reasoning_content: String::new(),
        reasoning_details: vec![],
        tool_calls: vec![],
        tool_call_id: String::new(),
        name: String::new(),
        tool_duration: String::new(),
    });
    out.extend_from_slice(recent_msgs);
    out
}

fn turns_after_id<'a>(turns: &'a [Turn], id: &str) -> &'a [Turn] {
    for (i, t) in turns.iter().enumerate() {
        if t.id == id {
            return &turns[i + 1..];
        }
    }
    &[]
}

fn truncate_for_summary(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.into();
    }
    let cut = max_len.saturating_sub(3);
    let boundary = s
        .char_indices()
        .take_while(|&(i, _)| i <= cut)
        .map(|(i, _)| i)
        .last()
        .unwrap_or(0);
    format!("{}...", &s[..boundary])
}

fn compute_keep_turns(turns: &[Turn], context_window: i32) -> usize {
    let budget = context_window / 3;
    let mut total: i32 = 0;
    let mut kept: usize = 0;
    for t in turns.iter().rev() {
        for msg in &t.messages {
            total += msg.content.len() as i32 + msg.reasoning_content.len() as i32;
        }
        if total > budget {
            break;
        }
        kept += 1;
    }
    const FLOOR: usize = 3;
    if kept < FLOOR {
        kept = FLOOR;
    }
    if kept > turns.len() {
        kept = turns.len();
    }
    kept
}

pub fn format_duration(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        let ms = d.as_millis() as u64;
        return format!("{}ms", ms);
    }
    // Round to 10ms
    let total_ns = d.as_nanos();
    let rounded_ns = total_ns / 10_000_000 * 10_000_000;
    let rounded = Duration::from_nanos(rounded_ns as u64);
    let ns = rounded.as_nanos();
    let hour_ns: u128 = 3600 * 1_000_000_000;
    let min_ns: u128 = 60 * 1_000_000_000;
    let sec_ns: u128 = 1_000_000_000;
    if ns >= hour_ns {
        let h = ns / hour_ns;
        let rem = ns % hour_ns;
        let m = rem / min_ns;
        let rem2 = rem % min_ns;
        let s = rem2 / sec_ns;
        return format!("{}h{}m{}s", h, m, s);
    }
    if ns >= min_ns {
        let m = ns / min_ns;
        let rem = ns % min_ns;
        if rem == 0 {
            return format!("{}m0s", m);
        }
        let s = rem / sec_ns;
        let frac_ns = rem % sec_ns;
        let sec_str = fmt_sec_fractional(s, frac_ns);
        return format!("{}m{}s", m, sec_str);
    }
    let s = ns / sec_ns;
    let frac_ns = ns % sec_ns;
    if frac_ns == 0 {
        format!("{}s", s)
    } else {
        format!("{}s", fmt_sec_fractional(s, frac_ns))
    }
}

fn fmt_sec_fractional(secs: u128, frac_ns: u128) -> String {
    let ds = format!("{:09}", frac_ns);
    let trimmed = ds.trim_end_matches('0');
    if trimmed.is_empty() {
        format!("{}", secs)
    } else {
        format!("{}.{}", secs, trimmed)
    }
}

// ── run_delegate ────────────────────────────────────────────────────────

struct DelegateData {
    prompt_tokens: i32,
    completion_tokens: i32,
    cost: f64,
    msgs: Vec<Message>,
    subagent_id: String,
    tool_call_id: String,
}

fn run_delegate(
    tc: StreamToolCall,
    subagents: HashMap<String, SubagentDef>,
    store: Arc<TokioMutex<Store>>,
    sess_id: String,
    max_rounds: i32,
    cwd: String,
    parent_events: mpsc::Sender<Event>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = (String, DelegateData)> + Send>> {
    Box::pin(async move {
        #[derive(Deserialize)]
        struct DelegateParams {
            subagent: String,
            task: String,
            #[serde(default)]
            context: String,
        }

        let params: DelegateParams = match serde_json::from_str(&tc.arguments) {
            Ok(p) => p,
            Err(e) => {
                return (
                    format!("invalid delegate params: {}", e),
                    DelegateData {
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        cost: 0.0,
                        msgs: vec![],
                        subagent_id: String::new(),
                        tool_call_id: tc.id.clone(),
                    },
                )
            }
        };

        let def = match subagents.get(&params.subagent) {
            Some(d) => d.clone(),
            None => {
                return (
                    format!("subagent {:?} not found", params.subagent),
                    DelegateData {
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        cost: 0.0,
                        msgs: vec![],
                        subagent_id: String::new(),
                        tool_call_id: tc.id.clone(),
                    },
                )
            }
        };

        let mut task_text = params.task;
        if !params.context.is_empty() {
            task_text.push_str("\n\n");
            task_text.push_str(&params.context);
        }

        let system_msg = format!("CWD: {}\n\n{}", cwd, def.system_prompt);

        let mut sub = AgentSession {
            store,
            sess: Session {
                id: sess_id,
                name: String::new(),
                hash: String::new(),
                named: false,
                current_turn: String::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                turn_count: 0,
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                context_tokens: 0,
                cost: 0.0,
                compacted_summary: String::new(),
                compacted_up_to: String::new(),
            },
            tools: def.registry.clone(),
            client: def.client.clone(),
            system_msg,
            max_rounds,
            cwd,
            is_subagent: true,
            cached_tool_defs: def.registry.tool_defs(),
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            total_cost: 0.0,
            last_request_tokens: 0,
            last_total_tokens: 0,
            working_messages: Vec::new(),
            cached_ancestry_msgs: Vec::new(),
            cached_ancestry_turn_id: String::new(),
            subagents: HashMap::new(),
            subagent_turns: Vec::new(),
            last_prompt: String::new(),
            captured_msgs: Vec::new(),
            compaction_client: None,
            compacting: false,
            compacted_summary: String::new(),
            compacted_up_to: String::new(),
        };

        let (event_tx, mut event_rx) = mpsc::channel(100);
        let sub_id = def.id.clone();
        let tc_id = tc.id.clone();

        let join_handle = tokio::spawn(async move {
            sub.run_loop(&task_text, &event_tx).await;
            (
                sub.total_prompt_tokens,
                sub.total_completion_tokens,
                sub.total_cost,
                sub.captured_msgs,
                sub.working_messages,
            )
        });

        let mut had_error = false;
        while let Some(mut ev) = event_rx.recv().await {
            match ev.event_type.as_str() {
                "agent_done" | "usage" | "text_delta" | "reasoning_delta" => {}
                "error" => {
                    had_error = true;
                    ev.subagent = sub_id.clone();
                    ev.subagent_id = tc_id.clone();
                    let _ = parent_events.send(ev).await;
                }
                _ => {
                    ev.subagent = sub_id.clone();
                    ev.subagent_id = tc_id.clone();
                    let _ = parent_events.send(ev).await;
                }
            }
        }

        let (prompt_tokens, completion_tokens, cost, captured_msgs, working_msgs) = join_handle
            .await
            .unwrap_or((0, 0, 0.0, Vec::new(), Vec::new()));

        let msgs = if captured_msgs.is_empty() {
            working_msgs
        } else {
            captured_msgs
        };

        if had_error {
            return (
                "(subagent encountered an error)".into(),
                DelegateData {
                    prompt_tokens,
                    completion_tokens,
                    cost,
                    msgs,
                    subagent_id: sub_id,
                    tool_call_id: tc_id,
                },
            );
        }

        let content = extract_final_content(&msgs);
        let final_result = if content.is_empty() {
            "(subagent produced no output)".into()
        } else {
            content
        };

        (
            final_result,
            DelegateData {
                prompt_tokens,
                completion_tokens,
                cost,
                msgs,
                subagent_id: sub_id,
                tool_call_id: tc_id,
            },
        )
    })
}

fn extract_final_content(msgs: &[Message]) -> String {
    for msg in msgs.iter().rev() {
        if msg.role == Role::Assistant && !msg.content.is_empty() {
            return msg.content.clone();
        }
    }
    String::new()
}

// ── build_delegate_def ────────────────────────────────────────────────────

fn build_delegate_def(names: &[String], descriptions: &HashMap<String, String>) -> ToolDef {
    let mut desc = String::from("The subagent to delegate to. Available: ");
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            desc.push_str(" | ");
        }
        desc.push_str(name);
        if let Some(d) = descriptions.get(name) {
            if !d.is_empty() {
                desc.push_str(": ");
                desc.push_str(d);
            }
        }
    }

    let mut params = HashMap::new();
    params.insert("type".into(), serde_json::json!("object"));
    let mut props: HashMap<String, serde_json::Value> = HashMap::new();
    props.insert(
        "subagent".into(),
        serde_json::json!({
            "type": "string", "enum": names, "description": desc
        }),
    );
    props.insert("task".into(), serde_json::json!({
        "type": "string", "description": "The task for the subagent. Be specific and include all necessary details."
    }));
    props.insert("context".into(), serde_json::json!({
        "type": "string", "description": "Additional context (file contents, requirements, constraints). Optional."
    }));
    params.insert("properties".into(), serde_json::json!(props));
    params.insert("required".into(), serde_json::json!(["subagent", "task"]));

    ToolDef {
        def_type: "function".into(),
        function: crate::message::ToolDefFunction {
            name: "delegate".into(),
            description: "Delegate a task to a subagent with its own model and tools. The subagent runs to completion and returns its result. Use for complex coding tasks or research that would benefit from a specialized model.".into(),
            parameters: params,
        },
    }
}

// ── compaction helpers ────────────────────────────────────────────────────

async fn call_compaction(client: &Arc<dyn ChatClient>, prompt: &str) -> Result<String, String> {
    let req = ChatRequest {
        model: String::new(),
        messages: vec![Message {
            role: Role::User,
            content: prompt.into(),
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

    let mut rx = client.chat(req).await.map_err(|e| e.to_string())?;
    let mut result = String::new();
    while let Some(ev) = rx.recv().await {
        if let Some(err) = ev.error {
            return Err(err.to_string());
        }
        if ev.event_type == "text_delta" {
            result.push_str(&ev.delta);
        }
    }
    Ok(result)
}

fn build_summarization_prompt(turns: &[Turn]) -> String {
    let mut sb = String::new();
    sb.push_str("You are writing a minimal handoff document. Another agent will continue the work using only this document. Extract only what's needed to continue — discard exploration, dead ends, and intermediate reasoning.\n\n");
    sb.push_str("Format:\n\n## Goal\nThe objective in 1-2 sentences.\n\n## Done\nWhat was accomplished. One bullet per change, with file path and what changed.\n\n## State\nCurrent state: what works, what's broken, any errors or blockers.\n\n## Next\nImmediate next steps or open questions.\n\nOmit empty sections.\n\n---\n\n");
    format_turns_for_summary(&mut sb, turns);
    sb
}

fn build_incremental_summary_prompt(prev_summary: &str, new_turns: &[Turn]) -> String {
    let mut sb = String::new();
    sb.push_str("Update this session handoff with the new turns below. Same format and rules: minimal, only what's needed to continue. Move completed items to \"Done\", update \"State\", and revise \"Next\".\n\n");
    sb.push_str("Current handoff:\n\n");
    sb.push_str(prev_summary);
    sb.push_str("\n\n---\n\nNew turns:\n\n");
    format_turns_for_summary(&mut sb, new_turns);
    sb
}

fn format_turns_for_summary(sb: &mut String, turns: &[Turn]) {
    for turn in turns {
        for msg in &turn.messages {
            match msg.role {
                Role::User => {
                    sb.push_str("User: ");
                    sb.push_str(&msg.content);
                    sb.push('\n');
                }
                Role::Assistant => {
                    if !msg.content.is_empty() {
                        sb.push_str("Assistant: ");
                        sb.push_str(&msg.content);
                        sb.push('\n');
                    }
                    if !msg.reasoning_content.is_empty() {
                        sb.push_str("Assistant reasoning: ");
                        sb.push_str(&msg.reasoning_content);
                        sb.push('\n');
                    }
                    for tc in &msg.tool_calls {
                        sb.push_str("Tool call: ");
                        sb.push_str(&tc.function.name);
                        sb.push('(');
                        sb.push_str(&tc.function.arguments);
                        sb.push_str(")\n");
                    }
                }
                Role::Tool => {
                    sb.push_str("Tool result (");
                    sb.push_str(&msg.name);
                    sb.push_str("): ");
                    sb.push_str(&truncate_for_summary(&msg.content, 2000));
                    sb.push('\n');
                }
                Role::System => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Client, ModelProfile, StreamEvent};
    use crate::tools::Registry;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn se(event_type: &str, delta: &str) -> StreamEvent {
        StreamEvent {
            event_type: event_type.to_string(),
            delta: delta.to_string(),
            reasoning_delta: String::new(),
            reasoning_details: vec![],
            tool_call: None,
            usage: None,
            finish_reason: String::new(),
            error: None,
        }
    }
    fn se_tool(tc: StreamToolCall) -> StreamEvent {
        StreamEvent {
            event_type: "tool_call".to_string(),
            delta: String::new(),
            reasoning_delta: String::new(),
            reasoning_details: vec![],
            tool_call: Some(tc),
            usage: None,
            finish_reason: String::new(),
            error: None,
        }
    }
    fn se_finish(reason: &str) -> StreamEvent {
        StreamEvent {
            event_type: "finish_reason".to_string(),
            delta: String::new(),
            reasoning_delta: String::new(),
            reasoning_details: vec![],
            tool_call: None,
            usage: None,
            finish_reason: reason.to_string(),
            error: None,
        }
    }

    struct MockClient {
        queue: Mutex<std::collections::VecDeque<Vec<StreamEvent>>>,
    }
    impl MockClient {
        fn new(responses: Vec<Vec<StreamEvent>>) -> Arc<Self> {
            Arc::new(Self {
                queue: Mutex::new(responses.into()),
            })
        }
    }
    #[async_trait::async_trait]
    impl ChatClient for MockClient {
        async fn chat(
            &self,
            _req: ChatRequest,
        ) -> Result<mpsc::Receiver<StreamEvent>, ProviderError> {
            let events = self.queue.lock().unwrap().pop_front().unwrap_or_default();
            let (tx, rx) = mpsc::channel(100);
            tokio::spawn(async move {
                for ev in events {
                    let _ = tx.send(ev).await;
                }
            });
            Ok(rx)
        }
        fn model(&self) -> &str {
            "mock"
        }
        fn context_window(&self) -> i32 {
            8000
        }
        fn pricing(&self) -> (f64, f64, f64) {
            (0.0, 0.0, 0.0)
        }
    }

    fn dummy_session() -> Session {
        Session {
            id: "s1".to_string(),
            name: String::new(),
            hash: "h".to_string(),
            named: false,
            current_turn: String::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            turn_count: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            context_tokens: 0,
            cost: 0.0,
            compacted_summary: String::new(),
            compacted_up_to: String::new(),
        }
    }

    fn dummy_agent() -> AgentSession {
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(TokioMutex::new(
            crate::session::store::Store::new(&dir.path().to_string_lossy()).unwrap(),
        ));
        let client: Arc<dyn ChatClient> = Arc::new(Client::new(
            "http://localhost",
            "m",
            "k",
            ModelProfile {
                context_window: 8000,
                max_output_tokens: 0,
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
        ));
        let registry = Arc::new(Registry::new());
        AgentSession::new(
            store,
            dummy_session(),
            client,
            registry,
            "sys".to_string(),
            5,
            "/tmp".to_string(),
        )
    }

    #[test]
    fn test_reload_from_syncs_session_and_accumulators() {
        let mut agent = dummy_agent();

        let mut fresh = agent.sess().clone();
        fresh.current_turn = "t1".to_string();
        fresh.prompt_tokens = 120;
        fresh.completion_tokens = 80;
        fresh.context_tokens = 200;
        fresh.cost = 1.5;
        fresh.compacted_summary = "sum".to_string();
        fresh.compacted_up_to = "t0".to_string();

        agent.reload_from(fresh);

        let sess = agent.sess();
        assert_eq!(sess.current_turn, "t1");
        assert_eq!(sess.prompt_tokens, 120);
        assert_eq!(sess.completion_tokens, 80);
        assert_eq!(sess.context_tokens, 200);
        assert_eq!(sess.cost, 1.5);
        assert_eq!(sess.compacted_summary, "sum");
        assert_eq!(sess.compacted_up_to, "t0");
    }

    #[test]
    fn test_truncate_for_summary_short() {
        assert_eq!(truncate_for_summary("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_for_summary_exact() {
        assert_eq!(truncate_for_summary("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_for_summary_truncation() {
        let result = truncate_for_summary("hello world this is long", 10);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 10);
    }

    #[test]
    fn test_truncate_for_summary_multibyte_no_panic() {
        let result = truncate_for_summary("héllo wörld", 8);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 8);
    }

    #[test]
    fn test_truncate_for_summary_small_maxlen() {
        let result = truncate_for_summary("abcdef", 4);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 4);
    }

    #[test]
    fn test_format_duration_500ms() {
        assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
    }

    #[test]
    fn test_format_duration_1s() {
        assert_eq!(format_duration(Duration::from_secs(1)), "1s");
    }

    #[test]
    fn test_format_duration_1500ms() {
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.5s");
    }

    #[test]
    fn test_format_duration_125s340ms() {
        let d = Duration::from_secs(125) + Duration::from_millis(340);
        assert_eq!(format_duration(d), "2m5.34s");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_delegate_end_to_end() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(TokioMutex::new(
            crate::session::store::Store::new(&dir.path().to_string_lossy()).unwrap(),
        ));

        let delegate_args = serde_json::json!({
            "subagent": "coder", "task": "say hello", "context": ""
        })
        .to_string();
        let parent_responses = vec![
            vec![
                se_tool(StreamToolCall {
                    id: "call1".into(),
                    name: "delegate".into(),
                    arguments: delegate_args,
                }),
                se_finish("tool_calls"),
            ],
            vec![se("text_delta", "parent-done"), se_finish("stop")],
        ];
        let parent_client = MockClient::new(parent_responses);

        let sub_responses = vec![vec![
            se("text_delta", "subagent result here"),
            se_finish("stop"),
        ]];
        let sub_client = MockClient::new(sub_responses);

        let sub_registry = Registry::new();

        let sub_def = SubagentDef {
            id: "coder".to_string(),
            description: "coder".to_string(),
            client: sub_client,
            registry: Arc::new(sub_registry),
            system_prompt: "sub".to_string(),
            model_name: "mock".to_string(),
        };

        let registry = Arc::new(Registry::new());
        let mut agent = AgentSession::new(
            store,
            dummy_session(),
            parent_client,
            registry,
            "sys".to_string(),
            5,
            "/tmp".to_string(),
        );
        agent.set_subagents(HashMap::from([("coder".to_string(), sub_def)]));

        let mut rx = agent.prompt("please delegate");
        let mut delegate_result = String::new();
        let mut final_text = String::new();
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "tool_result" && ev.tool_call_name == "delegate" {
                delegate_result = ev.tool_result.clone();
            }
            if ev.event_type == "text_delta" {
                final_text.push_str(&ev.delta);
            }
            if ev.event_type == "agent_done" {
                break;
            }
        }

        assert_eq!(final_text, "parent-done");
        assert!(
            delegate_result.contains("subagent result here"),
            "delegate result was: {:?}",
            delegate_result
        );
    }

    fn dummy_tool(name: &str, result: &str) -> crate::tools::Tool {
        let result = result.to_string();
        crate::tools::Tool {
            name: name.to_string(),
            description: String::new(),
            parameters: HashMap::new(),
            execute: Arc::new(move |_| {
                let result = result.clone();
                Box::pin(async move { Ok(result) })
            }),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_delegate_subagent_with_tool_round() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(TokioMutex::new(
            crate::session::store::Store::new(&dir.path().to_string_lossy()).unwrap(),
        ));

        let delegate_args = serde_json::json!({
            "subagent": "coder", "task": "read and summarize", "context": ""
        })
        .to_string();
        let parent_responses = vec![
            vec![
                se_tool(StreamToolCall {
                    id: "call1".into(),
                    name: "delegate".into(),
                    arguments: delegate_args,
                }),
                se_finish("tool_calls"),
            ],
            vec![se("text_delta", "parent-done"), se_finish("stop")],
        ];
        let parent_client = MockClient::new(parent_responses);

        let sub_responses = vec![
            vec![
                se_tool(StreamToolCall {
                    id: "sc1".into(),
                    name: "read_file".into(),
                    arguments: "{}".into(),
                }),
                se_finish("tool_calls"),
            ],
            vec![
                se("text_delta", "summarized file contents"),
                se_finish("stop"),
            ],
        ];
        let sub_client = MockClient::new(sub_responses);

        let mut sub_registry = Registry::new();
        let _ = sub_registry.register(dummy_tool("read_file", "FILE BODY"));

        let sub_def = SubagentDef {
            id: "coder".to_string(),
            description: "coder".to_string(),
            client: sub_client,
            registry: Arc::new(sub_registry),
            system_prompt: "sub".to_string(),
            model_name: "mock".to_string(),
        };

        let registry = Arc::new(Registry::new());
        let mut agent = AgentSession::new(
            store,
            dummy_session(),
            parent_client,
            registry,
            "sys".to_string(),
            5,
            "/tmp".to_string(),
        );
        agent.set_subagents(HashMap::from([("coder".to_string(), sub_def)]));

        let mut rx = agent.prompt("please delegate");
        let mut delegate_result = String::new();
        let mut final_text = String::new();
        while let Some(ev) = rx.recv().await {
            if ev.event_type == "tool_result" && ev.tool_call_name == "delegate" {
                delegate_result = ev.tool_result.clone();
            }
            if ev.event_type == "text_delta" {
                final_text.push_str(&ev.delta);
            }
            if ev.event_type == "agent_done" {
                break;
            }
        }

        assert_eq!(final_text, "parent-done");
        assert!(
            delegate_result.contains("summarized file contents"),
            "delegate result was: {:?}",
            delegate_result
        );
    }
}
