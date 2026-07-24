use super::format::format_duration;
use super::types::{Event, RoundResult};
use crate::message::{Message, Role};
use crate::provider::{ChatRequest, ProviderError, StreamEvent};
use chrono::Utc;
use rand::RngExt;
use std::time::Duration;

impl super::AgentSession {
    pub(super) async fn run_loop(
        &mut self,
        user_text: &str,
        events: &tokio::sync::mpsc::Sender<Event>,
    ) {
        let _herdr = crate::herdr::WorkingGuard::enter(!self.subagents_state.is_subagent);
        let loop_start = std::time::Instant::now();
        self.working_messages.clear();
        self.subagents_state.subagent_turns.clear();
        let mut parent_id = self.sess.current_turn.clone();

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
        let mut ancestry_msgs = {
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
                    let _ = events.send(Event::error_msg(&e.to_string())).await;
                    return;
                }
            }
        };

        for round in 1..=self.max_rounds {
            let req = self.build_request(&ancestry_msgs);
            let t0 = std::time::Instant::now();

            match self.stream_round(req, events).await {
                Ok(result) => {
                    let stream_dur = t0.elapsed();
                    if !result.tool_calls.is_empty() {
                        if !result.content.is_empty() {
                            self.working_messages.push(assistant_msg(
                                result.content.clone(),
                                result.reasoning.clone(),
                                vec![],
                            ));
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
                        ancestry_msgs.extend_from_slice(&self.working_messages);
                        self.commit_turn(&parent_id).await;
                        parent_id = self.sess.current_turn.clone();
                        continue;
                    }

                    if !result.content.is_empty()
                        || !result.reasoning.is_empty()
                        || !result.reasoning_details.is_empty()
                    {
                        self.working_messages.push(assistant_msg(
                            result.content,
                            result.reasoning,
                            result.reasoning_details,
                        ));
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
                    if working_has_progress(&self.working_messages) {
                        self.commit_turn(&parent_id).await;
                    }
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
            messages: msgs,
            tools: self.cached_tool_defs.clone(),
            session_id: self.api_session_id.clone(),
            ..Default::default()
        }
    }

    async fn stream_round(
        &mut self,
        req: ChatRequest,
        events: &tokio::sync::mpsc::Sender<Event>,
    ) -> Result<RoundResult, ProviderError> {
        const MAX_ATTEMPTS: usize = 5;
        const MAX_DELAY: Duration = Duration::from_secs(30);

        'attempt: for attempt in 0..MAX_ATTEMPTS {
            let mut rx = match self.client.chat(req.clone()).await {
                Ok(r) => r,
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
                        continue 'attempt;
                    } else {
                        return Err(e);
                    }
                }
            };

            let mut result = RoundResult {
                content: String::new(),
                reasoning: String::new(),
                reasoning_details: vec![],
                tool_calls: vec![],
                finish_reason: String::new(),
            };
            let mut has_content = false;
            let mut has_tool_calls = false;
            let mut has_reasoning = false;

            while let Some(ev) = rx.recv().await {
                if events.is_closed() {
                    break;
                }
                match ev {
                    StreamEvent::Error { error } => {
                        if !has_content
                            && !has_tool_calls
                            && !has_reasoning
                            && attempt < MAX_ATTEMPTS - 1
                        {
                            let shift = 1u64 << attempt;
                            let delay = Duration::from_secs(2 * shift).min(MAX_DELAY);
                            let jitter = delay.as_secs_f64()
                                * 0.25
                                * (2.0 * rand::rng().random::<f64>() - 1.0);
                            let delay = Duration::from_secs_f64(delay.as_secs_f64() + jitter);
                            log::warn!(
                                "stream stalled on empty attempt {}/{}: {} — retrying",
                                attempt + 1,
                                MAX_ATTEMPTS,
                                error
                            );
                            let _ = events
                                .send(Event::retry_ev(&format!(
                                    "Attempt {}/{} failed: {} — retrying in {}",
                                    attempt + 1,
                                    MAX_ATTEMPTS,
                                    error,
                                    format_duration(delay)
                                )))
                                .await;
                            tokio::time::sleep(delay).await;
                            continue 'attempt;
                        }
                        return Err(error);
                    }
                    StreamEvent::TextDelta { delta } => {
                        has_content = true;
                        result.content.push_str(&delta);
                        let _ = events.send(Event::text_delta(&delta)).await;
                    }
                    StreamEvent::ReasoningDelta { delta } => {
                        has_reasoning = true;
                        result.reasoning.push_str(&delta);
                        let _ = events.send(Event::reasoning_delta(&delta)).await;
                    }
                    StreamEvent::ToolCall { call } => {
                        has_tool_calls = true;
                        result.tool_calls.push(call);
                    }
                    StreamEvent::Usage { mut usage } => {
                        self.sess.prompt_tokens += usage.prompt_tokens;
                        self.sess.completion_tokens += usage.completion_tokens;
                        self.sess.context_tokens = usage.total_tokens;
                        if self.sess.context_tokens == 0 {
                            self.sess.context_tokens =
                                usage.prompt_tokens + usage.completion_tokens;
                        }
                        let cost = self.client.cost_for(&usage);
                        self.sess.cost += cost;
                        usage.cost = cost;
                        let _ = events.send(Event::usage_ev(usage)).await;
                    }
                    StreamEvent::ReasoningDetails { details } => {
                        result.reasoning_details = details;
                    }
                    StreamEvent::FinishReason { reason } => {
                        result.finish_reason = reason;
                    }
                }
            }
            return Ok(result);
        }

        Err(ProviderError {
            status_code: 0,
            body: "chat returned no stream".into(),
        })
    }

    async fn commit_turn(&mut self, parent_id: &str) {
        if self.working_messages.is_empty() {
            return;
        }

        if self.subagents_state.is_subagent {
            self.captured_msgs.append(&mut self.working_messages);
            return;
        }

        let turn_id = crate::session::types::compute_turn_id(parent_id, &self.working_messages);
        let turn = crate::session::Turn {
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

            for st in &self.subagents_state.subagent_turns {
                let sub_id = crate::session::types::compute_turn_id(&turn_id, &st.msgs);
                let sub_turn = crate::session::Turn {
                    id: sub_id,
                    parent_id: turn_id.clone(),
                    messages: st.msgs.clone(),
                    created_at: Utc::now(),
                    subagent: st.subagent.clone(),
                    tool_call_id: st.tool_call_id.clone(),
                };
                let _ = store.commit_turn(&self.sess.id, &sub_turn);
            }
            self.subagents_state.subagent_turns.clear();
        }

        self.sess.current_turn = turn_id;
        self.sess.turn_count += 1;
        self.sess.total_tokens = self.sess.prompt_tokens + self.sess.completion_tokens;

        if !self.sess.named {
            let label = crate::session::types::turn_label(&self.working_messages);
            if label != "(empty)" {
                let store = self.store.lock().await;
                store.set_name(&mut self.sess, &label);
            }
        }

        self.sess.compacted_summary = self.compaction.compacted_summary.clone();
        self.sess.compacted_up_to = self.compaction.compacted_up_to.clone();

        {
            let mut store = self.store.lock().await;
            let _ = store.save_meta(&self.sess);
        }

        self.working_messages.clear();
    }
}

fn working_has_progress(messages: &[Message]) -> bool {
    messages.iter().any(|m| m.role != Role::User)
}

fn assistant_msg(
    content: String,
    reasoning_content: String,
    reasoning_details: Vec<crate::message::ReasoningDetail>,
) -> Message {
    Message {
        role: Role::Assistant,
        content,
        reasoning_content,
        reasoning_details,
        tool_calls: vec![],
        tool_call_id: String::new(),
        name: String::new(),
        tool_duration: String::new(),
    }
}
