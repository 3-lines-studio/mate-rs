use crate::config::{save_config, Config, ModelConfig, ProviderConfig, SubagentConfig};
use crate::tui::chat_dropdowns::Dropdown;
use crate::tui::theme::COLORS;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Alignment, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

enum Row {
    Section(usize),
    Item(usize, usize),
    Field {
        si: usize,
        ii: Option<usize>,
        fi: usize,
    },
    EmptyHint,
}

enum Edit {
    None,
    Text(String, usize),
    Pick(Dropdown<String>),
}

struct RenderLine {
    text: String,
    selected: bool,
    is_section_hdr: bool,
}

pub struct ConfigScreen {
    config: Config,
    dir: String,
    rows: Vec<Row>,
    collapsed: Vec<bool>,
    row: usize,
    scroll: usize,
    view_h: usize,
    edit: Edit,
    dirty: bool,
    msg: String,
    pending_add_section: Option<usize>,
}

const SECTION_NAMES: &[&str] = &[
    "agent",
    "tui",
    "session",
    "telegram",
    "providers",
    "models",
    "subagents",
    "schedule",
];

const AGENT_FIELDS: &[(&str, FieldKind)] = &[
    ("model", FieldKind::PickModels),
    ("max_tool_rounds", FieldKind::I32),
    ("tools", FieldKind::Comma),
    ("interfaces", FieldKind::Comma),
    ("compaction_model", FieldKind::PickModels),
];

const TUI_FIELDS: &[(&str, FieldKind)] = &[
    ("tools_expanded", FieldKind::Bool),
    ("show_thinking", FieldKind::Bool),
];

const SESSION_FIELDS: &[(&str, FieldKind)] = &[("dir", FieldKind::Text)];

const TELEGRAM_FIELDS: &[(&str, FieldKind)] = &[("allowed_users", FieldKind::CommaI64)];

const PROVIDER_FIELDS: &[(&str, FieldKind)] =
    &[("id", FieldKind::Text), ("base_url", FieldKind::Text)];

const MODEL_FIELDS: &[(&str, FieldKind)] = &[
    ("id", FieldKind::Text),
    ("name", FieldKind::Text),
    ("provider", FieldKind::PickProviders),
    ("description", FieldKind::Text),
    ("context_window", FieldKind::I32),
    ("max_output_tokens", FieldKind::I32),
    ("thinking_type", FieldKind::Text),
    ("reasoning_effort", FieldKind::Text),
    ("reasoning_max_tokens", FieldKind::I32),
    ("input_price", FieldKind::F64),
    ("cached_input_price", FieldKind::F64),
    ("output_price", FieldKind::F64),
    ("prompt_cache", FieldKind::Bool),
    ("prompt_cache_ttl", FieldKind::Text),
    ("fallback_models", FieldKind::Comma),
    ("route", FieldKind::Text),
    ("provider_sort", FieldKind::Text),
];

const SUBAGENT_FIELDS: &[(&str, FieldKind)] = &[
    ("id", FieldKind::Text),
    ("description", FieldKind::Text),
    ("model", FieldKind::PickModels),
    ("tools", FieldKind::Comma),
];

const SCHEDULE_JOB_FIELDS: &[(&str, FieldKind)] = &[
    ("cron", FieldKind::Text),
    ("channel", FieldKind::Text),
    ("model", FieldKind::PickModels),
];

#[derive(Clone, Copy, PartialEq)]
enum FieldKind {
    Text,
    I32,
    F64,
    Bool,
    Comma,
    CommaI64,
    PickModels,
    PickProviders,
}

impl ConfigScreen {
    pub fn new(dir: String) -> Self {
        let config = crate::config::load_from(&dir).unwrap_or_default();
        let mut s = ConfigScreen {
            config,
            dir,
            rows: Vec::new(),
            collapsed: vec![true; SECTION_NAMES.len()],
            row: 0,
            scroll: 0,
            view_h: 20,
            edit: Edit::None,
            dirty: false,
            msg: String::new(),
            pending_add_section: None,
        };
        s.rebuild_rows();
        s
    }

    pub fn reload(&mut self) {
        if let Ok(cfg) = crate::config::load_from(&self.dir) {
            self.config = cfg;
        }
        self.row = 0;
        self.scroll = 0;
        self.edit = Edit::None;
        self.dirty = false;
        self.msg = String::new();
        self.rebuild_rows();
    }

