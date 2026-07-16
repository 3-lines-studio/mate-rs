use crate::agent::Event;

use super::chat_format::format_tool_label;

#[derive(Clone)]
pub struct LiveBlock {
    pub kind: String,
    pub raw: String,
    pub rendered: String,
    pub tool_name: String,
    pub tool_args: String,
    pub tool_id: String,
    pub tool_result: String,
    pub tool_error: String,
    pub tool_duration: String,
    pub tool_subagent: String,
}

impl LiveBlock {
    pub fn new(kind: &str) -> Self {
        LiveBlock {
            kind: kind.to_string(),
            raw: String::new(),
            rendered: String::new(),
            tool_name: String::new(),
            tool_args: String::new(),
            tool_id: String::new(),
            tool_result: String::new(),
            tool_error: String::new(),
            tool_duration: String::new(),
            tool_subagent: String::new(),
        }
    }
}

#[derive(Clone)]
pub struct Segment {
    pub kind: String,
    pub content: String,
    pub tool_name: String,
    pub tool_label: String,
    pub tool_args: String,
    pub tool_result: String,
    pub tool_error: String,
    pub tool_duration: String,
    pub tool_subagent: String,
}

impl Segment {
    pub fn prose(content: &str) -> Self {
        Segment {
            kind: "prose".into(),
            content: content.into(),
            tool_name: String::new(),
            tool_label: String::new(),
            tool_args: String::new(),
            tool_result: String::new(),
            tool_error: String::new(),
            tool_duration: String::new(),
            tool_subagent: String::new(),
        }
    }

    pub fn thinking(content: &str) -> Self {
        Segment {
            kind: "thinking".into(),
            content: content.into(),
            tool_name: String::new(),
            tool_label: String::new(),
            tool_args: String::new(),
            tool_result: String::new(),
            tool_error: String::new(),
            tool_duration: String::new(),
            tool_subagent: String::new(),
        }
    }

    pub fn tool(
        name: &str,
        args: &str,
        result: &str,
        error: &str,
        duration: &str,
        cwd: &str,
        subagent: &str,
    ) -> Self {
        Segment {
            kind: "tool".into(),
            content: String::new(),
            tool_name: name.into(),
            tool_label: format_tool_label(cwd, name, args),
            tool_args: args.into(),
            tool_result: result.into(),
            tool_error: error.into(),
            tool_duration: duration.into(),
            tool_subagent: subagent.into(),
        }
    }
}

#[derive(Clone)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
    pub segments: Vec<Segment>,
    pub rendered: String,
}

impl ChatMsg {
    pub fn user(content: &str) -> Self {
        ChatMsg {
            role: "user".into(),
            content: content.into(),
            segments: vec![],
            rendered: String::new(),
        }
    }

    pub fn assistant(segments: Vec<Segment>) -> Self {
        ChatMsg {
            role: "assistant".into(),
            content: String::new(),
            segments,
            rendered: String::new(),
        }
    }

    pub fn error(content: &str) -> Self {
        ChatMsg {
            role: "error".into(),
            content: content.into(),
            segments: vec![],
            rendered: String::new(),
        }
    }
}

