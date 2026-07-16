use std::time::Instant;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use crate::agent::{AgentSession, Event};
use crate::message::Message;
use crate::prompts::Template;
use crate::render::StreamRenderer;

use super::chat_dropdowns::{
    render_command_dropdown, render_file_dropdown, render_template_dropdown, Dropdown, COMMANDS,
};
use super::chat_handlers::{
    finish_bot_message, handle_agent_event, ChatMsg, LiveBlock, Segment,
};
use super::chat_render::{rainbow_text, render_status_line, render_tool_block};
use super::file_picker::index_files;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Modal {
    None,
    Template,
    File,
    Command,
    Tree,
}

pub struct ChatScreen {
    pub messages: Vec<ChatMsg>,
    pub live_blocks: Vec<LiveBlock>,
    pub waiting: bool,
    pub compacting: bool,
    pub wait_start: Instant,
    pub wait_ticks: usize,
    pub events: Option<tokio::sync::mpsc::Receiver<Event>>,
    pub session_name: String,
    pub model_name: String,
    pub cwd: String,
    pub total_tokens: i32,
    pub cache_hit_tokens: i32,
    pub total_cost: f64,
    pub context_window: i32,
    pub show_thinking: bool,
    pub tools_expanded: bool,
    pub retry_available: bool,
    pub active_session: Option<AgentSession>,

    pub textarea: String,
    pub cursor: usize,
    pub history: Vec<String>,
    pub history_idx: isize,
    pub textarea_height: u16,

    pub viewport_offset: usize,
    pub viewport_lines: Vec<String>,
    pub user_scrolled_up: bool,
    pub scroll_to_bottom: bool,

    pub active_modal: Modal,
    pub command_dropdown: Dropdown<(String, String)>,
    pub template_dropdown: Dropdown<(Template, String)>,
    pub file_dropdown: Dropdown<(String, String)>,
    pub tree_dropdown: Dropdown<(String, String, usize, bool, Vec<bool>, bool)>,

    pub all_files: Vec<String>,
    pub files_loaded: bool,
    pub all_template_items: Vec<(Template, String)>,
    pub templates: Vec<Template>,
    pub tree_items: Vec<(String, String, usize, bool, Vec<bool>, bool)>,

    pub renderer: StreamRenderer,
    pub width: u16,
    pub height: u16,

    pub ctrl_c_pending: bool,
    pub finished: bool,
}

impl ChatScreen {
    pub fn new(cwd: String, templates: Vec<Template>, show_thinking: bool, tools_expanded: bool) -> Self {
        let tmpl_items: Vec<(Template, String)> = templates
            .iter()
            .map(|t| (t.clone(), t.name.clone()))
            .collect();

        ChatScreen {
            messages: Vec::new(),
            live_blocks: Vec::new(),
            waiting: false,
            compacting: false,
            wait_start: Instant::now(),
            wait_ticks: 0,
            events: None,
            session_name: String::new(),
            model_name: String::new(),
            cwd,
            total_tokens: 0,
            cache_hit_tokens: 0,
            total_cost: 0.0,
            context_window: 0,
            show_thinking,
            tools_expanded,
            retry_available: false,
            active_session: None,

            textarea: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_idx: -1,
            textarea_height: 3,

            viewport_offset: 0,
            viewport_lines: Vec::new(),
            user_scrolled_up: false,
            scroll_to_bottom: false,

            active_modal: Modal::None,
            command_dropdown: Dropdown::new(),
            template_dropdown: Dropdown::new(),
            file_dropdown: Dropdown::new(),
            tree_dropdown: Dropdown::new(),

            all_files: Vec::new(),
            files_loaded: false,
            all_template_items: tmpl_items,
            templates,
            tree_items: Vec::new(),

            renderer: StreamRenderer::new(80),
            width: 80,
            height: 24,

            ctrl_c_pending: false,
            finished: false,
        }
    }

    pub fn set_size(&mut self, w: u16, h: u16) {
        self.width = w;
        self.height = h;
        self.renderer = StreamRenderer::new((w as usize).saturating_sub(2));
        for msg in &mut self.messages {
            msg.rendered.clear();
        }
        for lb in &mut self.live_blocks {
            lb.rendered.clear();
        }
    }