    fn is_array_section(&self, idx: usize) -> bool {
        matches!(idx, 4..=7)
    }

    fn array_len(&self, section_idx: usize) -> usize {
        match section_idx {
            4 => self.config.providers.len(),
            5 => self.config.models.len(),
            6 => self.config.subagents.len(),
            7 => self.config.schedule.jobs.len(),
            _ => 0,
        }
    }

    fn field_count_for_section(&self, section_idx: usize) -> usize {
        match section_idx {
            0 => AGENT_FIELDS.len(),
            1 => TUI_FIELDS.len(),
            2 => SESSION_FIELDS.len(),
            3 => TELEGRAM_FIELDS.len(),
            4 => PROVIDER_FIELDS.len(),
            5 => MODEL_FIELDS.len(),
            6 => SUBAGENT_FIELDS.len(),
            7 => SCHEDULE_JOB_FIELDS.len(),
            _ => 0,
        }
    }

    fn fields_for_section(&self, section_idx: usize) -> &[(&str, FieldKind)] {
        match section_idx {
            0 => AGENT_FIELDS,
            1 => TUI_FIELDS,
            2 => SESSION_FIELDS,
            3 => TELEGRAM_FIELDS,
            4 => PROVIDER_FIELDS,
            5 => MODEL_FIELDS,
            6 => SUBAGENT_FIELDS,
            7 => SCHEDULE_JOB_FIELDS,
            _ => &[],
        }
    }

