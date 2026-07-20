// Standalone preview: renders a full CommonMark/GFM sample through the exact
// pipeline the chat view uses (StreamRenderer -> ansi_to_text -> ratatui Paragraph).
//
//   cargo run --example mdcat_tui_preview
//
// Controls: j/Down  pgdn   scroll down
//           k/Up    pgup   scroll up
//           g/Home         top
//           G/End          bottom
//           q / Esc        quit

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use mate::render::{StreamRenderer, block::ansi_to_text};

const SAMPLE: &str = include_str!("mdcat_sample.md");

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let mut scroll: u16 = 0;
    let mut last_width: u16 = 0;
    let mut cached: ratatui::text::Text<'static> = ratatui::text::Text::from("");

    loop {
        terminal.draw(|f| {
            let chunks =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(f.area());
            let body = chunks[0];
            let bar = chunks[1];

            let width = body.width.max(1);
            if width != last_width {
                let renderer = StreamRenderer::new((width as usize).saturating_sub(5));
                let ansi = renderer.render(SAMPLE);
                cached = ansi_to_text(&ansi);
                last_width = width;
            }

            let total = cached.lines.len() as u16;
            let visible = body.height;
            let max_scroll = total.saturating_sub(visible);
            if scroll > max_scroll {
                scroll = max_scroll;
            }

            let paragraph = Paragraph::new(cached.clone()).scroll((scroll, 0)).block(
                Block::default()
                    .borders(Borders::TOP)
                    .title(" mdcat preview — q to quit "),
            );
            f.render_widget(paragraph, body);

            let info = format!(
                " {} / {} lines  ·  width {}  ·  j/k scroll  ·  q quit ",
                scroll, total, width
            );
            let spans = vec![Span::styled(info, Style::default().fg(Color::DarkGray))];
            f.render_widget(
                Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
                bar,
            );
        })?;

        if event::poll(std::time::Duration::from_millis(120))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if should_quit(&key) {
                break;
            }
            let visible = terminal.size()?.height.saturating_sub(2);
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => scroll = scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
                KeyCode::PageDown => scroll = scroll.saturating_add(visible),
                KeyCode::PageUp => scroll = scroll.saturating_sub(visible),
                KeyCode::Char('g') => scroll = 0,
                KeyCode::Char('G') => scroll = u16::MAX,
                KeyCode::Home => scroll = 0,
                KeyCode::End => scroll = u16::MAX,
                _ => {}
            }
        }
    }

    ratatui::restore();
    Ok(())
}

fn should_quit(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}
