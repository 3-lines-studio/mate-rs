use crate::agent::{AgentSession, Event};
use crate::message::Message;
use crate::prompts::Template;
use crate::render::StreamRenderer;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame,
};
use std::time::Instant;

use super::chat_dropdowns::{
    render_command_dropdown, render_file_dropdown, render_template_dropdown, Dropdown, COMMANDS,
};
use super::chat_handlers::{finish_bot_message, handle_agent_event, ChatMsg, LiveBlock, Segment};
use super::chat_render::{render_tool_block, thinking_indicator};
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
    pub textarea_scroll: usize,

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
    pub fn new(
        cwd: String,
        templates: Vec<Template>,
        show_thinking: bool,
        tools_expanded: bool,
    ) -> Self {
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
            textarea_height: 1,
            textarea_scroll: 0,

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
        self.renderer = StreamRenderer::new((w as usize).saturating_sub(5));
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

    pub fn load_messages(
        &mut self,
        msgs: &[Message],
        children: &std::collections::HashMap<String, Vec<Message>>,
    ) {
        let mut pending_tools: Vec<(String, String, String)> = Vec::new();

        for msg in msgs {
            match msg.role {
                crate::message::Role::User => {
                    self.messages.push(ChatMsg::user(&msg.content));
                    self.history.push(msg.content.clone());
                }
                crate::message::Role::Assistant => {
                    let last_was_assistant = self
                        .messages
                        .last()
                        .map(|m| m.role == "assistant")
                        .unwrap_or(false);

                    if last_was_assistant {
                        let last = self.messages.last_mut().unwrap();
                        if !msg.reasoning_content.is_empty() {
                            last.segments
                                .push(Segment::thinking(&msg.reasoning_content));
                        }
                        if !msg.content.is_empty() {
                            last.segments.push(Segment::prose(&msg.content));
                        }
                        for tc in &msg.tool_calls {
                            pending_tools.push((
                                tc.id.clone(),
                                tc.function.name.clone(),
                                tc.function.arguments.clone(),
                            ));
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
                            pending_tools.push((
                                tc.id.clone(),
                                tc.function.name.clone(),
                                tc.function.arguments.clone(),
                            ));
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
                                        if child_msg.role == crate::message::Role::Tool {
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
        self.update_textarea_layout();
        let bottom_h = self.bottom_bar_height().min(area.height);

        let msg_area = Rect::new(
            area.x + 2,
            area.y,
            area.width.saturating_sub(3),
            area.height.saturating_sub(bottom_h),
        );
        self.render_messages_area(f, msg_area);
        self.render_bottom_bar(f, area);
    }

    fn render_messages_area(&mut self, f: &mut Frame, msg_area: Rect) {
        let mut all_lines: Vec<Line> = Vec::new();
        all_lines.push(Line::from(""));

        for msg in &self.messages {
            let rendered = if msg.rendered.is_empty() {
                self.render_message_ansi(msg)
            } else {
                msg.rendered.clone()
            };

            let text = crate::render::block::ansi_to_text(&rendered);

            if msg.role == "user" {
                all_lines.push(Line::from(""));
            }
            for line in text.lines {
                all_lines.push(line);
            }
            if msg.role == "user" {
                all_lines.push(Line::from(""));
            }
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
        } else if !self.user_scrolled_up {
            self.viewport_offset = max_offset;
        }
        if self.viewport_offset > max_offset {
            self.viewport_offset = max_offset;
            self.user_scrolled_up = false;
        }

        let text = Text::from(all_lines);
        let paragraph = Paragraph::new(text).scroll((self.viewport_offset as u16, 0));
        f.render_widget(paragraph, msg_area);
    }

    fn render_message_ansi(&self, msg: &ChatMsg) -> String {
        match msg.role.as_str() {
            "user" => {
                let user_accent = "\x1b[38;2;200;200;200m";
                let text_fg = "\x1b[38;2;225;225;225m";
                let reset = "\x1b[0m";
                let w = (self.width as usize).saturating_sub(5).max(20);
                let content = crate::render::block::wordwrap(msg.content.trim(), w, "");
                let mut lines = Vec::new();
                for (i, line) in content.lines().enumerate() {
                    if i == 0 {
                        lines.push(format!("{user_accent}❯{reset} {text_fg}{line}{reset}"));
                    } else {
                        lines.push(format!("  {text_fg}{line}{reset}"));
                    }
                }
                lines.join("\n")
            }
            "assistant" => self.render_assistant_turn(msg),
            "error" => msg.content.clone(),
            _ => String::new(),
        }
    }

    fn render_assistant_turn(&self, msg: &ChatMsg) -> String {
        let mut parts = Vec::new();
        let diamond = "\x1b[38;2;187;154;247m◆\x1b[0m ";
        let bar = "\x1b[38;2;187;154;247m";
        let reset = "\x1b[0m";

        for seg in &msg.segments {
            let content = match seg.kind.as_str() {
                "thinking" => {
                    if self.show_thinking {
                        let rendered = self.renderer.render(&seg.content);
                        let mut out = String::new();
                        out.push_str(&format!("{bar}┃{reset} {diamond}Thinking…\n"));
                        out.push_str(&format!("{bar}┃{reset} "));
                        out.push_str(
                            &rendered
                                .lines()
                                .collect::<Vec<_>>()
                                .join(&format!("\n{bar}┃{reset} ")),
                        );
                        out
                    } else {
                        continue;
                    }
                }
                "prose" => self.renderer.render(&seg.content),
                "tool" => render_tool_block(
                    &seg.tool_name,
                    &seg.tool_args,
                    &seg.tool_result,
                    &seg.tool_error,
                    &seg.tool_duration,
                    &self.cwd,
                    &seg.tool_subagent,
                    !self.tools_expanded,
                    (self.width as usize).saturating_sub(3),
                    0,
                ),
                _ => continue,
            };
            if !content.is_empty() {
                parts.push(content);
            }
        }
        parts.join("\n\n")
    }

    fn render_live_turn(&self) -> String {
        let mut parts = Vec::new();
        let diamond = "\x1b[38;2;187;154;247m◆\x1b[0m ";
        let bar = "\x1b[38;2;187;154;247m";
        let reset = "\x1b[0m";

        for lb in &self.live_blocks {
            let content = match lb.kind.as_str() {
                "prose" => self.renderer.render(&lb.raw),
                "thinking" => {
                    if !self.show_thinking {
                        continue;
                    }
                    let rendered = self.renderer.render(&lb.raw);
                    let mut out = String::new();
                    out.push_str(&format!("{bar}┃{reset} {diamond}Thinking…\n"));
                    out.push_str(&format!("{bar}┃{reset} "));
                    out.push_str(
                        &rendered
                            .lines()
                            .collect::<Vec<_>>()
                            .join(&format!("\n{bar}┃{reset} ")),
                    );
                    out
                }
                "tool" => render_tool_block(
                    &lb.tool_name,
                    &lb.tool_args,
                    &lb.tool_result,
                    &lb.tool_error,
                    &lb.tool_duration,
                    &self.cwd,
                    &lb.tool_subagent,
                    !self.tools_expanded,
                    (self.width as usize).saturating_sub(3),
                    0,
                ),
                _ => continue,
            };
            if !content.is_empty() {
                parts.push(content);
            }
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
            let mut h = 1u16;
            if self.active_modal == Modal::Command {
                h += (COMMANDS.len() + 3) as u16;
            }
            if self.active_modal == Modal::Tree {
                h += 6;
            }
            return h;
        }
        let mut h = self.textarea_height + 2;
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
        let bottom_h = self.bottom_bar_height().min(area.height);
        let bottom_y = area.height.saturating_sub(bottom_h);
        let bottom_area = Rect::new(area.x, bottom_y, area.width, bottom_h);

        let mut y_offset = 0u16;

        if self.active_modal == Modal::Template {
            let h = 8u16;
            let top = bottom_area.y + y_offset;
            if let Some(h2) = fit_height(area, top, h) {
                let modal_area = Rect::new(bottom_area.x, top, bottom_area.width.min(60), h2);
                render_template_dropdown(f, modal_area, &self.template_dropdown, "");
            }
            y_offset += h;
        }
        if self.active_modal == Modal::File {
            let h = 11u16;
            let top = bottom_area.y + y_offset;
            if let Some(h2) = fit_height(area, top, h) {
                let modal_area = Rect::new(bottom_area.x, top, bottom_area.width.min(60), h2);
                render_file_dropdown(f, modal_area, &self.file_dropdown);
            }
            y_offset += h;
        }
        if self.active_modal == Modal::Command {
            let h = (COMMANDS.len() + 3) as u16;
            let top = bottom_area.y + y_offset;
            if let Some(h2) = fit_height(area, top, h) {
                let modal_area = Rect::new(bottom_area.x, top, bottom_area.width.min(40), h2);
                render_command_dropdown(f, modal_area, &self.command_dropdown);
            }
            y_offset += h;
        }

        if self.waiting || self.compacting {
            let label = if self.compacting {
                "Compacting…"
            } else {
                "Thinking…"
            };
            let indicator = thinking_indicator(self.wait_ticks, label, self.wait_start.elapsed());
            let top = bottom_area.y + y_offset;
            if fit_height(area, top, 1).is_some() {
                let thinking_area = Rect::new(bottom_area.x, top, bottom_area.width, 1);
                f.render_widget(
                    Paragraph::new(indicator).alignment(Alignment::Left),
                    thinking_area,
                );
            }
        } else {
            let prompt_h = self.textarea_height + 2;
            let top = bottom_area.y + y_offset;
            let input = if self.textarea.is_empty() {
                "Send a message…"
            } else {
                &self.textarea
            };
            let input_color = if self.textarea.is_empty() {
                Color::from_u32(0x006C6C6C)
            } else {
                Color::from_u32(0x00E1E1E1)
            };

            let branch_or_cwd = crate::tui::chat_render::git_branch(&self.cwd)
                .unwrap_or_else(|| shorten_cwd_string(&self.cwd));
            let border_color = Color::from_u32(0x00505058);
            let dim = Style::default().fg(Color::from_u32(0x006C6C6C));

            let mut parts = vec![format!(
                "{} / {}",
                fmt_tokens(self.total_tokens),
                fmt_tokens(self.context_window),
            )];
            if self.cache_hit_tokens > 0 {
                parts.push(format!("cache {}", fmt_tokens(self.cache_hit_tokens)));
            }
            parts.push(fmt_cost(self.total_cost));
            let stats = Line::styled(format!(" {} ", parts.join(" · ")), dim).left_aligned();
            let info = Line::styled(format!(" {} · {} ", self.model_name, branch_or_cwd), dim)
                .right_aligned();

            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title_bottom(stats)
                .title_bottom(info);

            if let Some(h2) = fit_height(area, top, prompt_h) {
                let prompt_area = Rect::new(
                    bottom_area.x + 1,
                    top,
                    bottom_area.width.saturating_sub(2),
                    h2,
                );
                let content_area = block.inner(prompt_area);
                f.render_widget(block, prompt_area);
                if content_area.height > 0 && content_area.width > 0 {
                    f.render_widget(
                        Paragraph::new(input.to_string())
                            .style(Style::default().fg(input_color))
                            .wrap(Wrap { trim: true })
                            .scroll((self.textarea_scroll as u16, 0)),
                        content_area,
                    );
                    let (crow, ccol) =
                        textarea_cursor_xy(&self.textarea, self.cursor, content_area.width);
                    let display_row = crow.saturating_sub(self.textarea_scroll as u16);
                    let cx = content_area.x + ccol.min(content_area.width);
                    let cy =
                        content_area.y + display_row.min(content_area.height.saturating_sub(1));
                    f.set_cursor_position((cx, cy));
                }
            }
        }
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

        let roots: Vec<String> = children.get("").cloned().unwrap_or_default();

        let mut items: Vec<(String, String, usize, bool, Vec<bool>, bool)> = Vec::new();

        #[allow(clippy::too_many_arguments, clippy::type_complexity)]
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
        self.textarea_scroll = 0;
        self.history_idx = -1;
    }

    pub fn update_textarea_layout(&mut self) {
        let cw = self.width.saturating_sub(4);
        let total = textarea_total_rows(&self.textarea, cw);
        let max_h = (self.height / 3).max(3);
        self.textarea_height = total.min(max_h).max(1);
        let (crow, _) = textarea_cursor_xy(&self.textarea, self.cursor, cw);
        let crow = crow as usize;
        let visible = self.textarea_height as usize;
        if crow < self.textarea_scroll {
            self.textarea_scroll = crow;
        } else if crow >= self.textarea_scroll + visible {
            self.textarea_scroll = crow + 1 - visible;
        }
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

fn shorten_cwd_string(cwd: &str) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy();
        if cwd.starts_with(&*home) {
            return format!("~/{}", &cwd[home.len()..]);
        }
    }
    cwd.to_string()
}

fn fmt_tokens(n: i32) -> String {
    let n = n as f64;
    if n >= 1_000_000.0 {
        format!("{:.1}M", n / 1_000_000.0)
    } else if n >= 1000.0 {
        let v = n / 1000.0;
        if v.fract() == 0.0 {
            format!("{:.0}k", v)
        } else {
            format!("{:.1}k", v)
        }
    } else {
        format!("{}", n as i32)
    }
}

fn fmt_cost(c: f64) -> String {
    if c < 0.01 {
        format!("${:.4}", c)
    } else {
        format!("${:.2}", c)
    }
}

fn wrap_line(line: &str, max_w: usize) -> Vec<(usize, usize)> {
    if max_w == 0 {
        return vec![(0, 0)];
    }
    let mut rows: Vec<(usize, usize)> = Vec::new();
    let mut line_start = 0usize;
    let mut line_w = 0usize;
    let mut content_end = 0usize;
    let mut i = 0usize;
    let mut iter = line.char_indices().peekable();
    loop {
        let gap_start = i;
        while let Some(&(_, c)) = iter.peek() {
            if c == ' ' || c == '\t' {
                let (b, c) = iter.next().unwrap();
                i = b + c.len_utf8();
            } else {
                break;
            }
        }
        let gap_w = unicode_width::UnicodeWidthStr::width(&line[gap_start..i]);

        let word_start = i;
        while let Some(&(_, c)) = iter.peek() {
            if c != ' ' && c != '\t' {
                let (b, c) = iter.next().unwrap();
                i = b + c.len_utf8();
            } else {
                break;
            }
        }
        let word_end = i;
        if word_start == word_end {
            break;
        }
        let word_w = unicode_width::UnicodeWidthStr::width(&line[word_start..word_end]);

        if line_w + gap_w + word_w <= max_w {
            line_w += gap_w + word_w;
            content_end = word_end;
        } else if word_w <= max_w {
            if content_end > line_start {
                rows.push((line_start, content_end));
            }
            line_start = word_start;
            line_w = word_w;
            content_end = word_end;
        } else {
            if content_end > line_start {
                rows.push((line_start, content_end));
            }
            let mut seg_start = word_start;
            let mut w = 0usize;
            for (b, c) in line[word_start..word_end].char_indices() {
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                let abs_b = word_start + b;
                if w + cw > max_w && w > 0 {
                    rows.push((seg_start, abs_b));
                    seg_start = abs_b;
                    w = cw;
                } else {
                    w += cw;
                }
            }
            rows.push((seg_start, word_end));
            line_start = word_end;
            line_w = 0;
            content_end = word_end;
        }
    }
    if i > content_end {
        content_end = i;
    }
    if content_end > line_start {
        rows.push((line_start, content_end));
    }
    if rows.is_empty() {
        rows.push((0, 0));
    }
    rows
}

fn fit_height(area: Rect, top: u16, h: u16) -> Option<u16> {
    let bottom = area.y.saturating_add(area.height);
    if top >= bottom {
        return None;
    }
    Some(h.min(bottom - top))
}

fn textarea_total_rows(text: &str, max_w: u16) -> u16 {
    let max_w = (max_w as usize).max(1);
    let mut total = 0u16;
    for line in text.split('\n') {
        total = total.saturating_add(wrap_line(line, max_w).len() as u16);
    }
    total.max(1)
}

fn textarea_cursor_xy(text: &str, cursor: usize, max_w: u16) -> (u16, u16) {
    let max_w = (max_w as usize).max(1);
    let mut row: u16 = 0;
    let mut byte = 0usize;
    for line in text.split('\n') {
        let llen = line.len();
        if cursor <= byte + llen {
            let rel = cursor - byte;
            let rows = wrap_line(line, max_w);
            let mut idx = 0usize;
            for (k, (s, _)) in rows.iter().enumerate() {
                if *s <= rel {
                    idx = k;
                } else {
                    break;
                }
            }
            let (s, e) = rows[idx];
            let col = unicode_width::UnicodeWidthStr::width(&line[s..rel.min(e)]);
            return (row + idx as u16, col as u16);
        }
        row += wrap_line(line, max_w).len() as u16;
        byte += llen + 1;
    }
    (row, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn render_to(screen: &mut ChatScreen, w: u16, h: u16) {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| screen.render(f)).unwrap();
    }

    #[test]
    fn bottom_bar_never_overflows_buffer() {
        use std::panic;
        let mut panics: Vec<String> = Vec::new();
        for &h in &[
            1u16, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 24, 30, 40, 50,
            60, 67, 80, 100,
        ] {
            for &(waiting, modal) in &[
                (false, Modal::None),
                (true, Modal::None),
                (false, Modal::Command),
                (true, Modal::Command),
                (false, Modal::Template),
                (true, Modal::Template),
                (false, Modal::File),
                (true, Modal::File),
                (false, Modal::Tree),
            ] {
                for &tall in &[false, true] {
                    let mut s = ChatScreen::new(".".into(), vec![], true, true);
                    s.set_size(116, h);
                    for _ in 0..20 {
                        s.add_message("user", &"word ".repeat(60));
                        s.add_message("assistant", &"reply ".repeat(80));
                    }
                    s.active_modal = modal;
                    if modal == Modal::Command {
                        s.open_command_dropdown();
                    }
                    s.waiting = waiting;
                    if tall {
                        s.set_text(&"long input line ".repeat(60));
                    }
                    s.scroll_to_bottom();
                    let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                        render_to(&mut s, 116, h);
                    }));
                    if res.is_err() {
                        panics.push(format!(
                            "h={} waiting={} modal={} tall={}",
                            h,
                            waiting,
                            match modal {
                                Modal::None => "None",
                                Modal::Command => "Command",
                                Modal::Template => "Template",
                                Modal::File => "File",
                                Modal::Tree => "Tree",
                            },
                            tall
                        ));
                    }
                }
            }
        }
        if !panics.is_empty() {
            for p in &panics {
                eprintln!("PANIC: {}", p);
            }
        }
        assert!(panics.is_empty(), "reproduced {} panics", panics.len());
    }

    fn wrapped(line: &str, w: usize) -> Vec<String> {
        wrap_line(line, w)
            .iter()
            .map(|(s, e)| line[*s..*e].to_string())
            .collect()
    }

    #[test]
    fn wrap_matches_ratatui_long_sentence() {
        let line =
            "abcd efghij klmnopabcd efgh ijklmnopabcdefg hijkl mnopab c d e f g h i j k l m n o";
        assert_eq!(
            wrapped(line, 20),
            vec![
                "abcd efghij",
                "klmnopabcd efgh",
                "ijklmnopabcdefg",
                "hijkl mnopab c d e f",
                "g h i j k l m n o",
            ]
        );
    }

    #[test]
    fn wrap_long_word_breaks_at_width() {
        let line: String = "a".repeat(75);
        let rows = wrapped(&line, 20);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].len(), 20);
        assert_eq!(rows[1].len(), 20);
        assert_eq!(rows[3].len(), 15);
    }

    #[test]
    fn cursor_xy_single_line() {
        assert_eq!(textarea_cursor_xy("hello", 0, 80), (0, 0));
        assert_eq!(textarea_cursor_xy("hello", 2, 80), (0, 2));
        assert_eq!(textarea_cursor_xy("hello", 5, 80), (0, 5));
    }

    #[test]
    fn cursor_xy_multiline() {
        assert_eq!(textarea_cursor_xy("ab\ncd", 5, 80), (1, 2));
        assert_eq!(textarea_cursor_xy("ab\ncd", 3, 80), (1, 0));
    }

    #[test]
    fn cursor_xy_wraps_at_word_boundary() {
        // width 11: "hello world" fits; cursor at end is col 11
        assert_eq!(textarea_cursor_xy("hello world", 11, 11), (0, 11));
        // width 5: "world" wraps to row 1; cursor at end is row 1 col 5
        assert_eq!(textarea_cursor_xy("hello world", 11, 5), (1, 5));
    }

    #[test]
    fn cursor_xy_trailing_space() {
        // trailing space at end of line: cursor must advance past it
        assert_eq!(textarea_cursor_xy("hello ", 6, 80), (0, 6));
        // cursor in the middle of a run of trailing spaces
        assert_eq!(textarea_cursor_xy("hello   ", 7, 80), (0, 7));
        // a line of only spaces
        assert_eq!(textarea_cursor_xy("   ", 3, 80), (0, 3));
    }

    #[test]
    fn total_rows_empty() {
        assert_eq!(textarea_total_rows("", 80), 1);
    }

    #[test]
    fn total_rows_single_line() {
        assert_eq!(textarea_total_rows("hello", 80), 1);
    }

    #[test]
    fn total_rows_newlines() {
        assert_eq!(textarea_total_rows("a\nb\nc", 80), 3);
    }

    #[test]
    fn total_rows_word_wrap() {
        // 4 words of 5 chars + 3 spaces = 23 chars; width 11 wraps to 2 rows
        assert_eq!(textarea_total_rows("hello world hello world", 11), 2);
        // 5 words: “a b c d e” = 9 chars; width 3 -> 3 rows
        assert_eq!(textarea_total_rows("a b c d e", 3), 3);
    }

    #[test]
    fn total_rows_long_word() {
        let line: String = "a".repeat(75);
        assert_eq!(textarea_total_rows(&line, 20), 4);
    }

    #[test]
    fn layout_grows_and_scrolls() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_size(80, 24);
        s.update_textarea_layout();
        assert_eq!(s.textarea_height, 1);
        assert_eq!(s.textarea_scroll, 0);

        // type enough to produce 5 wrapped rows (content width = 76)
        let long: String = "word ".repeat(80);
        s.set_text(&long);
        s.update_textarea_layout();
        assert!(s.textarea_height >= 1);
        let max_h = 24 / 3;
        assert!(s.textarea_height <= max_h);
        // cursor at end should be visible
        let (crow, _) = textarea_cursor_xy(&s.textarea, s.cursor, 76);
        assert!((crow as usize) >= s.textarea_scroll);
        assert!((crow as usize) < s.textarea_scroll + s.textarea_height as usize);
    }

    #[test]
    fn follows_new_messages_to_bottom_when_not_scrolled() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_size(80, 24);
        for _ in 0..40 {
            s.add_message("user", &"word ".repeat(200));
        }
        render_to(&mut s, 80, 24);
        assert!(!s.user_scrolled_up);
        let max_after = s.viewport_offset;
        assert!(max_after > 0);

        // append more content while at the bottom -> view must follow
        for _ in 0..20 {
            s.add_message("user", &"word ".repeat(200));
        }
        render_to(&mut s, 80, 24);
        assert!(!s.user_scrolled_up);
        assert!(s.viewport_offset > max_after);
    }

    #[test]
    fn stays_pinned_when_user_scrolled_up() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_size(80, 24);
        for _ in 0..40 {
            s.add_message("user", &"word ".repeat(200));
        }
        render_to(&mut s, 80, 24);

        // user scrolls up
        s.user_scrolled_up = true;
        s.viewport_offset = s.viewport_offset.saturating_sub(20);
        let pinned = s.viewport_offset;

        // new content appended -> view must NOT follow
        for _ in 0..20 {
            s.add_message("user", &"word ".repeat(200));
        }
        render_to(&mut s, 80, 24);
        assert!(s.user_scrolled_up);
        assert_eq!(s.viewport_offset, pinned);
    }
}