pub fn handle_agent_event(
    event: &Event,
    live_blocks: &mut Vec<LiveBlock>,
    messages: &mut Vec<ChatMsg>,
    _cwd: &str,
    _show_thinking: bool,
    _tools_expanded: bool,
    total_tokens: &mut i32,
    cache_hit_tokens: &mut i32,
    total_cost: &mut f64,
    retry_available: &mut bool,
    finished: &mut bool,
    active_session: Option<&crate::agent::AgentSession>,
) {
    match event.event_type.as_str() {
        "text_delta" => {
            let lb = ensure_block(live_blocks, "prose");
            lb.raw.push_str(&event.delta);
            lb.rendered.clear();
        }
        "reasoning_delta" => {
            let lb = ensure_block(live_blocks, "thinking");
            lb.raw.push_str(&event.delta);
            lb.rendered.clear();
        }
        "tool_call_start" => {
            live_blocks.push(LiveBlock {
                kind: "tool".into(),
                raw: String::new(),
                rendered: String::new(),
                tool_name: event.tool_call_name.clone(),
                tool_args: event.tool_call_args.clone(),
                tool_id: event.tool_call_id.clone(),
                tool_result: String::new(),
                tool_error: String::new(),
                tool_duration: String::new(),
                tool_subagent: event.subagent.clone(),
            });
        }
        "tool_result" => {
            for lb in live_blocks.iter_mut().rev() {
                if lb.kind == "tool" && lb.tool_id == event.tool_call_id {
                    lb.tool_result = event.tool_result.clone();
                    lb.tool_duration = event.tool_duration.clone();
                    lb.rendered.clear();
                    break;
                }
            }
        }
        "tool_error" => {
            for lb in live_blocks.iter_mut().rev() {
                if lb.kind == "tool" && lb.tool_id == event.tool_call_id {
                    lb.tool_error = event.tool_error.clone();
                    lb.tool_duration = event.tool_duration.clone();
                    lb.rendered.clear();
                    break;
                }
            }
        }
        "agent_done" => {
            if event.subagent.is_empty() {
                *finished = true;
            }
        }
        "retry" => {
            messages.push(ChatMsg::error(&event.error));
        }
        "retry_available" => {
            messages.push(ChatMsg::error(&format!("{} — Press Ctrl+R to retry", event.error)));
            *retry_available = true;
            *finished = true;
        }
        "error" => {
            messages.push(ChatMsg::error(&event.error));
            *finished = true;
        }
        "usage" => {
            if event.subagent.is_empty() {
                if let Some(ref usage) = event.usage {
                    *total_tokens = usage.total_tokens;
                    *cache_hit_tokens = usage.prompt_cache_hit_tokens;
                }
                if let Some(asession) = active_session {
                    *total_tokens = asession.context_tokens();
                    *total_cost = asession.sess().cost;
                }
            }
        }
        _ => {}
    }
}

fn ensure_block<'a>(blocks: &'a mut Vec<LiveBlock>, kind: &str) -> &'a mut LiveBlock {
    if let Some(last) = blocks.last() {
        if last.kind == kind {
            return blocks.last_mut().unwrap();
        }
    }
    blocks.push(LiveBlock::new(kind));
    blocks.last_mut().unwrap()
}

pub fn finish_bot_message(
    live_blocks: &mut Vec<LiveBlock>,
    messages: &mut Vec<ChatMsg>,
    cwd: &str,
) {
    if live_blocks.is_empty() {
        return;
    }
    let mut segments = Vec::new();
    for lb in live_blocks.drain(..) {
        match lb.kind.as_str() {
            "prose" => segments.push(Segment::prose(&lb.raw)),
            "thinking" => segments.push(Segment::thinking(&lb.raw)),
            "tool" => segments.push(Segment::tool(
                &lb.tool_name,
                &lb.tool_args,
                &lb.tool_result,
                &lb.tool_error,
                &lb.tool_duration,
                cwd,
                &lb.tool_subagent,
            )),
            _ => {}
        }
    }
    messages.push(ChatMsg::assistant(segments));
}

pub fn assemble_message_prose(msg: &ChatMsg) -> String {
    let mut out = String::new();
    for seg in &msg.segments {
        if seg.kind == "prose" {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&seg.content);
        }
    }
    if !msg.content.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&msg.content);
    }
    out
}

pub fn assemble_message_full_text(msg: &ChatMsg) -> String {
    let mut out = String::new();
    for (i, seg) in msg.segments.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        match seg.kind.as_str() {
            "thinking" | "prose" => out.push_str(&seg.content),
            "tool" => {
                out.push_str(&format!("Tool: {}", seg.tool_name));
                if !seg.tool_duration.is_empty() {
                    out.push_str(&format!(" · {}", seg.tool_duration));
                }
                if !seg.tool_args.is_empty() {
                    out.push_str(&format!("\nArgs: {}", seg.tool_args));
                }
                if !seg.tool_result.is_empty() {
                    out.push_str(&format!("\nResult: {}", seg.tool_result));
                }
                if !seg.tool_error.is_empty() {
                    out.push_str(&format!("\nError: {}", seg.tool_error));
                }
            }
            _ => {}
        }
    }
    if !msg.content.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&msg.content);
    }
    out
}
