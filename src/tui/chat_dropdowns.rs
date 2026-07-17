use crate::prompts::Template;
use crate::tui::theme::COLORS;
use ratatui::{
    layout::{Alignment, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn fuzzy_score(query: &str, hay: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let h: Vec<char> = hay.to_lowercase().chars().collect();
    if q.len() > h.len() {
        return None;
    }
    let mut qi = 0usize;
    let mut score: i64 = 0;
    let mut prev_matched = false;
    let mut first: Option<usize> = None;
    for (hi, hc) in h.iter().enumerate() {
        if qi < q.len() && hc == &q[qi] {
            first.get_or_insert(hi);
            score += if prev_matched { 16 } else { 1 };
            if hi == 0 || !h[hi - 1].is_alphanumeric() {
                score += 8;
            }
            prev_matched = true;
            qi += 1;
        } else {
            prev_matched = false;
        }
    }
    if qi != q.len() {
        return None;
    }
    if let Some(f) = first {
        score -= (f as i64).min(20);
    }
    Some(score)
}

pub const COMMANDS: &[(&str, &str)] = &[
    ("New Session", "new"),
    ("Back to Sessions", "back"),
    ("Turn Tree", "tree"),
    ("Toggle Tool Results", "tools"),
    ("Toggle Thinking", "thinking"),
    ("Switch Model", "model"),
    ("Compact", "compact"),
    ("Copy Last Response", "copy-last"),
    ("Export as Markdown", "export-md"),
    ("Edit Config", "config"),
    ("Quit", "quit"),
];

#[derive(Clone)]
pub struct Dropdown<T: Clone> {
    pub items: Vec<T>,
    pub selected: usize,
    pub visible: bool,
}

impl<T: Clone> Dropdown<T> {
    pub fn new() -> Self {
        Dropdown {
            items: Vec::new(),
            selected: 0,
            visible: false,
        }
    }

    pub fn up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn down(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }

    pub fn selected_item(&self) -> Option<&T> {
        self.items.get(self.selected)
    }
}

impl<T: Clone> Default for Dropdown<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_command_dropdown(
    f: &mut Frame,
    area: Rect,
    dropdown: &Dropdown<(String, String)>,
    query: &str,
) {
    let items: Vec<ListItem> = dropdown
        .items
        .iter()
        .enumerate()
        .map(|(i, (label, _))| {
            if i == dropdown.selected {
                ListItem::new(format!(" {}", label))
                    .style(Style::default().bg(COLORS.selected).fg(COLORS.accent))
            } else {
                ListItem::new(format!(" {}", label)).style(Style::default().fg(COLORS.muted))
            }
        })
        .collect();

    let title = if query.is_empty() {
        "Commands".to_string()
    } else {
        format!("Commands: {}", query)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLORS.border))
        .title(title.as_str());
    let inner = block.inner(area);
    f.render_widget(block, area);
    if dropdown.items.is_empty() {
        f.render_widget(
            Paragraph::new(" No matches")
                .style(Style::default().fg(COLORS.placeholder))
                .alignment(Alignment::Left),
            inner,
        );
        return;
    }
    let list = List::new(items);
    f.render_widget(list, inner);
}

pub fn render_template_dropdown(
    f: &mut Frame,
    area: Rect,
    dropdown: &Dropdown<(Template, String)>,
    query: &str,
) {
    let items: Vec<ListItem> = dropdown
        .items
        .iter()
        .enumerate()
        .map(|(i, (t, _))| {
            let name = format!("/{}", t.name);
            let desc = if t.description.is_empty() {
                String::new()
            } else {
                format!("  {}", t.description)
            };
            let text = format!(" {}{}", name, desc);
            if i == dropdown.selected {
                ListItem::new(text).style(Style::default().bg(COLORS.selected).fg(COLORS.accent))
            } else {
                ListItem::new(text).style(Style::default().fg(COLORS.muted))
            }
        })
        .collect();

    let title = if query.is_empty() {
        "Templates".to_string()
    } else {
        format!("Templates: /{}", query)
    };

    let list = List::new(items);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLORS.border))
        .title(title.as_str());
    let inner = block.inner(area);
    f.render_widget(block, area);
    if dropdown.items.is_empty() {
        f.render_widget(
            Paragraph::new(" No matches")
                .style(Style::default().fg(COLORS.placeholder))
                .alignment(Alignment::Left),
            inner,
        );
        return;
    }
    f.render_widget(list, inner);
}

