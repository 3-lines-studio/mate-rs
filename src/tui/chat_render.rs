use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use super::chat_format::{format_tokens, tool_color, format_tool_label, result_lang};
use crate::render::highlight;

pub fn render_status_line(
    f: &mut Frame,
    area: Rect,
    model_name: &str,
    total_tokens: i32,
    context_window: i32,
    cache_hit_tokens: i32,
    total_cost: f64,
    session_name: &str,
    has_more: bool,
) {
    let accent = Color::from_u32(0x00FFC799);
    let muted = Color::from_u32(0x006A6A6A);
    let warning = Color::from_u32(0x00FFCB8B);

    let mut spans = vec![Span::styled(model_name.to_string(), Style::default().fg(accent))];

    if context_window > 0 {
        let mut token_info = format!(
            " {}/{}",
            format_tokens(total_tokens),
            format_tokens(context_window)
        );
        if cache_hit_tokens > 0 {
            token_info.push_str(&format!(" · {} cached", format_tokens(cache_hit_tokens)));
        }
        spans.push(Span::styled(token_info, Style::default().fg(muted)));
    }

    let left_text = Text::from(Line::from(spans));

    let mut right_spans = Vec::new();
    if total_cost > 0.0 {
        right_spans.push(Span::styled(
            format!("${:.4} ", total_cost),
            Style::default().fg(muted),
        ));
    }
    right_spans.push(Span::styled(
        session_name.to_string(),
        Style::default().fg(muted),
    ));
    if has_more {
        right_spans.push(Span::styled(" [more]", Style::default().fg(warning)));
    }

    let right_text = Text::from(Line::from(right_spans));

    let left_w = left_text.width() as u16;
    let right_w = right_text.width() as u16;
    let padding = area.width.saturating_sub(left_w).saturating_sub(right_w);

    let full_line = Line::from(vec![
        Span::from(left_text.lines[0].spans[0].clone()),
        Span::from(" ".repeat(padding as usize)),
        Span::from(right_text.lines[0].spans[0].clone()),
    ]);

    f.render_widget(
        Paragraph::new(full_line).style(Style::default().fg(Color::from_u32(0x006A6A6A))),
        area,
    );
}

const RAINBOW: [u32; 6] = [0xFF6B6B, 0xFFA94D, 0xFFD43B, 0x69DB7C, 0x4DABF7, 0x9775FA];

pub fn rainbow_text(text: &str, offset: usize) -> Text<'static> {
    let spans: Vec<Span> = text
        .chars()
        .enumerate()
        .map(|(i, c)| {
            let color_idx = (i + offset) % RAINBOW.len();
            Span::styled(
                c.to_string(),
                Style::default()
                    .fg(Color::from_u32(RAINBOW[color_idx]))
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )
        })
        .collect();
    Text::from(Line::from(spans))
}

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
    let label = if label.is_empty() { name.to_string() } else { label };
    let is_running = result.is_empty() && error.is_empty() && duration.is_empty();

    let symbol = if is_running {
        "○"
    } else if !error.is_empty() {
        "✗"
    } else {
        "▸"
    };

    let color = tool_color(name);
    let (r, g, b) = crate::render::block::hex_to_rgb(color);
    let label_style = format!("\x1b[38;2;{r};{g};{b}m");
    let reset = "\x1b[0m";

    let mut name_str = format!("{}{}{}", label_style, label, reset);
    if !subagent.is_empty() {
        name_str = format!("[{}] {}", subagent, name_str);
    }

    out.push_str(&prefix);
    out.push_str(symbol);
    out.push(' ');
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

    if !args.is_empty() {
        out.push('\n');
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(args) {
            if let Some(map) = obj.as_object() {
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push('\n');
                    }
                    out.push_str(&field_prefix);
                    out.push_str(k);
                    out.push_str(&format_field_value(v, width.saturating_sub(indent + 2)));
                }
            } else {
                out.push_str(&field_prefix);
                out.push_str(args);
            }
        } else {
            out.push_str(&field_prefix);
            out.push_str(args);
        }
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
            let mut truncated: Vec<String> = lines[..max_lines].iter().map(|s| s.to_string()).collect();
            truncated.push(trunc_msg);
            truncated
        } else {
            lines.iter().map(|s| s.to_string()).collect()
        };

        for line in display_lines {
            let truncated = crate::render::block::truncate(&line, width.saturating_sub(indent + 2), "…");
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

fn format_field_value(val: &serde_json::Value, _width: usize) -> String {
    match val {
        serde_json::Value::String(s) => {
            if s.is_empty() {
                ": (empty)".to_string()
            } else {
                let display = truncate_value(s, 200);
                format!(": {}", display.replace('\n', " "))
            }
        }
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                format!(": {}", i)
            } else {
                format!(": {}", n)
            }
        }
        serde_json::Value::Bool(b) => format!(": {}", b),
        serde_json::Value::Null => ": null".to_string(),
        _ => {
            let s = serde_json::to_string(val).unwrap_or_default();
            format!(": {}", truncate_value(&s, 200))
        }
    }
}

fn truncate_value(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let safe_len = s.char_indices()
            .take_while(|(i, _)| *i < max_len)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        let safe = &s[..safe_len];
        format!("{}... ({} bytes)", safe, s.len())
    }
}
