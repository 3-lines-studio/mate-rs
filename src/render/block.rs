use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    (r, g, b)
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
        let w_width = UnicodeWidthStr::width(word);

        if !first_word && line_width + 1 + w_width > width {
            result.push('\n');
            result.push_str(continuation);
            line_width = UnicodeWidthStr::width(continuation);
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
    let ell_width = UnicodeWidthStr::width(ellipsis);
    let max_w = width.saturating_sub(ell_width);
    let mut result = String::new();
    let mut w = 0usize;
    for c in plain.chars() {
        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
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
                i += 1;
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
