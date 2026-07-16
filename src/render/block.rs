use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use unicode_width::UnicodeWidthStr;

use super::theme::VESPER;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineKind {
    Plain,
    Code,
    BoldItalic,
    Bold,
    Italic,
    Strike,
    Link,
}

pub struct InlineMatch {
    pub kind: InlineKind,
    pub end: usize,
    pub inner: String,
}

fn is_boundary(text: &str, i: isize) -> bool {
    if i < 0 || i as usize >= text.len() {
        return true;
    }
    matches!(text.as_bytes()[i as usize], b' ' | b'\t' | b'\n')
}

pub fn match_inline(text: &str, i: usize, check_boundaries: bool) -> Option<InlineMatch> {
    if i >= text.len() {
        return None;
    }
    let bytes = text.as_bytes();

    if bytes[i] == b'`' {
        if let Some(idx) = text[i + 1..].find('`') {
            let end = i + idx + 2;
            let inner = &text[i..end];
            if check_boundaries
                && (!is_boundary(text, i as isize - 1) || !is_boundary(text, end as isize))
            {
                return None;
            }
            return Some(InlineMatch {
                kind: InlineKind::Code,
                end,
                inner: inner.to_string(),
            });
        }
    }

    if i + 2 < text.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*' {
        if let Some(idx) = text[i + 3..].find("***") {
            let end = i + idx + 6;
            let inner = &text[i + 3..i + 3 + idx];
            if check_boundaries
                && (!is_boundary(text, i as isize - 1) || !is_boundary(text, end as isize))
            {
                return None;
            }
            return Some(InlineMatch {
                kind: InlineKind::BoldItalic,
                end,
                inner: inner.to_string(),
            });
        }
    }

    if i + 1 < text.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' {
        if let Some(idx) = text[i + 2..].find("**") {
            let end = i + idx + 4;
            let inner = &text[i + 2..i + 2 + idx];
            if check_boundaries
                && (!is_boundary(text, i as isize - 1) || !is_boundary(text, end as isize))
            {
                return None;
            }
            return Some(InlineMatch {
                kind: InlineKind::Bold,
                end,
                inner: inner.to_string(),
            });
        }
    }

    if bytes[i] == b'*' {
        if let Some(idx) = text[i + 1..].find('*') {
            let end = i + idx + 2;
            let inner = &text[i + 1..i + 1 + idx];
            if check_boundaries
                && (!is_boundary(text, i as isize - 1) || !is_boundary(text, end as isize))
            {
                return None;
            }
            return Some(InlineMatch {
                kind: InlineKind::Italic,
                end,
                inner: inner.to_string(),
            });
        }
    }

    if i + 1 < text.len() && bytes[i] == b'~' && bytes[i + 1] == b'~' {
        if let Some(idx) = text[i + 2..].find("~~") {
            let end = i + idx + 4;
            let inner = &text[i + 2..i + 2 + idx];
            if check_boundaries
                && (!is_boundary(text, i as isize - 1) || !is_boundary(text, end as isize))
            {
                return None;
            }
            return Some(InlineMatch {
                kind: InlineKind::Strike,
                end,
                inner: inner.to_string(),
            });
        }
    }

    if bytes[i] == b'[' {
        if let Some(close_b) = text[i + 1..].find(']') {
            let close_paren_pos = i + close_b + 2;
            if close_paren_pos < text.len() && bytes[close_paren_pos] == b'(' {
                if let Some(close_p) = text[close_paren_pos + 1..].find(')') {
                    let end = close_paren_pos + close_p + 2;
                    let inner = &text[i + 1..i + 1 + close_b];
                    if check_boundaries
                        && (!is_boundary(text, i as isize - 1) || !is_boundary(text, end as isize))
                    {
                        return None;
                    }
                    return Some(InlineMatch {
                        kind: InlineKind::Link,
                        end,
                        inner: inner.to_string(),
                    });
                }
            }
        }
    }

    None
}

fn ansi_style(
    text: &str,
    fg: Option<&str>,
    bold: bool,
    italic: bool,
    strikethrough: bool,
    underline: bool,
) -> String {
    let mut codes: Vec<String> = Vec::new();
    if bold {
        codes.push("1".to_string());
    }
    if italic {
        codes.push("3".to_string());
    }
    if strikethrough {
        codes.push("9".to_string());
    }
    if underline {
        codes.push("4".to_string());
    }
    if let Some(hex) = fg {
        let (r, g, b) = hex_to_rgb(hex);
        codes.push(format!("38;2;{r};{g};{b}"));
    }
    if codes.is_empty() {
        return text.to_string();
    }
    format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text)
}

pub fn hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    (r, g, b)
}

