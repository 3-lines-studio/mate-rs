mod compaction;
mod format;
mod loop_;
#[cfg(test)]
mod tests;
mod tools;
mod types;

pub use types::{Event, EventKind, SubagentDef};

use crate::message::{Message, ToolDef};
use crate::provider::ChatClient;
use crate::session::store::Store;
use crate::session::Session;
use crate::tools::Registry;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex as TokioMutex;

const TOOL_TIMEOUT_SECS: u64 = 120;
const COMPACTION_THRESHOLD_NUM: i32 = 7;
const COMPACTION_THRESHOLD_DEN: i32 = 10;
const COMPACTION_BUDGET_DIVISOR: i32 = 3;

fn tool_rules_prompt() -> String {
    format!(
        "CRITICAL TOOL RULES:\n- Use tools directly — never describe what you'd do, execute it.\n- Do not fabricate results.\n- Non-delegate tool calls timeout after {} seconds.\n- Prefer `symbols` (kind: find/refs/list) over grep for symbol lookups in Rust, Go, TS/TSX/JSX, CSS.",
        TOOL_TIMEOUT_SECS
    )
}

#[derive(Clone)]
pub struct AgentSession {
    store: Arc<TokioMutex<Store>>,
    sess: Session,
    tools: Arc<Registry>,
    client: Arc<dyn ChatClient>,
    system_msg: String,
    max_rounds: i32,
    cwd: String,

    counters: types::UsageCounters,
    cached_tool_defs: Vec<ToolDef>,

    working_messages: Vec<Message>,

    compaction: types::CompactionState,

    subagents_state: types::SubagentState,

    last_prompt: String,
    captured_msgs: Vec<Message>,
}

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
    sb.push_str(&tool_rules_prompt());
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
        let prompt_tokens = sess.prompt_tokens;
        let completion_tokens = sess.completion_tokens;
        let cost = sess.cost;
        let context_tokens = sess.context_tokens;
        let compacted_summary = sess.compacted_summary.clone();
        let compacted_up_to = sess.compacted_up_to.clone();

        AgentSession {
            store,
            sess,
            tools: registry,
            client,
            system_msg,
            max_rounds,
            cwd,
            counters: types::UsageCounters {
                total_prompt_tokens: prompt_tokens,
                total_completion_tokens: completion_tokens,
                total_cost: cost,
                last_request_tokens: 0,
                last_total_tokens: context_tokens,
            },
            cached_tool_defs,
            working_messages: Vec::new(),
            compaction: types::CompactionState {
                compaction_client: None,
                compacted_summary,
                compacted_up_to,
            },
            subagents_state: types::SubagentState {
                subagents: HashMap::new(),
                subagent_turns: Vec::new(),
                is_subagent: false,
            },
            last_prompt: String::new(),
            captured_msgs: Vec::new(),
        }
    }

    fn new_subagent(
        store: Arc<TokioMutex<Store>>,
        sess_id: String,
        def: &SubagentDef,
        max_rounds: i32,
        cwd: String,
    ) -> Self {
        let system_msg = format!("CWD: {}\n\n{}", cwd, def.system_prompt);
        AgentSession {
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
            counters: types::UsageCounters {
                total_prompt_tokens: 0,
                total_completion_tokens: 0,
                total_cost: 0.0,
                last_request_tokens: 0,
                last_total_tokens: 0,
            },
            cached_tool_defs: def.registry.tool_defs(),
            working_messages: Vec::new(),
            compaction: types::CompactionState {
                compaction_client: None,
                compacted_summary: String::new(),
                compacted_up_to: String::new(),
            },
            subagents_state: types::SubagentState {
                subagents: HashMap::new(),
                subagent_turns: Vec::new(),
                is_subagent: true,
            },
            last_prompt: String::new(),
            captured_msgs: Vec::new(),
        }
    }

    pub fn sess(&self) -> Session {
        self.sess.clone()
    }
    pub fn reload_from(&mut self, sess: Session) {
        self.counters.total_prompt_tokens = sess.prompt_tokens;
        self.counters.total_completion_tokens = sess.completion_tokens;
        self.counters.total_cost = sess.cost;
        self.counters.last_total_tokens = sess.context_tokens;
        self.compaction.compacted_summary = sess.compacted_summary.clone();
        self.compaction.compacted_up_to = sess.compacted_up_to.clone();
        self.sess = sess;
    }
    pub fn system_prompt(&self) -> &str {
        &self.system_msg
    }
    pub fn context_window(&self) -> i32 {
        self.client.context_window()
    }
    pub fn context_tokens(&self) -> i32 {
        self.counters.last_total_tokens
    }
    pub fn tool_defs(&self) -> Vec<ToolDef> {
        self.cached_tool_defs.clone()
    }

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
        self.subagents_state.subagents = defs;
        self.cached_tool_defs
            .push(tools::build_delegate_def(&names, &descriptions));
    }

    pub fn set_compaction_client(&mut self, client: Arc<dyn ChatClient>) {
        self.compaction.compaction_client = Some(client);
    }

    pub fn set_client(&mut self, client: Arc<dyn ChatClient>) {
        self.client = client;
    }

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
        if self.compaction.compaction_client.is_none() {
            return Err("no compaction client configured".into());
        }
        let (tx, rx) = mpsc::channel(100);
        let mut s = self.clone();
        tokio::spawn(async move {
            let _ = tx
                .send(Event {
                    kind: EventKind::CompactingStart,
                    subagent: String::new(),
                    subagent_id: String::new(),
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
}
