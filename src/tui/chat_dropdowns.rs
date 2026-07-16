use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use crate::prompts::Template;

pub const COMMANDS: &[(&str, &str)] = &[
    ("New Session", "new"),
    ("Back to Sessions", "back"),
    ("Turn Tree", "tree"),
    ("Toggle Tool Results", "tools"),
    ("Toggle Thinking", "thinking"),
    ("Compact", "compact"),
    ("Copy Last Response", "copy-last"),
    ("Export as Markdown", "export-md"),
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

pub fn render_command_dropdown(f: &mut Frame, area: Rect, dropdown: &Dropdown<(String, String)>) {
    let items: Vec<ListItem> = dropdown
        .items
        .iter()
        .enumerate()
        .map(|(i, (label, _))| {
            if i == dropdown.selected {
                ListItem::new(format!(" {}", label)).style(
                    Style::default()
                        .bg(Color::from_u32(0x00FFC799))
                        .fg(Color::from_u32(0x00171717)),
                )
            } else {
                ListItem::new(format!(" {}", label))
                    .style(Style::default().fg(Color::from_u32(0x00D4D4D4)))
            }
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::from_u32(0x00FFC799)))
            .title("Commands"),
    );
    f.render_widget(list, area);
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
                ListItem::new(text).style(
                    Style::default()
                        .bg(Color::from_u32(0x00FFC799))
                        .fg(Color::from_u32(0x00171717)),
                )
            } else {
                ListItem::new(text)
                    .style(Style::default().fg(Color::from_u32(0x00D4D4D4)))
            }
        })
        .collect();

    let title = if query.is_empty() {
        "Templates".to_string()
    } else {
        format!("Templates: /{}", query)
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::from_u32(0x00FFC799)))
            .title(title.as_str()),
    );
    f.render_widget(list, area);
}

pub fn render_file_dropdown(f: &mut Frame, area: Rect, dropdown: &Dropdown<(String, String)>) {
    let items: Vec<ListItem> = dropdown
        .items
        .iter()
        .enumerate()
        .map(|(i, (path, _))| {
            if i == dropdown.selected {
                ListItem::new(format!(" {}", path)).style(
                    Style::default()
                        .bg(Color::from_u32(0x00FFC799))
                        .fg(Color::from_u32(0x00171717)),
                )
            } else {
                ListItem::new(format!(" {}", path))
                    .style(Style::default().fg(Color::from_u32(0x00D4D4D4)))
            }
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::from_u32(0x00FFC799)))
            .title("Files"),
    );
    f.render_widget(list, area);
}

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
                ListItem::new(text).style(
                    Style::default()
                        .bg(Color::from_u32(0x00FFCB8B))
                        .fg(Color::from_u32(0x00171717)),
                )
            } else if *is_current {
                ListItem::new(text).style(Style::default().fg(Color::from_u32(0x00FFCB8B)))
            } else {
                ListItem::new(text)
                    .style(Style::default().fg(Color::from_u32(0x00D4D4D4)))
            }
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::from_u32(0x00FFCB8B)))
            .title("Turn Tree"),
    );
    f.render_widget(list, area);
}