pub fn render_inline(kind: InlineKind, inner: &str) -> String {
    match kind {
        InlineKind::Code => {
            let inner = ansi_style(inner, Some(VESPER.string), false, false, false, false);
            format!("`{}`", inner)
        }
        InlineKind::BoldItalic => ansi_style(inner, None, true, true, false, false),
        InlineKind::Bold => ansi_style(inner, None, true, false, false, false),
        InlineKind::Italic => ansi_style(inner, None, false, true, false, false),
        InlineKind::Strike => ansi_style(inner, Some(VESPER.muted), false, false, true, false),
        InlineKind::Link => ansi_style(inner, Some(VESPER.accent), false, false, false, true),
        InlineKind::Plain => inner.to_string(),
    }
}

pub fn inline_format(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let byte_pos = chars[..i].iter().map(|c| c.len_utf8()).sum::<usize>();
        match match_inline(text, byte_pos, false) {
            Some(m) => {
                out.push_str(&render_inline(m.kind, &m.inner));
                let end_chars: usize = text[..m.end].chars().count();
                i = end_chars;
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

pub fn styled(text: &str, fg: Option<&str>, bold: bool, italic: bool) -> String {
    ansi_style(text, fg, bold, italic, false, false)
}

pub fn header_style() -> (Option<String>, bool, bool) {
    (Some(VESPER.typ.to_string()), true, false)
}

pub fn bullet_style() -> (Option<String>, bool, bool) {
    (Some(VESPER.muted.to_string()), false, false)
}

pub fn number_style() -> (Option<String>, bool, bool) {
    (Some(VESPER.number.to_string()), false, false)
}

pub fn blockquote_style() -> (Option<String>, bool, bool) {
    (Some(VESPER.muted.to_string()), false, false)
}

pub fn rule_style() -> (Option<String>, bool, bool) {
    (Some(VESPER.border.to_string()), false, false)
}

pub fn task_done_style() -> (Option<String>, bool, bool) {
    (Some(VESPER.string.to_string()), false, false)
}

pub fn task_todo_style() -> (Option<String>, bool, bool) {
    (Some(VESPER.muted.to_string()), false, false)
}

pub fn table_border_style() -> String {
    format!(
        "\x1b[38;2;{};{};{}m",
        hex_to_rgb(VESPER.border).0,
        hex_to_rgb(VESPER.border).1,
        hex_to_rgb(VESPER.border).2
    )
}

pub fn table_header_style() -> String {
    format!(
        "\x1b[1;38;2;{};{};{}m",
        hex_to_rgb(VESPER.fg).0,
        hex_to_rgb(VESPER.fg).1,
        hex_to_rgb(VESPER.fg).2
    )
}

pub fn reset_ansi() -> &'static str {
    "\x1b[0m"
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some(&'[') = chars.peek() {
                chars.next();
                in_escape = true;
                continue;
            }
        }
        if in_escape {
            if c == 'm' {
                in_escape = false;
            }
            continue;
        }
        out.push(c);
    }
    out
}

pub fn visible_width(s: &str) -> usize {
    UnicodeWidthStr::width(strip_ansi(s).as_str())
}

pub fn wordwrap(text: &str, width: usize, continuation: &str) -> String {
    if width == 0 {
        return text.to_string();
    }
    let plain = strip_ansi(text);
    let mut result = String::with_capacity(text.len() * 2);
    let mut line_width = 0usize;
    let mut first_word = true;

    for word in plain.split(' ') {
        let w_width = unicode_width::UnicodeWidthStr::width(word);

        if !first_word && line_width + 1 + w_width > width {
            result.push('\n');
            result.push_str(continuation);
            line_width = unicode_width::UnicodeWidthStr::width(continuation);
            first_word = true;
        }

        if !first_word {
            result.push(' ');
            line_width += 1;
        }
        result.push_str(word);
        line_width += w_width;
        first_word = false;
    }

    result
}

pub fn truncate(s: &str, width: usize, ellipsis: &str) -> String {
    if visible_width(s) <= width {
        return s.to_string();
    }
    let plain = strip_ansi(s);
    let ell_width = unicode_width::UnicodeWidthStr::width(ellipsis);
    let max_w = width.saturating_sub(ell_width);
    let mut result = String::new();
    let mut w = 0usize;
    for c in plain.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw > max_w {
            break;
        }
        result.push(c);
        w += cw;
    }
    result.push_str(ellipsis);
    result
}