    fn build_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (si, _) in SECTION_NAMES.iter().enumerate() {
            rows.push(Row::Section(si));
            if self.collapsed[si] {
                continue;
            }
            if self.is_array_section(si) {
                if self.array_len(si) == 0 {
                    rows.push(Row::EmptyHint);
                } else {
                    for ii in 0..self.array_len(si) {
                        rows.push(Row::Item(si, ii));
                        for fi in 0..self.field_count_for_section(si) {
                            rows.push(Row::Field {
                                si,
                                ii: Some(ii),
                                fi,
                            });
                        }
                    }
                }
            } else {
                for fi in 0..self.field_count_for_section(si) {
                    rows.push(Row::Field { si, ii: None, fi });
                }
            }
        }
        rows
    }

    fn rebuild_rows(&mut self) {
        self.rows = self.build_rows();
        if self.row >= self.rows.len() {
            self.row = self.rows.len().saturating_sub(1);
        }
        while self.row > 0 && matches!(self.rows.get(self.row), Some(Row::EmptyHint)) {
            self.row -= 1;
        }
    }

    fn field_at_cursor(&self) -> Option<(usize, Option<usize>, usize, String, FieldKind)> {
        let (si, ii, fi) = match self.rows.get(self.row)? {
            Row::Field { si, ii, fi } => (*si, *ii, *fi),
            _ => return None,
        };
        let &(name, kind) = self.fields_for_section(si).get(fi)?;
        Some((si, ii, fi, name.to_string(), kind))
    }

    fn section_at_cursor(&self) -> Option<usize> {
        self.rows[..=self.row].iter().rev().find_map(|r| match r {
            Row::Section(si) => Some(*si),
            _ => None,
        })
    }

    fn item_at_cursor(&self) -> Option<(usize, usize)> {
        self.rows[..=self.row].iter().rev().find_map(|r| match r {
            Row::Item(si, ii) => Some((*si, *ii)),
            _ => None,
        })
    }

    fn get_agent_field(&self, name: &str) -> String {
        match name {
            "model" => self.config.agent.model.clone(),
            "max_tool_rounds" => self.config.agent.max_tool_rounds.to_string(),
            "tools" => self.config.agent.tools.join(", "),
            "interfaces" => self.config.agent.interfaces.join(", "),
            "compaction_model" => self.config.agent.compaction_model.clone(),
            _ => String::new(),
        }
    }

    fn set_agent_field(&mut self, name: &str, value: &str) -> Result<(), String> {
        match name {
            "model" => self.config.agent.model = value.to_string(),
            "max_tool_rounds" => {
                self.config.agent.max_tool_rounds = value
                    .parse::<i32>()
                    .map_err(|_| format!("invalid i32: {}", value))?
            }
            "tools" => self.config.agent.tools = parse_comma(value),
            "interfaces" => self.config.agent.interfaces = parse_comma(value),
            "compaction_model" => self.config.agent.compaction_model = value.to_string(),
            _ => {}
        }
        Ok(())
    }

    fn get_tui_field(&self, name: &str) -> String {
        match name {
            "tools_expanded" => self.config.tui.tools_expanded.to_string(),
            "show_thinking" => self.config.tui.show_thinking.to_string(),
            _ => String::new(),
        }
    }

    fn set_tui_field(&mut self, name: &str, value: &str) -> Result<(), String> {
        match name {
            "tools_expanded" => {
                self.config.tui.tools_expanded = value
                    .parse::<bool>()
                    .map_err(|_| format!("invalid bool: {}", value))?
            }
            "show_thinking" => {
                self.config.tui.show_thinking = value
                    .parse::<bool>()
                    .map_err(|_| format!("invalid bool: {}", value))?
            }
            _ => {}
        }
        Ok(())
    }

    fn get_session_field(&self, name: &str) -> String {
        match name {
            "dir" => self.config.session.dir.clone(),
            _ => String::new(),
        }
    }

    fn set_session_field(&mut self, name: &str, value: &str) -> Result<(), String> {
        if name == "dir" {
            self.config.session.dir = value.to_string();
        }
        Ok(())
    }

    fn get_telegram_field(&self, name: &str) -> String {
        match name {
            "allowed_users" => comma_i64(&self.config.telegram.allowed_users),
            _ => String::new(),
        }
    }

    fn set_telegram_field(&mut self, name: &str, value: &str) -> Result<(), String> {
        if name == "allowed_users" {
            self.config.telegram.allowed_users =
                parse_comma_i64(value).map_err(|_| format!("invalid i64 list: {}", value))?
        }
        Ok(())
    }

    fn get_provider_field(&self, idx: usize, name: &str) -> String {
        if let Some(p) = self.config.providers.get(idx) {
            match name {
                "id" => p.id.clone(),
                "base_url" => p.base_url.clone(),
                _ => String::new(),
            }
        } else {
            String::new()
        }
    }

    fn set_provider_field(&mut self, idx: usize, name: &str, value: &str) -> Result<(), String> {
        if let Some(p) = self.config.providers.get_mut(idx) {
            match name {
                "id" => p.id = value.to_string(),
                "base_url" => p.base_url = value.to_string(),
                _ => {}
            }
        }
        Ok(())
    }

    fn get_model_field(&self, idx: usize, name: &str) -> String {
        if let Some(m) = self.config.models.get(idx) {
            match name {
                "id" => m.id.clone(),
                "name" => m.name.clone(),
                "provider" => m.provider.clone(),
                "description" => m.description.clone(),
                "context_window" => m.context_window.to_string(),
                "max_output_tokens" => m.max_output_tokens.to_string(),
                "thinking_type" => m.thinking_type.clone(),
                "reasoning_effort" => m.reasoning_effort.clone(),
                "reasoning_max_tokens" => m.reasoning_max_tokens.to_string(),
                "input_price" => format_f64(m.input_price),
                "cached_input_price" => format_f64(m.cached_input_price),
                "output_price" => format_f64(m.output_price),
                "prompt_cache" => m.prompt_cache.to_string(),
                "prompt_cache_ttl" => m.prompt_cache_ttl.clone(),
                "fallback_models" => m.fallback_models.join(", "),
                "route" => m.route.clone(),
                "provider_sort" => m.provider_sort.clone(),
                _ => String::new(),
            }
        } else {
            String::new()
        }
    }

    fn set_model_field(&mut self, idx: usize, name: &str, value: &str) -> Result<(), String> {
        if let Some(m) = self.config.models.get_mut(idx) {
            match name {
                "id" => m.id = value.to_string(),
                "name" => m.name = value.to_string(),
                "provider" => m.provider = value.to_string(),
                "description" => m.description = value.to_string(),
                "context_window" => {
                    m.context_window = value
                        .parse::<i32>()
                        .map_err(|_| format!("invalid i32: {}", value))?
                }
                "max_output_tokens" => {
                    m.max_output_tokens = value
                        .parse::<i32>()
                        .map_err(|_| format!("invalid i32: {}", value))?
                }
                "thinking_type" => m.thinking_type = value.to_string(),
                "reasoning_effort" => m.reasoning_effort = value.to_string(),
                "reasoning_max_tokens" => {
                    m.reasoning_max_tokens = value
                        .parse::<i32>()
                        .map_err(|_| format!("invalid i32: {}", value))?
                }
                "input_price" => {
                    m.input_price = value
                        .parse::<f64>()
                        .map_err(|_| format!("invalid f64: {}", value))?
                }
                "cached_input_price" => {
                    m.cached_input_price = value
                        .parse::<f64>()
                        .map_err(|_| format!("invalid f64: {}", value))?
                }
                "output_price" => {
                    m.output_price = value
                        .parse::<f64>()
                        .map_err(|_| format!("invalid f64: {}", value))?
                }
                "prompt_cache" => {
                    m.prompt_cache = value
                        .parse::<bool>()
                        .map_err(|_| format!("invalid bool: {}", value))?
                }
                "prompt_cache_ttl" => m.prompt_cache_ttl = value.to_string(),
                "fallback_models" => m.fallback_models = parse_comma(value),
                "route" => m.route = value.to_string(),
                "provider_sort" => m.provider_sort = value.to_string(),
                _ => {}
            }
        }
        Ok(())
    }

    fn get_subagent_field(&self, idx: usize, name: &str) -> String {
        if let Some(s) = self.config.subagents.get(idx) {
            match name {
                "id" => s.id.clone(),
                "description" => s.description.clone(),
                "model" => s.model.clone(),
                "tools" => s.tools.join(", "),
                _ => String::new(),
            }
        } else {
            String::new()
        }
    }

    fn set_subagent_field(&mut self, idx: usize, name: &str, value: &str) -> Result<(), String> {
        if let Some(s) = self.config.subagents.get_mut(idx) {
            match name {
                "id" => s.id = value.to_string(),
                "description" => s.description = value.to_string(),
                "model" => s.model = value.to_string(),
                "tools" => s.tools = parse_comma(value),
                _ => {}
            }
        }
        Ok(())
    }

    fn get_job_field(&self, idx: usize, name: &str) -> String {
        if let Some(j) = self.config.schedule.jobs.get(idx) {
            match name {
                "cron" => j.cron.clone(),
                "channel" => j.channel.clone(),
                "model" => j.model.clone(),
                _ => String::new(),
            }
        } else {
            String::new()
        }
    }

    fn set_job_field(&mut self, idx: usize, name: &str, value: &str) -> Result<(), String> {
        if let Some(j) = self.config.schedule.jobs.get_mut(idx) {
            match name {
                "cron" => j.cron = value.to_string(),
                "channel" => j.channel = value.to_string(),
                "model" => j.model = value.to_string(),
                _ => {}
            }
        }
        Ok(())
    }

    fn get_scalar_field_value(&self, section_idx: usize, field_name: &str) -> String {
        match section_idx {
            0 => self.get_agent_field(field_name),
            1 => self.get_tui_field(field_name),
            2 => self.get_session_field(field_name),
            3 => self.get_telegram_field(field_name),
            _ => String::new(),
        }
    }

    fn set_scalar_field_value(
        &mut self,
        section_idx: usize,
        field_name: &str,
        value: &str,
    ) -> Result<(), String> {
        match section_idx {
            0 => self.set_agent_field(field_name, value),
            1 => self.set_tui_field(field_name, value),
            2 => self.set_session_field(field_name, value),
            3 => self.set_telegram_field(field_name, value),
            _ => Ok(()),
        }
    }

    fn get_item_field_value(
        &self,
        section_idx: usize,
        item_idx: usize,
        field_name: &str,
    ) -> String {
        match section_idx {
            4 => self.get_provider_field(item_idx, field_name),
            5 => self.get_model_field(item_idx, field_name),
            6 => self.get_subagent_field(item_idx, field_name),
            7 => self.get_job_field(item_idx, field_name),
            _ => String::new(),
        }
    }

    fn set_item_field_value(
        &mut self,
        section_idx: usize,
        item_idx: usize,
        field_name: &str,
        value: &str,
    ) -> Result<(), String> {
        match section_idx {
            4 => self.set_provider_field(item_idx, field_name, value),
            5 => self.set_model_field(item_idx, field_name, value),
            6 => self.set_subagent_field(item_idx, field_name, value),
            7 => self.set_job_field(item_idx, field_name, value),
            _ => Ok(()),
        }
    }

    fn pick_options(&self, kind: FieldKind) -> Vec<String> {
        match kind {
            FieldKind::PickModels => self.config.models.iter().map(|m| m.id.clone()).collect(),
            FieldKind::PickProviders => {
                self.config.providers.iter().map(|p| p.id.clone()).collect()
            }
            _ => Vec::new(),
        }
    }

    fn cur_value_at(&self) -> String {
        if let Some((si, ii, _fi, name, _kind)) = self.field_at_cursor() {
            if let Some(item_idx) = ii {
                self.get_item_field_value(si, item_idx, &name)
            } else {
                self.get_scalar_field_value(si, &name)
            }
        } else {
            String::new()
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<bool> {
        if key.kind != KeyEventKind::Press {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        if ctrl && key.code == KeyCode::Char('s') {
            self.save();
            return None;
        }

        match &self.edit {
            Edit::Text(_, _) => {
                return self.handle_text_key(key);
            }
            Edit::Pick(_) => {
                return self.handle_pick_key(key);
            }
            Edit::None => {}
        }

        match key.code {
            KeyCode::Up => {
                if self.row > 0 {
                    self.row -= 1;
                    if matches!(self.rows.get(self.row), Some(Row::EmptyHint)) && self.row > 0 {
                        self.row -= 1;
                    }
                    if self.row < self.scroll {
                        self.scroll = self.row;
                    }
                }
            }
            KeyCode::Down => {
                let max = self.rows.len().saturating_sub(1);
                if self.row < max {
                    self.row += 1;
                    if matches!(self.rows.get(self.row), Some(Row::EmptyHint)) && self.row < max {
                        self.row += 1;
                    }
                    if self.row >= self.scroll + self.view_h {
                        self.scroll = self.row + 1 - self.view_h;
                    }
                }
            }
            KeyCode::Enter => self.enter(),
            KeyCode::Esc => {
                self.msg.clear();
                return Some(true);
            }
            KeyCode::Char('a') => self.add_item(),
            KeyCode::Char('d') => self.delete_item(),
            _ => {}
        }
        None
    }

    fn enter(&mut self) {
        self.msg.clear();
        match self.rows.get(self.row) {
            Some(Row::Section(si)) => {
                self.collapsed[*si] = !self.collapsed[*si];
                self.rebuild_rows();
                return;
            }
            Some(Row::Item(_, _)) => {
                if let Some(pos) = (self.row + 1..self.rows.len())
                    .find(|&i| matches!(self.rows.get(i), Some(Row::Field { .. })))
                {
                    self.row = pos;
                }
                return;
            }
            Some(Row::Field { .. }) => {}
            _ => return,
        }
        if let Some((si, ii, _fi, _name, kind)) = self.field_at_cursor() {
            if kind == FieldKind::Bool {
                self.toggle_bool(si, ii);
                return;
            }
            if matches!(kind, FieldKind::PickModels | FieldKind::PickProviders) {
                let options = self.pick_options(kind);
                let cur = self.cur_value_at();
                let mut dd = Dropdown::new();
                dd.items = options;
                dd.visible = true;
                dd.selected = dd.items.iter().position(|o| o == &cur).unwrap_or(0);
                self.edit = Edit::Pick(dd);
                return;
            }
            let val = self.cur_value_at();
            self.edit = Edit::Text(val.clone(), val.len());
        }
    }

    fn toggle_bool(&mut self, si: usize, ii: Option<usize>) {
        if let Some((_, _, _, name, _)) = self.field_at_cursor() {
            let cur = if let Some(item_idx) = ii {
                self.get_item_field_value(si, item_idx, &name)
            } else {
                self.get_scalar_field_value(si, &name)
            };
            let new_val = if cur == "true" { "false" } else { "true" };
            if let Some(item_idx) = ii {
                let _ = self.set_item_field_value(si, item_idx, &name, new_val);
            } else {
                let _ = self.set_scalar_field_value(si, &name, new_val);
            }
            self.dirty = true;
        }
    }

    fn add_item(&mut self) {
        self.msg.clear();
        let Some(si) = self.section_at_cursor() else {
            return;
        };
        if !self.is_array_section(si) {
            return;
        }
        self.edit = Edit::Text(String::new(), 0);
        self.pending_add_section = Some(si);
    }

    fn delete_item(&mut self) {
        self.msg.clear();
        let Some((si, ii)) = self.item_at_cursor() else {
            return;
        };
        match si {
            4 => {
                self.config.providers.remove(ii);
            }
            5 => {
                self.config.models.remove(ii);
            }
            6 => {
                self.config.subagents.remove(ii);
            }
            7 => {
                self.config.schedule.jobs.remove(ii);
            }
            _ => return,
        }
        self.dirty = true;
        self.rebuild_rows();
    }

    fn handle_text_key(&mut self, key: KeyEvent) -> Option<bool> {
        if let Edit::Text(ref buf, ref cursor) = &self.edit {
            let mut buf = buf.clone();
            let mut cursor = *cursor;
            match key.code {
                KeyCode::Esc => {
                    self.edit = Edit::None;
                }
                KeyCode::Enter => {
                    self.edit = Edit::None;
                    self.apply_text(&buf);
                }
                KeyCode::Backspace => {
                    if cursor > 0 {
                        let prev = buf[..cursor].chars().next_back().unwrap().len_utf8();
                        cursor -= prev;
                        buf.remove(cursor);
                    }
                    self.edit = Edit::Text(buf, cursor);
                }
                KeyCode::Left => {
                    if cursor > 0 {
                        let prev = buf[..cursor].chars().next_back().unwrap().len_utf8();
                        cursor -= prev;
                    }
                    self.edit = Edit::Text(buf, cursor);
                }
                KeyCode::Right => {
                    if cursor < buf.len() {
                        let next = buf[cursor..].chars().next().unwrap().len_utf8();
                        cursor += next;
                    }
                    self.edit = Edit::Text(buf, cursor);
                }
                KeyCode::Home => {
                    cursor = 0;
                    self.edit = Edit::Text(buf, cursor);
                }
                KeyCode::End => {
                    cursor = buf.len();
                    self.edit = Edit::Text(buf, cursor);
                }
                KeyCode::Char(c) => {
                    buf.insert(cursor, c);
                    cursor += c.len_utf8();
                    self.edit = Edit::Text(buf, cursor);
                }
                _ => {}
            }
        }
        None
    }

    fn handle_pick_key(&mut self, key: KeyEvent) -> Option<bool> {
        if let Edit::Pick(ref dd) = &self.edit {
            let mut dd = dd.clone();
            match key.code {
                KeyCode::Esc => {
                    self.edit = Edit::None;
                }
                KeyCode::Enter => {
                    let sel = dd.selected_item().cloned();
                    self.edit = Edit::None;
                    if let Some(val) = sel {
                        self.apply_text(&val);
                    }
                }
                KeyCode::Up => {
                    dd.up();
                    self.edit = Edit::Pick(dd);
                }
                KeyCode::Down => {
                    dd.down();
                    self.edit = Edit::Pick(dd);
                }
                _ => {}
            }
        }
        None
    }

    fn apply_text(&mut self, value: &str) {
        if let Some(si) = self.pending_add_section.take() {
            self.collapsed[si] = false;
            self.do_add_item(si, value);
            self.rebuild_rows();
            if let Some(pos) = self
                .rows
                .iter()
                .rposition(|r| matches!(r, Row::Item(s, _) if *s == si))
            {
                self.row = pos;
            }
            return;
        }
        if let Some((si, ii, _fi, name, _kind)) = self.field_at_cursor() {
            let res = if let Some(item_idx) = ii {
                self.set_item_field_value(si, item_idx, &name, value)
            } else {
                self.set_scalar_field_value(si, &name, value)
            };
            match res {
                Ok(()) => self.dirty = true,
                Err(e) => self.msg = e,
            }
        }
    }

    fn do_add_item(&mut self, section_idx: usize, id: &str) {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return;
        }
        match section_idx {
            4 => {
                self.config.providers.push(ProviderConfig {
                    id: trimmed.to_string(),
                    base_url: String::new(),
                    api_key: String::new(),
                });
            }
            5 => {
                self.config.models.push(ModelConfig {
                    id: trimmed.to_string(),
                    name: trimmed.to_string(),
                    provider: String::new(),
                    description: String::new(),
                    context_window: 0,
                    max_output_tokens: 0,
                    thinking_type: String::new(),
                    reasoning_effort: String::new(),
                    reasoning_max_tokens: 0,
                    input_price: 0.0,
                    cached_input_price: 0.0,
                    output_price: 0.0,
                    prompt_cache: false,
                    prompt_cache_ttl: String::new(),
                    fallback_models: Vec::new(),
                    route: String::new(),
                    provider_sort: String::new(),
                });
            }
            6 => {
                self.config.subagents.push(SubagentConfig {
                    id: trimmed.to_string(),
                    description: String::new(),
                    model: String::new(),
                    tools: Vec::new(),
                    prompt: String::new(),
                });
            }
            7 => {
                self.config.schedule.jobs.push(crate::config::ScheduledJob {
                    cron: String::new(),
                    prompt: String::new(),
                    channel: String::new(),
                    model: String::new(),
                });
            }
            _ => {}
        }
        self.dirty = true;
    }

    fn save(&mut self) {
        match save_config(&self.dir, &self.config) {
            Ok(()) => {
                self.dirty = false;
                self.msg = "Saved.".to_string();
            }
            Err(e) => {
                self.msg = format!("Save error: {}", e);
            }
        }
    }

    pub fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        let title = if self.dirty {
            "* Config Editor"
        } else {
            "Config Editor"
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(COLORS.border))
            .title(title)
            .title_alignment(Alignment::Left);
        let hint = "[↑↓] scroll  [⏎] toggle/edit  [a] add  [d] delete  [^S] save  [Esc] close";
        let hint_area = Rect::new(area.x, area.y, area.width, 1);
        let hint_p = Paragraph::new(hint)
            .style(Style::default().fg(COLORS.muted))
            .alignment(Alignment::Right);
        f.render_widget(hint_p, hint_area);

        let content_area = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(3),
        );
        let inner = block.inner(content_area);
        f.render_widget(block, content_area);

        self.view_h = inner.height as usize;

        let items = self.build_render_items();
        let visible = inner.height as usize;
        let end = (self.scroll + visible).min(items.len());
        let slice: Vec<ListItem> = items[self.scroll..end]
            .iter()
            .map(|rl| {
                let style = if rl.selected {
                    Style::default().bg(COLORS.selected).fg(COLORS.accent)
                } else if rl.is_section_hdr {
                    Style::default().fg(COLORS.accent)
                } else {
                    Style::default().fg(COLORS.fg)
                };
                ListItem::new(rl.text.as_str()).style(style)
            })
            .collect();
        let list = List::new(slice);
        f.render_widget(list, inner);

        let status_area = Rect::new(area.x, area.height.saturating_sub(1), area.width, 1);
        let status_line = if !self.msg.is_empty() {
            Paragraph::new(self.msg.as_str()).style(Style::default().fg(COLORS.accent))
        } else {
            Paragraph::new(self.location_hint()).style(Style::default().fg(COLORS.muted))
        };
        f.render_widget(status_line, status_area);

        if let Edit::Text(ref buf, cursor) = self.edit {
            let (rel_x, rel_y) = self.cursor_position(buf, cursor, inner);
            let x = inner.x + rel_x;
            let y = inner.y + rel_y;
            f.set_cursor_position((x, y));
        }

        if let Edit::Pick(ref dd) = self.edit {
            self.render_pick_dropdown(f, dd);
        }
    }

    fn cursor_position(&self, buf: &str, cursor: usize, list_area: Rect) -> (u16, u16) {
        let items = self.build_render_items();
        let visible = list_area.height as usize;
        let end = (self.scroll + visible).min(items.len());
        let slice = &items[self.scroll..end];
        let row_offset = slice.iter().position(|rl| rl.selected).unwrap_or(0);
        let indent = match self.field_at_cursor() {
            Some((_, ii, _, _, _)) if ii.is_some() => "    ",
            _ => "  ",
        };
        let name = self
            .field_at_cursor()
            .map(|(_, _, _, n, _)| n)
            .unwrap_or_default();
        let prefix = format!("{}{}: ", indent, name);
        let prefix_len = unicode_display_width(&prefix);
        let text_before = &buf[..cursor];
        let col = prefix_len + unicode_display_width(text_before);
        (
            (col as u16).min(list_area.width.saturating_sub(1)),
            row_offset as u16,
        )
    }

    fn render_pick_dropdown(&self, f: &mut Frame, dd: &Dropdown<String>) {
        let max_width = dd.items.iter().map(|s| s.len()).max().unwrap_or(0) as u16 + 4;
        let height = (dd.items.len() as u16).min(10) + 2;
        let popup_w = max_width.max(16);
        let popup_h = height;
        let area = f.area();
        let x = (area.width.saturating_sub(popup_w)) / 2;
        let y = (area.height.saturating_sub(popup_h)) / 2;
        let popup = Rect::new(x, y, popup_w, popup_h);
        f.render_widget(Clear, popup);
        let items: Vec<ListItem> = dd
            .items
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if i == dd.selected {
                    ListItem::new(format!(" {}", s))
                        .style(Style::default().bg(COLORS.selected).fg(COLORS.accent))
                } else {
                    ListItem::new(format!(" {}", s)).style(Style::default().fg(COLORS.muted))
                }
            })
            .collect();
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(COLORS.border)),
        );
        f.render_widget(list, popup);
    }

    fn location_hint(&self) -> &str {
        self.section_at_cursor()
            .and_then(|si| SECTION_NAMES.get(si).copied())
            .unwrap_or("")
    }

    fn build_render_items(&self) -> Vec<RenderLine> {
        let mut items = Vec::new();
        let edit_buf = match &self.edit {
            Edit::Text(buf, _) => Some(buf.as_str()),
            _ => None,
        };
        for (i, row) in self.rows.iter().enumerate() {
            match row {
                Row::Section(si) => {
                    let name = SECTION_NAMES[*si];
                    let marker = if self.collapsed[*si] { '▸' } else { '▾' };
                    let extra = if self.is_array_section(*si) {
                        format!(" [{}]", self.array_len(*si))
                    } else {
                        String::new()
                    };
                    items.push(RenderLine {
                        text: format!("{} {}{}", marker, name, extra),
                        selected: i == self.row,
                        is_section_hdr: true,
                    });
                }
                Row::Item(si, ii) => {
                    let id = match si {
                        4 => self.config.providers[*ii].id.clone(),
                        5 => self.config.models[*ii].id.clone(),
                        6 => self.config.subagents[*ii].id.clone(),
                        7 => self.config.schedule.jobs[*ii].cron.clone(),
                        _ => String::new(),
                    };
                    items.push(RenderLine {
                        text: format!("  ▸ {}", id),
                        selected: i == self.row,
                        is_section_hdr: false,
                    });
                }
                Row::Field { si, ii, fi } => {
                    let fields = self.fields_for_section(*si);
                    let &(name, _kind) = fields.get(*fi).unwrap();
                    let indent = if ii.is_some() { "    " } else { "  " };
                    let mut value = if let Some(item_idx) = ii {
                        self.get_item_field_value(*si, *item_idx, name)
                    } else {
                        self.get_scalar_field_value(*si, name)
                    };
                    if let Some(buf) = edit_buf {
                        if i == self.row {
                            value = buf.to_string();
                        }
                    }
                    let text = if value.is_empty() {
                        format!("{}{}", indent, name)
                    } else {
                        format!("{}{}: {}", indent, name, value)
                    };
                    items.push(RenderLine {
                        text,
                        selected: i == self.row,
                        is_section_hdr: false,
                    });
                }
                Row::EmptyHint => {
                    items.push(RenderLine {
                        text: "  (empty — press a to add)".to_string(),
                        selected: false,
                        is_section_hdr: false,
                    });
                }
            }
        }
        items
    }
}

fn parse_comma(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn parse_comma_i64(s: &str) -> Result<Vec<i64>, std::num::ParseIntError> {
    s.split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<i64>())
        .collect()
}

fn comma_i64(v: &[i64]) -> String {
    v.iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_f64(v: f64) -> String {
    let s = format!("{:.6}", v);
    let trimmed = s.trim_end_matches('0');
    if trimmed.ends_with('.') {
        format!("{}0", trimmed)
    } else {
        trimmed.to_string()
    }
}

fn unicode_display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    UnicodeWidthStr::width(s)
}