pub fn render_model_dropdown(
    f: &mut Frame,
    area: Rect,
    dropdown: &Dropdown<(String, String)>,
    current: &str,
) {
    let items: Vec<ListItem> = dropdown
        .items
        .iter()
        .enumerate()
        .map(|(i, (label, _))| {
            let marker = if label == current { "● " } else { "  " };
            let text = format!("{}{}", marker, label);
            if i == dropdown.selected {
                ListItem::new(text).style(Style::default().bg(COLORS.selected).fg(COLORS.accent))
            } else {
                ListItem::new(text).style(Style::default().fg(COLORS.muted))
            }
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLORS.border))
        .title("Switch Model");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if dropdown.items.is_empty() {
        f.render_widget(
            Paragraph::new(" No models configured")
                .style(Style::default().fg(COLORS.placeholder))
                .alignment(Alignment::Left),
            inner,
        );
        return;
    }
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(dropdown.selected));
    let list = List::new(items);
    f.render_stateful_widget(list, inner, &mut state);
}

pub fn render_file_dropdown(f: &mut Frame, area: Rect, dropdown: &Dropdown<(String, String)>) {
    let items: Vec<ListItem> = dropdown
        .items
        .iter()
        .enumerate()
        .map(|(i, (path, _))| {
            if i == dropdown.selected {
                ListItem::new(format!(" {}", path))
                    .style(Style::default().bg(COLORS.selected).fg(COLORS.accent))
            } else {
                ListItem::new(format!(" {}", path)).style(Style::default().fg(COLORS.muted))
            }
        })
        .collect();

    let list = List::new(items);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLORS.border))
        .title("Files");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if dropdown.items.is_empty() {
        f.render_widget(
            Paragraph::new(" No matches")
                .style(Style::default().fg(COLORS.placeholder))
                .alignment(Alignment::Left),
            inner,
        );
        return;
    }
    f.render_widget(list, inner);
}

#[allow(clippy::type_complexity)]
pub fn render_tree_dropdown(
    f: &mut Frame,
    area: Rect,
    dropdown: &Dropdown<(String, String, usize, bool, Vec<bool>, bool)>,
) {
    // items: (turn_id, label, depth, is_last, ancestors, is_current)
    let items: Vec<ListItem> = dropdown
        .items
        .iter()
        .enumerate()
        .map(|(i, (_, label, depth, is_last, ancestors, is_current))| {
            let mut prefix = String::new();
            for d in 0..*depth {
                if d < ancestors.len() && ancestors[d] {
                    prefix.push_str("│  ");
                } else {
                    prefix.push_str("   ");
                }
            }
            if *depth > 0 {
                if *is_last {
                    prefix.push_str("└─ ");
                } else {
                    prefix.push_str("├─ ");
                }
            }
            let marker = if *is_current { "● " } else { "  " };
            let text = format!("{}{}{}", prefix, marker, label);

            if i == dropdown.selected {
                ListItem::new(text).style(Style::default().bg(COLORS.selected).fg(COLORS.accent))
            } else if *is_current {
                ListItem::new(text).style(Style::default().fg(COLORS.accent))
            } else {
                ListItem::new(text).style(Style::default().fg(COLORS.muted))
            }
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(COLORS.border))
            .title("Turn Tree"),
    );
    f.render_widget(list, area);
}
