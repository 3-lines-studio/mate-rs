use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::chat_format::{format_tool_label, result_lang, TOOL_COLOR};
use crate::render::highlight;
use crate::tui::theme::COLORS;

const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

pub fn thinking_indicator(
    ticks: usize,
    label: &str,
    elapsed: std::time::Duration,
) -> Line<'static> {
    let frame = SPINNER[ticks % SPINNER.len()];
    Line::from(vec![
        Span::styled(frame.to_string(), Style::default().fg(COLORS.accent)),
        Span::raw(" "),
        Span::styled(label.to_string(), Style::default().fg(COLORS.accent)),
        Span::styled(
            format!(" ({:.0?})", elapsed),
            Style::default().fg(COLORS.placeholder),
        ),
    ])
}

#[allow(clippy::too_many_arguments)]
pub fn render_tool_block(
    name: &str,
    args: &str,
    result: &str,
    error: &str,
    duration: &str,
    cwd: &str,
    subagent: &str,
    collapsed: bool,
    width: usize,
    indent: usize,
) -> String {
    let mut out = String::new();
    let prefix = " ".repeat(indent);
    let field_prefix = " ".repeat(indent + 2);

    let label = format_tool_label(cwd, name, args);
    let label = if label.is_empty() {
        name.to_string()
    } else {
        label
    };
    let is_running = result.is_empty() && error.is_empty() && duration.is_empty();

    let symbol = if is_running {
        "○"
    } else if !error.is_empty() {
        "✗"
    } else {
        ""
    };

    let color = TOOL_COLOR;
    let (r, g, b) = crate::render::block::hex_to_rgb(color);
    let label_style = format!("\x1b[38;2;{r};{g};{b}m");
    let reset = "\x1b[0m";

    let mut name_str = format!("{}{}{}", label_style, label, reset);
    if !subagent.is_empty() {
        name_str = format!("[{}] {}", subagent, name_str);
    }

    out.push_str(&prefix);
    if !symbol.is_empty() {
        out.push_str(symbol);
        out.push(' ');
    }
    out.push_str(&name_str);

    if is_running {
        out.push_str(" running");
    } else if error.is_empty() && !duration.is_empty() {
        out.push(' ');
        out.push_str(duration);
    }

    if collapsed {
        return out;
    }

    if !result.is_empty() {
        out.push('\n');
        let lang = result_lang(name, args);
        let rendered = if !lang.is_empty() {
            highlight::highlight(&lang, result)
        } else if let Ok(obj) = serde_json::from_str::<serde_json::Value>(result) {
            if let Ok(pretty) = serde_json::to_string_pretty(&obj) {
                highlight::highlight("json", &pretty)
            } else {
                result.to_string()
            }
        } else {
            result.to_string()
        };

        let lines: Vec<&str> = rendered.lines().collect();
        let max_lines = 50;
        let display_lines: Vec<String> = if lines.len() > max_lines {
            let trunc_msg = format!("... (truncated, {} more lines)", lines.len() - max_lines);
            let mut truncated: Vec<String> =
                lines[..max_lines].iter().map(|s| s.to_string()).collect();
            truncated.push(trunc_msg);
            truncated
        } else {
            lines.iter().map(|s| s.to_string()).collect()
        };

        for line in display_lines {
            let truncated =
                crate::render::block::truncate(&line, width.saturating_sub(indent + 2), "…");
            out.push_str(&field_prefix);
            out.push_str(&truncated);
            out.push('\n');
        }
        out = out.trim_end().to_string();
    }

    if !error.is_empty() {
        out.push('\n');
        out.push_str(&field_prefix);
        out.push_str(error);
    }

    out
}

pub fn git_branch(cwd: &str) -> Option<String> {
    let mut dir: &std::path::Path = std::path::Path::new(cwd);
    loop {
        let head = dir.join(".git").join("HEAD");
        if let Ok(content) = std::fs::read_to_string(&head) {
            let trimmed = content.trim();
            if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
                return Some(branch.to_string());
            }
            if trimmed.len() >= 7 {
                return Some(trimmed[..7].to_string());
            }
            return Some(trimmed.to_string());
        }
        dir = dir.parent()?;
    }
}

fn shorten_cwd(cwd: &str) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy();
        if cwd.starts_with(&*home) {
            return format!("~/{}", &cwd[home.len()..]);
        }
    }
    cwd.to_string()
}

pub fn render_top_bar(f: &mut Frame, area: Rect, cwd: &str, model: &str) {
    let gray = COLORS.placeholder;
    let bright = COLORS.muted;

    let mut spans = Vec::new();
    if let Some(branch) = git_branch(cwd) {
        spans.push(Span::styled(
            format!(" {branch}"),
            Style::default().fg(bright),
        ));
    }
    let display_cwd = shorten_cwd(cwd);
    spans.push(Span::styled(
        format!("  {display_cwd}"),
        Style::default().fg(gray),
    ));

    if !model.is_empty() {
        let left_w: usize = spans.iter().map(|s| s.width()).sum();
        let model_text = format!(" {model}");
        let model_w = model_text.len();
        let pad = (area.width as usize)
            .saturating_sub(left_w)
            .saturating_sub(model_w);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(model_text, Style::default().fg(gray)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub fn render_shortcuts_bar(f: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    let key_style = Style::default()
        .fg(COLORS.muted)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(COLORS.placeholder);
    let sep = Span::styled("  │  ", Style::default().fg(COLORS.placeholder));

    let mut spans = Vec::new();
    for (i, (key, label)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(sep.clone());
        }
        spans.push(Span::styled(format!(" {key} "), key_style));
        spans.push(Span::styled(*label, label_style));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