    pub fn reset(&mut self) {
        self.messages.clear();
        self.live_blocks.clear();
        self.textarea.clear();
        self.cursor = 0;
        self.history.clear();
        self.history_idx = -1;
        self.waiting = false;
        self.compacting = false;
        self.active_modal = Modal::None;
        self.retry_available = false;
        self.ctrl_c_pending = false;
        self.finished = false;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_to_bottom = true;
        self.user_scrolled_up = false;
    }

    pub fn add_message(&mut self, role: &str, content: &str) {
        self.messages.push(match role {
            "user" => ChatMsg::user(content),
            "error" => ChatMsg::error(content),
            _ => ChatMsg::assistant(vec![Segment::prose(content)]),
        });
        self.render_messages();
    }

    pub fn load_messages(&mut self, msgs: &[Message], children: &std::collections::HashMap<String, Vec<Message>>) {
        let mut pending_tools: Vec<(String, String, String)> = Vec::new();

        for msg in msgs {
            match msg.role {
                crate::message::Role::User => {
                    self.messages.push(ChatMsg::user(&msg.content));
                    self.history.push(msg.content.clone());
                }
                crate::message::Role::Assistant => {
                    let last_was_assistant = self.messages.last().map(|m| m.role == "assistant").unwrap_or(false);

                    if last_was_assistant {
                        let last = self.messages.last_mut().unwrap();
                        if !msg.reasoning_content.is_empty() {
                            last.segments.push(Segment::thinking(&msg.reasoning_content));
                        }
                        if !msg.content.is_empty() {
                            last.segments.push(Segment::prose(&msg.content));
                        }
                        for tc in &msg.tool_calls {
                            pending_tools.push((tc.id.clone(), tc.function.name.clone(), tc.function.arguments.clone()));
                        }
                    } else {
                        let mut segments = Vec::new();
                        if !msg.reasoning_content.is_empty() {
                            segments.push(Segment::thinking(&msg.reasoning_content));
                        }
                        if !msg.content.is_empty() {
                            segments.push(Segment::prose(&msg.content));
                        }
                        for tc in &msg.tool_calls {
                            pending_tools.push((tc.id.clone(), tc.function.name.clone(), tc.function.arguments.clone()));
                        }
                        self.messages.push(ChatMsg::assistant(segments));
                    }
                }
                crate::message::Role::Tool => {
                    if let Some((id, name, args)) = pending_tools.first() {
                        let id = id.clone();
                        let name = name.clone();
                        let args = args.clone();
                        pending_tools.remove(0);

                        let last = self.messages.last_mut().unwrap();
                        if last.role == "assistant" {
                            last.segments.push(Segment::tool(
                                &name,
                                &args,
                                &msg.content,
                                "",
                                &msg.tool_duration,
                                &self.cwd,
                                "",
                            ));

                            if name == "delegate" && !id.is_empty() {
                                if let Some(child_msgs) = children.get(&id) {
                                    let subagent = parse_subagent_from_args(&args);
                                    for child_msg in child_msgs {
                                        match child_msg.role {
                                            crate::message::Role::Tool => {
                                                last.segments.push(Segment::tool(
                                                    &child_msg.name,
                                                    "",
                                                    &child_msg.content,
                                                    "",
                                                    &child_msg.tool_duration,
                                                    &self.cwd,
                                                    &subagent,
                                                ));
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        self.render_messages();
    }

    pub fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        if area.width != self.width || area.height != self.height {
            self.set_size(area.width, area.height);
        }
        self.render_messages_area(f, area);
        self.render_bottom_bar(f, area);
    }

    fn render_messages_area(&mut self, f: &mut Frame, area: Rect) {
        let bottom_height = self.bottom_bar_height();
        let msg_area = Rect::new(
            area.x,
            area.y,
            area.width,
            area.height.saturating_sub(bottom_height),
        );

        let mut all_lines: Vec<Line> = Vec::new();
        all_lines.push(Line::from(""));
        all_lines.push(Line::from(""));

        for msg in &self.messages {
            let rendered = if msg.rendered.is_empty() {
                self.render_message_ansi(msg)
            } else {
                msg.rendered.clone()
            };

            let text = crate::render::block::ansi_to_text(&rendered);

            for line in text.lines {
                all_lines.push(line);
            }
            all_lines.push(Line::from(""));
        }

        if !self.live_blocks.is_empty() {
            let live = self.render_live_turn();
            let t = crate::render::block::ansi_to_text(&live);
            for line in t.lines {
                all_lines.push(line);
            }
        }

        all_lines.push(Line::from(""));

        let total_lines = all_lines.len();
        let visible = msg_area.height as usize;
        let max_offset = total_lines.saturating_sub(visible);

        if self.scroll_to_bottom {
            self.viewport_offset = max_offset;
            self.scroll_to_bottom = false;
            self.user_scrolled_up = false;
        }
        if self.viewport_offset > max_offset {
            self.viewport_offset = max_offset;
            self.user_scrolled_up = false;
        }

        let text = Text::from(all_lines);
        let paragraph = Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .scroll((self.viewport_offset as u16, 0));
        f.render_widget(paragraph, msg_area);
    }

    fn render_message_ansi(&self, msg: &ChatMsg) -> String {
        match msg.role.as_str() {
            "user" => {
                let accent = "\x1b[38;2;255;199;153m";
                let reset = "\x1b[0m";
                let w = (self.width as usize).saturating_sub(4).max(20);
                let content = crate::render::block::wordwrap(msg.content.trim(), w, "");
                let top = format!("{accent}╭{}╮{reset}", "─".repeat(w + 2));
                let mut lines = vec![top];
                for line in content.lines() {
                    let padded = format!("{accent}│{reset} {}{} {accent}│{reset}", line, " ".repeat(w.saturating_sub(unicode_width::UnicodeWidthStr::width(line))));
                    lines.push(padded);
                }
                let bottom = format!("{accent}╰{}╯{reset}", "─".repeat(w + 2));
                lines.push(bottom);
                lines.join("\n")
            }
            "assistant" => self.render_assistant_turn(msg),
            "error" => msg.content.clone(),
            _ => String::new(),
        }
    }

    fn render_assistant_turn(&self, msg: &ChatMsg) -> String {
        let mut parts = Vec::new();
        let prefix = "◆ ";
        let muted = "\x1b[38;2;106;106;106m";
        let reset = "\x1b[0m";
        let mut first = true;

        for seg in &msg.segments {
            let content = match seg.kind.as_str() {
                "thinking" => {
                    if self.show_thinking {
                        let rendered = self.renderer.render(&seg.content);
                        let mut out = String::new();
                        if first {
                            out.push_str(&format!("{}{}{} {}", muted, "│", reset, prefix));
                            out.push_str(&rendered.lines().collect::<Vec<_>>().join(&format!("\n{muted}│{reset} ")));
                        } else {
                            out.push_str(&format!("{muted}│{reset} "));
                            out.push_str(&rendered.lines().collect::<Vec<_>>().join(&format!("\n{muted}│{reset} ")));
                        }
                        out
                    } else {
                        continue;
                    }
                }
                "prose" => {
                    let rendered = self.renderer.render(&seg.content);
                    if first {
                        format!("{} {}", prefix, rendered)
                    } else {
                        rendered
                    }
                }
                "tool" => {
                    render_tool_block(
                        &seg.tool_name,
                        &seg.tool_args,
                        &seg.tool_result,
                        &seg.tool_error,
                        &seg.tool_duration,
                        &self.cwd,
                        &seg.tool_subagent,
                        !self.tools_expanded,
                        self.width as usize,
                        0,
                    )
                }
                _ => continue,
            };
            if !content.is_empty() {
                parts.push(content);
            }
            first = false;
        }
        parts.join("\n\n")
    }

    fn render_live_turn(&self) -> String {
        let mut parts = Vec::new();
        let prefix = "◆ ";
        let muted = "\x1b[38;2;106;106;106m";
        let reset = "\x1b[0m";
        let mut first = true;

        for lb in &self.live_blocks {
            let content = match lb.kind.as_str() {
                "prose" => {
                    let rendered = self.renderer.render(&lb.raw);
                    if first {
                        format!("{} {}", prefix, rendered)
                    } else {
                        rendered
                    }
                }
                "thinking" => {
                    if !self.show_thinking {
                        continue;
                    }
                    let rendered = self.renderer.render(&lb.raw);
                    let mut out = String::new();
                    if first {
                        out.push_str(&format!("{}{}{} {}", muted, "│", reset, prefix));
                        out.push_str(&rendered.lines().collect::<Vec<_>>().join(&format!("\n{muted}│{reset} ")));
                    } else {
                        out.push_str(&format!("{muted}│{reset} "));
                        out.push_str(&rendered.lines().collect::<Vec<_>>().join(&format!("\n{muted}│{reset} ")));
                    }
                    out
                }
                "tool" => {
                    render_tool_block(
                        &lb.tool_name,
                        &lb.tool_args,
                        &lb.tool_result,
                        &lb.tool_error,
                        &lb.tool_duration,
                        &self.cwd,
                        &lb.tool_subagent,
                        !self.tools_expanded,
                        self.width as usize,
                        0,
                    )
                }
                _ => continue,
            };
            if !content.is_empty() {
                parts.push(content);
            }
            first = false;
        }
        parts.join("\n\n")
    }

    pub fn render_messages(&mut self) {
        let mut rendered_msgs: Vec<String> = Vec::with_capacity(self.messages.len());
        for msg in &self.messages {
            let r = if msg.rendered.is_empty() {
                self.render_message_ansi(msg)
            } else {
                msg.rendered.clone()
            };
            rendered_msgs.push(r);
        }
        for (msg, r) in self.messages.iter_mut().zip(rendered_msgs) {
            if msg.rendered.is_empty() {
                msg.rendered = r;
            }
        }
    }

    fn bottom_bar_height(&self) -> u16 {
        if self.waiting || self.compacting {
            let mut h = 2u16;
            if self.active_modal == Modal::Command {
                h += (COMMANDS.len() + 3) as u16;
            }
            if self.active_modal == Modal::Tree {
                h += 6;
            }
            return h;
        }
        let mut h = self.textarea_height + 3;
        if self.active_modal == Modal::Template {
            h += 8;
        }
        if self.active_modal == Modal::File {
            h += 11;
        }
        if self.active_modal == Modal::Command {
            h += (COMMANDS.len() + 3) as u16;
        }
        if self.active_modal == Modal::Tree {
            h += 6;
        }
        h
    }

    fn render_bottom_bar(&self, f: &mut Frame, area: Rect) {
        let bottom_h = self.bottom_bar_height();
        let bottom_y = area.height.saturating_sub(bottom_h);
        let bottom_area = Rect::new(area.x, bottom_y, area.width, bottom_h);

        let mut y_offset = 0u16;

        if self.active_modal == Modal::Template {
            let h = 8u16;
            let modal_area = Rect::new(bottom_area.x, bottom_area.y, bottom_area.width.min(60), h);
            render_template_dropdown(f, modal_area, &self.template_dropdown, "");
            y_offset += h;
        }
        if self.active_modal == Modal::File {
            let h = 11u16;
            let modal_area = Rect::new(bottom_area.x, bottom_area.y + y_offset, bottom_area.width.min(60), h);
            render_file_dropdown(f, modal_area, &self.file_dropdown);
            y_offset += h;
        }
        if self.active_modal == Modal::Command {
            let h = (COMMANDS.len() + 3) as u16;
            let modal_area = Rect::new(bottom_area.x, bottom_area.y + y_offset, bottom_area.width.min(40), h);
            render_command_dropdown(f, modal_area, &self.command_dropdown);
            y_offset += h;
        }

        if self.waiting || self.compacting {
            let label = if self.compacting { "Compacting…" } else { "Thinking…" };
            let thinking_text = format!("{} ({:.0?})", rainbow_text(label, self.wait_ticks).lines[0].to_string(), self.wait_start.elapsed());
            let thinking_area = Rect::new(bottom_area.x, bottom_area.y + y_offset, bottom_area.width, 1);
            f.render_widget(
                Paragraph::new(thinking_text).alignment(Alignment::Left),
                thinking_area,
            );
            y_offset += 1;
        } else {
            let input = if self.textarea.is_empty() { "Send a message…" } else { &self.textarea };
            let input_area = Rect::new(
                bottom_area.x + 1,
                bottom_area.y + y_offset,
                bottom_area.width.saturating_sub(2),
                self.textarea_height,
            );
            let input_widget = Paragraph::new(input.to_string())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::from_u32(0x002D2D2D))),
                )
                .wrap(Wrap { trim: true });
            f.render_widget(input_widget, input_area);
            y_offset += self.textarea_height + 1;
        }

        let status_area = Rect::new(
            bottom_area.x,
            bottom_area.y + y_offset,
            bottom_area.width,
            1,
        );
        render_status_line(
            f,
            status_area,
            &self.model_name,
            self.total_tokens,
            self.context_window,
            self.cache_hit_tokens,
            self.total_cost,
            &self.session_name,
            self.user_scrolled_up,
        );
    }

    pub fn open_command_dropdown(&mut self) {
        self.active_modal = Modal::Command;
        self.command_dropdown.items = COMMANDS
            .iter()
            .map(|(l, a)| (l.to_string(), a.to_string()))
            .collect();
        self.command_dropdown.selected = 0;
        self.command_dropdown.visible = true;
    }

    pub fn close_dropdowns(&mut self) {
        self.active_modal = Modal::None;
        self.command_dropdown.visible = false;
        self.template_dropdown.visible = false;
        self.file_dropdown.visible = false;
        self.tree_dropdown.visible = false;
    }

    pub fn handle_agent_event_inner(&mut self, event: &Event) {
        handle_agent_event(
            event,
            &mut self.live_blocks,
            &mut self.messages,
            &self.cwd,
            self.show_thinking,
            self.tools_expanded,
            &mut self.total_tokens,
            &mut self.cache_hit_tokens,
            &mut self.total_cost,
            &mut self.retry_available,
            &mut self.finished,
            self.active_session.as_ref(),
        );
        if self.finished {
            finish_bot_message(&mut self.live_blocks, &mut self.messages, &self.cwd);
            self.waiting = false;
            self.finished = false;
        }
        self.render_messages();
    }

    pub fn finish_bot_message_now(&mut self) {
        finish_bot_message(&mut self.live_blocks, &mut self.messages, &self.cwd);
        self.waiting = false;
        self.files_loaded = false;
        self.render_messages();
    }

    pub fn ensure_files_loaded(&mut self) {
        if !self.files_loaded {
            self.all_files = index_files(&self.cwd);
            self.files_loaded = true;
        }
    }

    pub fn open_template_dropdown(&mut self, query: &str) {
        self.active_modal = Modal::Template;
        self.filter_template_dropdown(query);
    }

    pub fn filter_template_dropdown(&mut self, query: &str) {
        if query.is_empty() {
            self.template_dropdown.items = self.all_template_items.clone();
        } else {
            let lower = query.to_lowercase();
            self.template_dropdown.items = self
                .all_template_items
                .iter()
                .filter(|(t, _)| t.name.to_lowercase().contains(&lower))
                .cloned()
                .collect();
        }
        if self.template_dropdown.selected >= self.template_dropdown.items.len() {
            self.template_dropdown.selected = self.template_dropdown.items.len().saturating_sub(1);
        }
        self.template_dropdown.visible = true;
    }

    pub fn open_file_dropdown(&mut self, query: &str) {
        self.active_modal = Modal::File;
        self.filter_file_dropdown(query);
    }

    pub fn filter_file_dropdown(&mut self, query: &str) {
        self.ensure_files_loaded();
        let all = self.all_files.clone();
        if query.is_empty() {
            self.file_dropdown.items = all.into_iter().map(|f| (f.clone(), f)).collect();
        } else {
            let lower = query.to_lowercase();
            self.file_dropdown.items = all
                .into_iter()
                .filter(|f| f.to_lowercase().contains(&lower))
                .map(|f| (f.clone(), f))
                .collect();
        }
        if self.file_dropdown.selected >= self.file_dropdown.items.len() {
            self.file_dropdown.selected = self.file_dropdown.items.len().saturating_sub(1);
        }
        self.file_dropdown.visible = true;
    }

    pub fn build_tree_from_index(
        &mut self,
        index: &[crate::session::types::TurnMeta],
        current_turn: &str,
    ) {
        let by_id: std::collections::HashMap<String, &crate::session::types::TurnMeta> =
            index.iter().map(|m| (m.id.clone(), m)).collect();
        let mut children: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for m in index {
            children
                .entry(m.parent_id.clone())
                .or_default()
                .push(m.id.clone());
        }

        let roots: Vec<String> = children
            .get("")
            .cloned()
            .unwrap_or_default();

        let mut items: Vec<(String, String, usize, bool, Vec<bool>, bool)> = Vec::new();

        fn walk(
            turn_id: &str,
            depth: usize,
            ancestors: &mut Vec<bool>,
            is_last: bool,
            by_id: &std::collections::HashMap<String, &crate::session::types::TurnMeta>,
            children: &std::collections::HashMap<String, Vec<String>>,
            current_turn: &str,
            items: &mut Vec<(String, String, usize, bool, Vec<bool>, bool)>,
        ) {
            let label = by_id
                .get(turn_id)
                .map(|m| m.label.clone())
                .unwrap_or_else(|| turn_id.to_string());
            let is_current = turn_id == current_turn;
            items.push((
                turn_id.to_string(),
                label,
                depth,
                is_last,
                ancestors.clone(),
                is_current,
            ));

            if let Some(kids) = children.get(turn_id) {
                for (i, child_id) in kids.iter().enumerate() {
                    let child_last = i == kids.len() - 1;
                    ancestors.push(!is_last);
                    walk(
                        child_id,
                        depth + 1,
                        ancestors,
                        child_last,
                        by_id,
                        children,
                        current_turn,
                        items,
                    );
                    ancestors.pop();
                }
            }
        }

        let mut ancestors = Vec::new();
        for (i, root_id) in roots.iter().enumerate() {
            let last = i == roots.len() - 1;
            walk(
                root_id,
                0,
                &mut ancestors,
                last,
                &by_id,
                &children,
                current_turn,
                &mut items,
            );
        }

        self.tree_items = items;
    }

    pub fn show_tree(&mut self) {
        if self.tree_items.is_empty() {
            return;
        }
        let items = self.tree_items.clone();
        let mut selected = 0usize;
        for (i, item) in items.iter().enumerate() {
            if item.5 {
                selected = i;
                break;
            }
        }
        self.tree_dropdown.items = items;
        self.tree_dropdown.selected = selected;
        self.tree_dropdown.visible = true;
        self.active_modal = Modal::Tree;
    }

    pub fn compact_session(&mut self) {
        if let Some(ref asession) = self.active_session {
            match asession.compact() {
                Ok(events) => {
                    self.events = Some(events);
                    self.compacting = true;
                    self.wait_start = Instant::now();
                    self.wait_ticks = 0;
                }
                Err(e) => {
                    self.add_message("error", &format!("Compact failed: {}", e));
                }
            }
        }
    }

    pub fn copy_last_response(&mut self) {
        for msg in self.messages.iter().rev() {
            if msg.role == "assistant" {
                let text = super::chat_handlers::assemble_message_prose(msg);
                if !text.is_empty() {
                    match arboard::Clipboard::new() {
                        Ok(mut clipboard) => {
                            if clipboard.set_text(&text).is_ok() {
                                self.add_message("assistant", "Copied last response to clipboard");
                            } else {
                                self.add_message("error", "Failed to copy to clipboard");
                            }
                        }
                        Err(e) => {
                            self.add_message("error", &format!("Clipboard error: {}", e));
                        }
                    }
                }
                return;
            }
        }
        self.add_message("error", "No assistant response to copy");
    }

    pub fn export_markdown(&mut self) {
        let mut sb = String::new();
        sb.push_str(&format!("# Mate Session: {}\n\n", self.session_name));
        sb.push_str(&format!("**Model:** {}  \n", self.model_name));
        sb.push_str(&format!(
            "**Date:** {}  \n\n---\n\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ));

        for msg in &self.messages {
            match msg.role.as_str() {
                "user" => {
                    sb.push_str(&format!("### You\n\n{}\n\n", msg.content));
                }
                "assistant" => {
                    let text = super::chat_handlers::assemble_message_full_text(msg);
                    sb.push_str(&format!("### Mate\n\n{}\n\n", text));
                }
                "error" => {
                    sb.push_str(&format!("### Error\n\n{}\n\n", msg.content));
                }
                _ => {}
            }
        }

        let filename = format!(
            "mate-export-{}.md",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );
        let path = std::path::PathBuf::from(&self.cwd).join(&filename);
        if let Err(e) = std::fs::write(&path, sb) {
            self.add_message("error", &format!("Export failed: {}", e));
        } else {
            self.add_message("assistant", &format!("Exported to {}", filename));
        }
    }

    pub fn textarea_value(&self) -> &str {
        &self.textarea
    }

    pub fn is_empty(&self) -> bool {
        self.textarea.is_empty()
    }

    pub fn clear_textarea(&mut self) {
        self.textarea.clear();
        self.cursor = 0;
        self.history_idx = -1;
    }

    pub fn insert_char(&mut self, c: char) {
        self.textarea.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, s: &str) {
        self.textarea.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn delete_before_cursor(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.textarea.remove(self.cursor);
        }
    }

    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn cursor_right(&mut self) {
        if self.cursor < self.textarea.len() {
            self.cursor += 1;
        }
    }

    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn cursor_end(&mut self) {
        self.cursor = self.textarea.len();
    }

    pub fn set_text(&mut self, text: &str) {
        self.textarea = text.to_string();
        self.cursor = self.textarea.len();
    }
}

fn parse_subagent_from_args(args: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
        v["subagent"].as_str().unwrap_or("").to_string()
    } else {
        String::new()
    }
}
