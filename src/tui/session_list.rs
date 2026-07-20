use crate::session::Session;
use crate::session::store::Store;
use crate::tui::theme::COLORS;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, List, ListItem, Paragraph},
};

pub struct SessionListScreen {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub loaded: bool,
}

impl SessionListScreen {
    pub fn new() -> Self {
        SessionListScreen {
            sessions: Vec::new(),
            selected: 0,
            loaded: false,
        }
    }
}

impl Default for SessionListScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionListScreen {
    pub fn load(&mut self, store: &mut Store) {
        self.sessions = store.list().unwrap_or_default();
        self.sessions
            .sort_by_key(|s| std::cmp::Reverse(s.updated_at));
        self.sessions.truncate(10);
        self.loaded = true;
    }

    pub fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn select_down(&mut self) {
        if self.selected < self.sessions.len() {
            self.selected += 1;
        }
    }

    pub fn selected_session(&self) -> Option<&Session> {
        if self.selected == 0 {
            None
        } else {
            self.sessions.get(self.selected - 1)
        }
    }

    pub fn is_new_selected(&self) -> bool {
        self.selected == 0
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let accent = COLORS.accent;

        if area.height >= 18 {
            self.render_welcome(f, area, accent);
        } else {
            self.render_compact(f, area, accent);
        }
    }

    fn render_welcome(&self, f: &mut Frame, area: Rect, accent: Color) {
        let logo_lines = [
            "███╗   ███╗ █████╗ ████████╗███████╗",
            "████╗ ████║██╔══██╗╚══██╔══╝██╔════╝",
            "██╔████╔██║███████║   ██║   █████╗  ",
            "██║╚██╔╝██║██╔══██║   ██║   ██╔══╝  ",
            "██║ ╚═╝ ██║██║  ██║   ██║   ███████╗",
            "╚═╝     ╚═╝╚═╝  ╚═╝   ╚═╝   ╚══════╝",
        ];

        let mut logo_area = area;
        logo_area.y += 1;
        logo_area.height = 7;

        for (i, line) in logo_lines.iter().enumerate() {
            let p = Paragraph::new(*line)
                .style(Style::default().fg(accent))
                .alignment(Alignment::Center);
            let line_area = Rect::new(area.x, logo_area.y + i as u16, area.width, 1);
            f.render_widget(p, line_area);
        }

        let list_area = Rect::new(
            area.x,
            logo_area.y + 8,
            area.width.min(55),
            area.height.saturating_sub(10),
        );
        let centered_x = area.x + (area.width.saturating_sub(list_area.width)) / 2;
        let centered_list = Rect::new(centered_x, list_area.y, list_area.width, list_area.height);

        self.render_list(f, centered_list, accent);
    }

    fn render_compact(&self, f: &mut Frame, area: Rect, accent: Color) {
        let w = area.width.min(55);
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let list_area = Rect::new(x, area.y, w, area.height);
        self.render_list(f, list_area, accent);
    }

    fn render_list(&self, f: &mut Frame, area: Rect, accent: Color) {
        let mut items: Vec<ListItem> = Vec::new();

        if !self.loaded {
            let item =
                ListItem::new("Loading sessions...").style(Style::default().fg(COLORS.placeholder));
            items.push(item);
        } else {
            let new_lines = vec![Line::raw("  New Session"), Line::raw("")];
            let new_item = if self.selected == 0 {
                ListItem::new(new_lines).style(Style::default().fg(accent))
            } else {
                ListItem::new(new_lines).style(Style::default().fg(COLORS.muted))
            };
            items.push(new_item);

            for (i, s) in self.sessions.iter().enumerate() {
                let idx = i + 1;
                let name = s.name.replace('\n', " ");
                let desc = if s.cost > 0.0 {
                    format!(
                        "{} turns · ${:.4} · {}",
                        s.turn_count,
                        s.cost,
                        s.updated_at.format("%Y-%m-%d %H:%M")
                    )
                } else {
                    format!(
                        "{} turns · {}",
                        s.turn_count,
                        s.updated_at.format("%Y-%m-%d %H:%M")
                    )
                };
                let lines = vec![
                    Line::raw(format!("  {}", name)),
                    Line::raw(format!("    {}", desc)),
                    Line::raw(""),
                ];

                if self.selected == idx {
                    items.push(ListItem::new(lines).style(Style::default().fg(accent)));
                } else {
                    items.push(ListItem::new(lines).style(Style::default().fg(COLORS.muted)));
                }
            }
        }

        let list = List::new(items).block(Block::default());
        f.render_widget(list, area);
    }
}