pub fn ansi_to_text(s: &str) -> Text<'static> {
    let mut lines: Vec<Line> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut current = String::new();
    let mut fg: Option<Color> = None;
    let mut bg: Option<Color> = None;
    let mut bold = false;
    let mut italic = false;
    let mut underline = false;
    let mut strikethrough = false;

    let flush = |current: &mut String,
                 spans: &mut Vec<Span>,
                 fg: Option<Color>,
                 bg: Option<Color>,
                 bold: bool,
                 italic: bool,
                 underline: bool,
                 strikethrough: bool| {
        if !current.is_empty() {
            let mut style = Style::default();
            if let Some(c) = fg {
                style = style.fg(c);
            }
            if let Some(c) = bg {
                style = style.bg(c);
            }
            if bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            if italic {
                style = style.add_modifier(Modifier::ITALIC);
            }
            if underline {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if strikethrough {
                style = style.add_modifier(Modifier::CROSSED_OUT);
            }
            spans.push(Span::styled(std::mem::take(current), style));
        }
    };

    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\x1b' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            flush(
                &mut current,
                &mut spans,
                fg,
                bg,
                bold,
                italic,
                underline,
                strikethrough,
            );
            i += 2;
            let mut code_str = String::new();
            while i < bytes.len() && bytes[i] != b'm' {
                code_str.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() {
                i += 1; // skip 'm'
            }
            if code_str == "0" {
                fg = None;
                bg = None;
                bold = false;
                italic = false;
                underline = false;
                strikethrough = false;
            } else {
                let codes: Vec<&str> = code_str.split(';').collect();
                let mut ci = 0;
                while ci < codes.len() {
                    match codes[ci] {
                        "1" => bold = true,
                        "3" => italic = true,
                        "4" => underline = true,
                        "9" => strikethrough = true,
                        "38" => {
                            if ci + 4 < codes.len() && codes[ci + 1] == "2" {
                                let r: u8 = codes[ci + 2].parse().unwrap_or(0);
                                let g: u8 = codes[ci + 3].parse().unwrap_or(0);
                                let b: u8 = codes[ci + 4].parse().unwrap_or(0);
                                fg = Some(Color::from_u32(
                                    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
                                ));
                                ci += 5;
                                continue;
                            }
                        }
                        "48" if ci + 4 < codes.len() && codes[ci + 1] == "2" => {
                            let r: u8 = codes[ci + 2].parse().unwrap_or(0);
                            let g: u8 = codes[ci + 3].parse().unwrap_or(0);
                            let b: u8 = codes[ci + 4].parse().unwrap_or(0);
                            bg = Some(Color::from_u32(
                                ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
                            ));
                            ci += 5;
                            continue;
                        }
                        _ => {}
                    }
                    ci += 1;
                }
            }
            continue;
        }
        if bytes[i] == b'\n' {
            flush(
                &mut current,
                &mut spans,
                fg,
                bg,
                bold,
                italic,
                underline,
                strikethrough,
            );
            lines.push(Line::from(std::mem::take(&mut spans)));
            spans.clear();
            i += 1;
            continue;
        }
        let c = s[i..].chars().next().unwrap_or(' ');
        current.push(c);
        i += c.len_utf8();
    }
    flush(
        &mut current,
        &mut spans,
        fg,
        bg,
        bold,
        italic,
        underline,
        strikethrough,
    );
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    if lines.is_empty() {
        lines.push(Line::from(""));
    }
    Text::from(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_format_bold() {
        let out = inline_format("this is **bold** text");
        let plain = strip_ansi(&out);
        assert_eq!(plain, "this is bold text");
    }

    #[test]
    fn test_inline_format_italic() {
        let out = inline_format("this is *italic* text");
        let plain = strip_ansi(&out);
        assert_eq!(plain, "this is italic text");
    }

    #[test]
    fn test_inline_format_bold_italic() {
        let out = inline_format("this is ***both*** text");
        let plain = strip_ansi(&out);
        assert_eq!(plain, "this is both text");
    }

    #[test]
    fn test_inline_format_code() {
        let out = inline_format("use `fmt.Println()` to print");
        let plain = strip_ansi(&out);
        assert!(plain.contains("fmt.Println()"));
    }

    #[test]
    fn test_inline_format_strikethrough() {
        let out = inline_format("this is ~~struck~~ text");
        let plain = strip_ansi(&out);
        assert_eq!(plain, "this is struck text");
    }

    #[test]
    fn test_inline_format_link() {
        let out = inline_format("visit [example](https://example.com) today");
        let plain = strip_ansi(&out);
        assert!(plain.contains("example"));
        assert!(!plain.contains("https://example.com"));
    }

    #[test]
    fn test_inline_format_boundaries() {
        // match_inline with check_boundaries=false does NOT require boundaries,
        // matching Go behavior where inlineFormat passes false
        let out = inline_format("this is**not**bold");
        let plain = strip_ansi(&out);
        assert_eq!(plain, "this isnotbold");
    }

    #[test]
    fn test_strip_ansi() {
        let ansi = ansi_style("hello", Some("#FF0000"), true, false, false, false);
        assert_eq!(strip_ansi(&ansi), "hello");
    }
}
