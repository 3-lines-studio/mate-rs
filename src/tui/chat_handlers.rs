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
    pub children: Vec<LiveBlock>,
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
            children: Vec::new(),
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
    pub children: Vec<Segment>,
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
            children: Vec::new(),
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
            children: Vec::new(),
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
            children: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
    pub segments: Vec<Segment>,
    pub rendered: String,
    pub stopped: bool,
}

impl ChatMsg {
    pub fn user(content: &str) -> Self {
        ChatMsg {
            role: "user".into(),
            content: content.into(),
            segments: vec![],
            rendered: String::new(),
            stopped: false,
        }
    }

    pub fn assistant(segments: Vec<Segment>) -> Self {
        ChatMsg {
            role: "assistant".into(),
            content: String::new(),
            segments,
            rendered: String::new(),
            stopped: false,
        }
    }

    pub fn error(content: &str) -> Self {
        ChatMsg {
            role: "error".into(),
            content: content.into(),
            segments: vec![],
            rendered: String::new(),
            stopped: false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
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
    use crate::agent::EventKind;
    match &event.kind {
        EventKind::TextDelta(delta) => {
            let lb = ensure_block(live_blocks, "prose");
            lb.raw.push_str(delta);
            lb.rendered.clear();
        }
        EventKind::ReasoningDelta(delta) => {
            let lb = ensure_block(live_blocks, "thinking");
            lb.raw.push_str(delta);
            lb.rendered.clear();
        }
        EventKind::ToolCallStart { id, name, args } => {
            let block = LiveBlock {
                kind: "tool".into(),
                raw: String::new(),
                rendered: String::new(),
                tool_name: name.clone(),
                tool_args: args.clone(),
                tool_id: id.clone(),
                tool_result: String::new(),
                tool_error: String::new(),
                tool_duration: String::new(),
                tool_subagent: String::new(),
                children: Vec::new(),
            };
            if !event.subagent_id.is_empty() {
                if let Some(parent) = find_block_mut(live_blocks, &event.subagent_id) {
                    parent.children.push(block);
                } else {
                    live_blocks.push(block);
                }
            } else {
                live_blocks.push(block);
            }
        }
        EventKind::ToolResult {
            id,
            result,
            duration,
            ..
        } => {
            if let Some(lb) = find_block_mut(live_blocks, id) {
                lb.tool_result = result.clone();
                lb.tool_duration = duration.clone();
                lb.rendered.clear();
            }
        }
        EventKind::ToolError {
            id,
            error,
            duration,
            ..
        } => {
            if let Some(lb) = find_block_mut(live_blocks, id) {
                lb.tool_error = error.clone();
                lb.tool_duration = duration.clone();
                lb.rendered.clear();
            }
        }
        EventKind::AgentDone(_) => {
            if event.subagent.is_empty() {
                *finished = true;
            }
        }
        EventKind::Retry(msg) => {
            if event.subagent.is_empty() {
                messages.push(ChatMsg::error(msg));
            }
        }
        EventKind::RetryAvailable(msg) => {
            if event.subagent.is_empty() {
                messages.push(ChatMsg::error(&format!("{} — Press Ctrl+R to retry", msg)));
                *retry_available = true;
                *finished = true;
            }
        }
        EventKind::Error(msg) => {
            if event.subagent.is_empty() {
                messages.push(ChatMsg::error(msg));
                *finished = true;
            }
        }
        EventKind::Usage(usage) if event.subagent.is_empty() => {
            *total_tokens = usage.total_tokens;
            *cache_hit_tokens = usage.prompt_cache_hit_tokens;
            if let Some(asession) = active_session {
                *total_cost = asession.sess().cost;
            }
        }
        _ => {}
    }
}

fn find_block_mut<'a>(blocks: &'a mut [LiveBlock], tool_id: &str) -> Option<&'a mut LiveBlock> {
    for lb in blocks.iter_mut().rev() {
        if lb.tool_id == tool_id {
            return Some(lb);
        }
        if let Some(c) = find_block_mut(&mut lb.children, tool_id) {
            return Some(c);
        }
    }
    None
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
