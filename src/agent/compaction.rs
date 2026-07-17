use crate::message::{Message, Role};
use crate::provider::{ChatClient, ChatRequest, StreamEvent};
use crate::session::Turn;
use std::sync::Arc;

impl super::AgentSession {
    pub(super) async fn compact_ancestry(&mut self, turns: &[Turn], force: bool) -> Vec<Message> {
        let compaction_client = match &self.compaction.compaction_client {
            Some(cc) => cc,
            None => return flatten_turns(turns),
        };

        let ctx_window = self.client.context_window();
        let above_threshold = force
            || self.counters.last_total_tokens
                > ctx_window * super::COMPACTION_THRESHOLD_NUM / super::COMPACTION_THRESHOLD_DEN;
        let keep_turns = compute_keep_turns(turns, ctx_window);

        if force && turns.len() <= 1 {
            return flatten_turns(turns);
        }

        let force_keep = if force { 0 } else { keep_turns };

        if self.compaction.compacted_summary.is_empty() {
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
                    self.compaction.compacted_summary = summary.clone();
                    self.compaction.compacted_up_to = old[old.len() - 1].id.clone();
                    build_compacted_messages(&summary, &flatten_turns(recent))
                }
                Err(e) => {
                    log::warn!("compaction failed, using full ancestry error={}", e);
                    flatten_turns(turns)
                }
            };
        }

        let new_turns = turns_after_id(turns, &self.compaction.compacted_up_to);
        if new_turns.is_empty() {
            if turns.len() <= force_keep {
                return build_compacted_messages(
                    &self.compaction.compacted_summary,
                    &flatten_turns(turns),
                );
            }
            let split = turns.len() - force_keep;
            let old = &turns[..split];
            let recent = &turns[split..];
            return match call_compaction(compaction_client, &build_summarization_prompt(old)).await
            {
                Ok(summary) => {
                    self.compaction.compacted_summary = summary.clone();
                    self.compaction.compacted_up_to = old[old.len() - 1].id.clone();
                    build_compacted_messages(&summary, &flatten_turns(recent))
                }
                Err(e) => {
                    log::warn!("compaction failed, reusing stale summary error={}", e);
                    build_compacted_messages(
                        &self.compaction.compacted_summary,
                        &flatten_turns(turns),
                    )
                }
            };
        }

        if !above_threshold {
            return build_compacted_messages(
                &self.compaction.compacted_summary,
                &flatten_turns(new_turns),
            );
        }

        let keep_new = compute_keep_turns(new_turns, ctx_window);
        if new_turns.len() <= keep_new {
            return build_compacted_messages(
                &self.compaction.compacted_summary,
                &flatten_turns(new_turns),
            );
        }

        let split = new_turns.len() - keep_new;
        let summary_turns = &new_turns[..split];
        let recent_turns = &new_turns[split..];

        match call_compaction(
            compaction_client,
            &build_incremental_summary_prompt(&self.compaction.compacted_summary, summary_turns),
        )
        .await
        {
            Ok(summary) => {
                self.compaction.compacted_summary = summary.clone();
                self.compaction.compacted_up_to = summary_turns[summary_turns.len() - 1].id.clone();
                build_compacted_messages(&summary, &flatten_turns(recent_turns))
            }
            Err(e) => {
                log::warn!("compaction failed, reusing stale summary error={}", e);
                build_compacted_messages(
                    &self.compaction.compacted_summary,
                    &flatten_turns(new_turns),
                )
            }
        }
    }
}

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
    let budget = context_window / super::COMPACTION_BUDGET_DIVISOR;
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
        if let StreamEvent::Error { error } = ev {
            return Err(error.to_string());
        }
        if let StreamEvent::TextDelta { delta } = ev {
            result.push_str(&delta);
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
}
