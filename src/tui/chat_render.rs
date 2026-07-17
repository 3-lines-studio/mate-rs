use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::chat_format::{format_tool_label, result_lang, TOOL_COLOR};
use crate::render::highlight;
use crate::tui::theme::COLORS;

const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

const RAINBOW: [Color; 6] = [
    Color::Rgb(0xFF, 0x6B, 0x6B),
    Color::Rgb(0xFF, 0xA9, 0x4D),
    Color::Rgb(0xFF, 0xD4, 0x3B),
    Color::Rgb(0x69, 0xDB, 0x7C),
    Color::Rgb(0x4D, 0xAB, 0xF7),
    Color::Rgb(0x97, 0x75, 0xFA),
];

pub fn thinking_indicator(
    ticks: usize,
    label: &str,
    elapsed: std::time::Duration,
) -> Line<'static> {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, ch) in label.chars().enumerate() {
        let color = RAINBOW[(i + ticks) % RAINBOW.len()];
        spans.push(Span::styled(ch.to_string(), bold.fg(color)));
    }
    spans.push(Span::styled(
        format!(" ({})", fmt_duration(elapsed)),
        Style::default().fg(COLORS.placeholder),
    ));
    Line::from(spans)
}

fn fmt_duration(d: std::time::Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{}h{}m", h, m)
    } else if m > 0 {
        format!("{}m{}s", m, s)
    } else {
        format!("{}s", s)
    }
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
    ticks: usize,
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
        SPINNER[ticks % SPINNER.len()]
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
        let rendered = if name == "grep" {
            format_grep_result(result, width, indent)
        } else if name == "glob" {
            format_glob_result(result, width, indent)
        } else if !lang.is_empty() {
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

pub fn format_grep_result(result: &str, width: usize, indent: usize) -> String {
    use crate::render::block::{hex_to_rgb, truncate, visible_width};
    use crate::render::theme::VESPER;

    let eff_w = width.saturating_sub(indent + 2);
    let (pr, pg, pb) = hex_to_rgb(VESPER.accent);
    let (nr, ng, nb) = hex_to_rgb(VESPER.muted);
    let path_style = format!("\x1b[1;38;2;{pr};{pg};{pb}m");
    let num_style = format!("\x1b[38;2;{nr};{ng};{nb}m");
    let reset = "\x1b[0m";

    let mut out = String::new();
    let mut cur_path: Option<&str> = None;

    for raw in result.lines() {
        let mut parts = raw.splitn(3, ':');
        let path = parts.next().unwrap_or("");
        let (Some(linenum), Some(rest)) = (parts.next(), parts.next()) else {
            out.push_str(raw);
            out.push('\n');
            continue;
        };
        let content = rest.strip_prefix(' ').unwrap_or(rest);

        if cur_path != Some(path) {
            cur_path = Some(path);
            out.push_str(&path_style);
            out.push_str(&truncate(path, eff_w, "\u{2026}"));
            out.push_str(reset);
            out.push('\n');
        }

        let avail = eff_w.saturating_sub(2 + linenum.len() + 2);
        let content = if visible_width(content) > avail {
            truncate(content, avail, "\u{2026}")
        } else {
            content.to_string()
        };
        out.push_str("  ");
        out.push_str(&num_style);
        out.push_str(linenum);
        out.push_str(reset);
        out.push_str(": ");
        out.push_str(&content);
        out.push('\n');
    }

    out.trim_end_matches('\n').to_string()
}

pub fn format_glob_result(result: &str, width: usize, indent: usize) -> String {
    use crate::render::block::{hex_to_rgb, truncate, visible_width};
    use crate::render::theme::VESPER;

    let eff_w = width.saturating_sub(indent + 2);
    let (dr, dg, db) = hex_to_rgb(VESPER.accent);
    let dir_style = format!("\x1b[1;38;2;{dr};{dg};{db}m");
    let reset = "\x1b[0m";

    let mut out = String::new();
    let mut cur_dir: Option<&str> = None;

    for raw in result.lines() {
        if raw.is_empty() {
            continue;
        }
        let (dir, file) = match raw.rfind('/') {
            Some(idx) => (&raw[..=idx], &raw[idx + 1..]),
            None => ("", raw),
        };

        if dir.is_empty() {
            cur_dir = Some("");
            let f = if visible_width(file) > eff_w {
                truncate(file, eff_w, "\u{2026}")
            } else {
                file.to_string()
            };
            out.push_str(&f);
            out.push('\n');
            continue;
        }

        if cur_dir != Some(dir) {
            cur_dir = Some(dir);
            out.push_str(&dir_style);
            out.push_str(&truncate(dir, eff_w, "\u{2026}"));
            out.push_str(reset);
            out.push('\n');
        }

        let avail = eff_w.saturating_sub(2);
        let f = if visible_width(file) > avail {
            truncate(file, avail, "\u{2026}")
        } else {
            file.to_string()
        };
        out.push_str("  ");
        out.push_str(&f);
        out.push('\n');
    }

    out.trim_end_matches('\n').to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::block::{strip_ansi, visible_width};

    #[test]
    fn test_format_grep_result_groups_by_file() {
        let input = "src/a.rs:1: foo\nsrc/a.rs:5: bar\nsrc/b.rs:2: baz";
        let out = format_grep_result(input, 80, 0);
        let plain = strip_ansi(&out);
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(lines[0], "src/a.rs");
        assert!(lines[1].starts_with("  1: foo"));
        assert!(lines[2].starts_with("  5: bar"));
        assert_eq!(lines[3], "src/b.rs");
        assert!(lines[4].starts_with("  2: baz"));
    }

    #[test]
    fn test_format_grep_result_truncates_content() {
        let long = "x".repeat(100);
        let input = format!("a.rs:1: {}", long);
        let out = format_grep_result(&input, 30, 0);
        let plain = strip_ansi(&out);
        let match_line = plain.lines().nth(1).unwrap();
        assert!(visible_width(match_line) <= 28);
        assert!(match_line.contains('\u{2026}'));
    }

    #[test]
    fn test_format_grep_result_preserves_non_grep_line() {
        let out = format_grep_result("not a grep line", 80, 0);
        assert_eq!(strip_ansi(&out), "not a grep line");
    }

    #[test]
    fn test_format_glob_result_groups_by_dir() {
        let input = "src/core/local.rs\nsrc/core/mod.rs\nsrc/render/block.rs\nREADME.md";
        let out = format_glob_result(input, 80, 0);
        let plain = strip_ansi(&out);
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(lines[0], "src/core/");
        assert_eq!(lines[1], "  local.rs");
        assert_eq!(lines[2], "  mod.rs");
        assert_eq!(lines[3], "src/render/");
        assert_eq!(lines[4], "  block.rs");
        assert_eq!(lines[5], "README.md");
    }

    #[test]
    fn test_format_glob_result_root_files_flat() {
        let out = format_glob_result("a.go\nb.go", 80, 0);
        let plain = strip_ansi(&out);
        assert_eq!(plain, "a.go\nb.go");
    }

    #[test]
    fn test_format_glob_result_truncates_filename() {
        let long = format!("src/{}.rs", "x".repeat(100));
        let out = format_glob_result(&long, 30, 0);
        let plain = strip_ansi(&out);
        let file_line = plain.lines().nth(1).unwrap();
        assert!(visible_width(file_line) <= 28);
        assert!(file_line.contains('\u{2026}'));
    }

    #[test]
    fn test_thinking_indicator_rainbow_colors_per_char() {
        let line = thinking_indicator(0, "ab", std::time::Duration::from_secs(5));
        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[0].content.as_ref(), "a");
        assert_eq!(line.spans[0].style.fg, Some(RAINBOW[0]));
        assert_eq!(line.spans[1].content.as_ref(), "b");
        assert_eq!(line.spans[1].style.fg, Some(RAINBOW[1]));
        assert!(line.spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_thinking_indicator_empty_label() {
        let line = thinking_indicator(0, "", std::time::Duration::from_secs(5));
        assert_eq!(line.spans.len(), 1);
        assert!(line.spans[0].content.contains("(5s)"));
    }

    #[test]
    fn test_thinking_indicator_offset_shifts_colors() {
        let base = thinking_indicator(0, "ab", std::time::Duration::from_secs(5));
        let shifted = thinking_indicator(1, "ab", std::time::Duration::from_secs(5));
        assert_eq!(base.spans[0].style.fg, Some(RAINBOW[0]));
        assert_eq!(shifted.spans[0].style.fg, Some(RAINBOW[1]));
        assert_eq!(base.spans[1].style.fg, shifted.spans[0].style.fg);
    }
}
