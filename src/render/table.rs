use unicode_width::UnicodeWidthStr;

use super::block::{inline_format, match_inline, strip_ansi, truncate, visible_width};

pub const TBL_NONE: i32 = 0;
pub const TBL_HEADER: i32 = 1;
pub const TBL_BODY: i32 = 2;

pub fn is_table_line(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return false;
    }
    if is_horizontal_rule(trimmed) {
        return false;
    }
    trimmed.matches('|').count() >= 2
}

pub fn is_table_delimiter(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.contains('|') && is_table_separator(trimmed)
}

pub fn looks_like_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.contains('|')
}

pub fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim().trim_matches(|c| c == '|' || c == ' ');
    if trimmed.is_empty() {
        return false;
    }
    for part in trimmed.split('|') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        for c in part.chars() {
            if c != '-' && c != ':' {
                return false;
            }
        }
    }
    true
}

pub fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.chars().next().unwrap();
    if !matches!(first, '-' | '*' | '_') {
        return false;
    }
    trimmed.chars().all(|c| c == first)
}

pub fn parse_table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim().trim_matches('|').trim();
    if trimmed.is_empty() {
        return vec![];
    }
    trimmed.split('|').map(|p| p.trim().to_string()).collect()
}

pub fn parse_alignments(line: &str) -> Vec<i32> {
    let trimmed = line.trim().trim_matches('|').trim();
    trimmed
        .split('|')
        .map(|p| {
            let p = p.trim();
            let left = p.starts_with(':');
            let right = p.ends_with(':');
            if left && right {
                1
            } else if right {
                2
            } else {
                0
            }
        })
        .collect()
}

pub fn render_table(
    header: &[String],
    align: &[i32],
    rows: &[Vec<String>],
    width: usize,
) -> String {
    if header.is_empty() {
        return String::new();
    }

    let ncols = header.len();
    let mut align = align.to_vec();
    if align.len() < ncols {
        align.resize(ncols, 0);
    } else if align.len() > ncols {
        align.truncate(ncols);
    }

    let mut rows = rows.to_vec();
    for row in &mut rows {
        if row.len() > ncols {
            row.truncate(ncols);
        } else {
            while row.len() < ncols {
                row.push(String::new());
            }
        }
    }

    let mut col_widths: Vec<usize> = vec![0; ncols];
    for (i, h) in header.iter().enumerate() {
        col_widths[i] = visible_width(&inline_format(h));
    }
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            let w = visible_width(&inline_format(cell));
            if w > col_widths[i] {
                col_widths[i] = w;
            }
        }
    }

    for w in &mut col_widths {
        if *w < 3 {
            *w = 3;
        }
    }

    let available = if width == 0 { 80 } else { width }.max(20);
    if ncols == 1 {
        col_widths[0] = (available.saturating_sub(4)).max(3);
    } else {
        let border_overhead = 1 + 3 * ncols;
        let target_total = available.saturating_sub(border_overhead);

        let total_content: usize = col_widths.iter().sum();
        if total_content > 0 && target_total >= 3 * ncols {
            let mut largest = 0;
            for i in 0..ncols {
                col_widths[i] = col_widths[i] * target_total / total_content;
                if col_widths[i] < 3 {
                    col_widths[i] = 3;
                }
                if col_widths[i] > col_widths[largest] {
                    largest = i;
                }
            }
            let distributed: usize = col_widths.iter().sum();
            let diff = target_total as isize - distributed as isize;
            if diff != 0 {
                col_widths[largest] = (col_widths[largest] as isize + diff).max(3) as usize;
            }
        }
    }

    let mut out = String::new();

    let top_div = build_divider(&col_widths, "┌", "┬", "┐");
    let mid_div = build_divider(&col_widths, "├", "┼", "┤");
    let bot_div = build_divider(&col_widths, "└", "┴", "┘");

    out.push_str(&top_div);
    out.push('\n');

    let header_lines = wrap_cells(header, &col_widths);
    write_row_lines(&mut out, &header_lines, &col_widths, &align, true);
    out.push_str(&mid_div);
    out.push('\n');

    for row in &rows {
        let cell_lines = wrap_cells(row, &col_widths);
        write_row_lines(&mut out, &cell_lines, &col_widths, &align, false);
    }
    out.push_str(&bot_div);

    out
}

fn build_divider(widths: &[usize], left: &str, cross: &str, right: &str) -> String {
    let parts: Vec<String> = widths.iter().map(|w| "─".repeat(w + 2)).collect();
    let border_color = super::block::table_border_style();
    let reset = super::block::reset_ansi();
    format!(
        "{}{}{}{}{}",
        border_color,
        left,
        parts.join(&format!("{reset}{border_color}{cross}{border_color}")),
        right,
        reset
    )
}

fn write_row_lines(
    out: &mut String,
    cells: &[Vec<String>],
    widths: &[usize],
    align: &[i32],
    is_header: bool,
) {
    let nlines = cells.iter().map(|c| c.len()).max().unwrap_or(0);
    let border_color = super::block::table_border_style();
    let reset = super::block::reset_ansi();

    for li in 0..nlines {
        let line_cells: Vec<String> = cells
            .iter()
            .map(|c| {
                if li < c.len() {
                    c[li].clone()
                } else {
                    String::new()
                }
            })
            .collect();

        let header_style = super::block::table_header_style();

        let parts: Vec<String> = widths
            .iter()
            .enumerate()
            .map(|(i, &w)| {
                let mut cell = if i < line_cells.len() {
                    line_cells[i].trim().to_string()
                } else {
                    String::new()
                };
                if is_header {
                    cell = format!("{}{}{}", header_style, cell, reset);
                }
                pad_cell(&cell, w, align, i)
            })
            .collect();

        let sep = format!("{reset}{border_color}│{reset}");
        out.push_str(&format!(
            "{border_color}│{reset} {}{} {} {border_color}│{reset}\n",
            parts.join(&format!(" {} ", sep)),
            border_color,
            reset
        ));
    }
}

fn pad_cell(cell: &str, width: usize, align: &[i32], col: usize) -> String {
    let content_w = visible_width(cell);
    let al = if col < align.len() { align[col] } else { 0 };

    if content_w > width {
        return truncate(cell, width, "");
    }

    let remaining = width - content_w;
    match al {
        1 => {
            let left = remaining / 2;
            let right = remaining - left;
            format!("{}{}{}", " ".repeat(left), cell, " ".repeat(right))
        }
        2 => format!("{}{}", " ".repeat(remaining), cell),
        _ => format!("{}{}", cell, " ".repeat(remaining)),
    }
}

fn wrap_cells(cells: &[String], widths: &[usize]) -> Vec<Vec<String>> {
    cells
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let cell = cell.trim().to_string();
            if cell.is_empty() {
                vec![String::new()]
            } else {
                wrap_cell_text(&cell, widths[i])
            }
        })
        .collect()
}

pub fn wrap_cell_text(text: &str, width: usize) -> Vec<String> {
    let mut segments: Vec<(String, usize)> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let byte_pos: usize = chars[..i].iter().map(|c| c.len_utf8()).sum();
        match match_inline(text, byte_pos, true) {
            Some(m) => {
                if !current.is_empty() {
                    let w = UnicodeWidthStr::width(current.as_str());
                    segments.push((current.clone(), w));
                    current.clear();
                }
                let seg_text = &text[byte_pos..m.end];
                segments.push((seg_text.to_string(), 0));
                i = text[..m.end].chars().count();
            }
            None => {
                current.push(chars[i]);
                i += 1;
            }
        }
    }
    if !current.is_empty() {
        let w = UnicodeWidthStr::width(current.as_str());
        segments.push((current, w));
    }

    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut line_width = 0usize;

    for (seg, _seg_w) in &segments {
        let formatted = inline_format(seg);
        let seg_width = UnicodeWidthStr::width(strip_ansi(&formatted).as_str());
        let is_atomic =
            !seg.is_empty() && matches!(seg.chars().next().unwrap(), '`' | '*' | '~' | '[');

        if is_atomic {
            if seg_width <= width {
                if line_width > 0 && line_width + seg_width + 1 > width {
                    lines.push(line.trim_end().to_string());
                    line.clear();
                    line_width = 0;
                }
                if !line.is_empty() {
                    line.push(' ');
                    line_width += 1;
                }
                line.push_str(&formatted);
                line_width += seg_width;
            } else {
                if !line.is_empty() {
                    lines.push(line.trim_end().to_string());
                    line.clear();
                    line_width = 0;
                }
                if let Some(m) = match_inline(seg, 0, false) {
                    let wrapped = wrap_inner_text(&m.inner, width);
                    for wi in &wrapped {
                        let rewrapped = rewrap_inline(seg, wi);
                        lines.push(inline_format(&rewrapped));
                    }
                } else {
                    for chunk in char_wrap(seg, width) {
                        lines.push(inline_format(&chunk));
                    }
                }
            }
        } else {
            for word in seg.split(' ') {
                let word_formatted = inline_format(word);
                let word_width = UnicodeWidthStr::width(strip_ansi(&word_formatted).as_str());

                if word_width > width {
                    if !line.is_empty() {
                        lines.push(line.trim_end().to_string());
                        line.clear();
                        line_width = 0;
                    }
                    for chunk in char_wrap(word, width) {
                        lines.push(inline_format(&chunk));
                    }
                    continue;
                }

                if line_width > 0 && line_width + word_width + 1 > width {
                    lines.push(line.trim_end().to_string());
                    line.clear();
                    line_width = 0;
                }
                if !line.is_empty() {
                    line.push(' ');
                    line_width += 1;
                }
                line.push_str(&word_formatted);
                line_width += word_width;
            }
        }
    }

    if !line.is_empty() {
        lines.push(line.trim_end().to_string());
    }

    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn char_wrap(s: &str, width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut remain = s;
    loop {
        let mut w = 0usize;
        let mut idx = 0usize;
        for c in remain.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if w + cw > width {
                break;
            }
            w += cw;
            idx += c.len_utf8();
        }
        if idx == 0 {
            break;
        }
        chunks.push(remain[..idx].to_string());
        remain = &remain[idx..];
    }
    if !remain.is_empty() {
        chunks.push(remain.to_string());
    }
    chunks
}

fn wrap_inner_text(inner: &str, width: usize) -> Vec<String> {
    let words: Vec<&str> = inner.split(' ').collect();
    if words.is_empty() {
        return vec![inner.to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut line_width = 0usize;

    for w in &words {
        let w_len = w.chars().count();
        if w_len > width {
            if !line.is_empty() {
                lines.push(line.trim_end().to_string());
                line.clear();
                line_width = 0;
            }
            for chunk in char_wrap(w, width) {
                lines.push(chunk);
            }
            continue;
        }
        if line_width > 0 && line_width + w_len + 1 > width {
            lines.push(line.trim_end().to_string());
            line.clear();
            line_width = 0;
        }
        if !line.is_empty() {
            line.push(' ');
            line_width += 1;
        }
        line.push_str(w);
        line_width += w_len;
    }
    if !line.is_empty() {
        lines.push(line.trim_end().to_string());
    }
    if lines.is_empty() {
        vec![inner.to_string()]
    } else {
        lines
    }
}

fn rewrap_inline(original: &str, inner: &str) -> String {
    if let Some(_m) = match_inline(original, 0, false) {
        match original.chars().next().unwrap_or(' ') {
            '`' => {
                let n = original.chars().take_while(|&c| c == '`').count();
                let ticks = "`".repeat(n);
                format!("{}{}{}", ticks, inner, ticks)
            }
            '*' => {
                if original.starts_with("***") {
                    format!("***{}***", inner)
                } else if original.starts_with("**") {
                    format!("**{}**", inner)
                } else {
                    format!("*{}*", inner)
                }
            }
            '~' => format!("~~{}~~", inner),
            '[' => {
                if let Some(cb) = original[1..].find(']') {
                    let close_b = cb + 1;
                    if close_b + 1 < original.len() && original.as_bytes()[close_b + 1] == b'(' {
                        if let Some(cp) = original[close_b + 2..].find(')') {
                            let url = &original[close_b + 2..close_b + 2 + cp];
                            return format!("[{}]({})", inner, url);
                        }
                    }
                }
                inline_format(inner)
            }
            _ => inline_format(inner),
        }
    } else {
        inline_format(inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_table_line() {
        assert!(is_table_line("| a | b |"));
        assert!(is_table_line("| a | b"));
        assert!(!is_table_line("| single"));
        assert!(!is_table_line("not a table"));
        assert!(is_table_line("|---|"));
        assert!(!is_table_line(""));
    }

    #[test]
    fn test_is_table_separator() {
        assert!(is_table_separator("|------|-----|"));
        assert!(is_table_separator("|---|"));
        assert!(is_table_separator("|:---|:--:|"));
        assert!(!is_table_separator("| a | b |"));
        assert!(!is_table_separator(""));
    }

    #[test]
    fn test_is_table_delimiter() {
        assert!(is_table_delimiter("| --- | --- |"));
        assert!(is_table_delimiter("--- | ---"));
        assert!(is_table_delimiter(":--: | :-:"));
        assert!(!is_table_delimiter("---"));
        assert!(!is_table_delimiter("a | b"));
        assert!(!is_table_delimiter(""));
    }

    #[test]
    fn test_looks_like_table_row() {
        assert!(looks_like_table_row("foo | bar"));
        assert!(looks_like_table_row("| a | b |"));
        assert!(!looks_like_table_row("no pipe here"));
        assert!(!looks_like_table_row(""));
    }

    #[test]
    fn test_parse_table_cells() {
        let cells = parse_table_cells("| a | b | c |");
        assert_eq!(cells, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_table_cells_no_trailing_pipe() {
        let cells = parse_table_cells("| a | b");
        assert_eq!(cells, vec!["a", "b"]);
    }

    #[test]
    fn test_parse_alignments() {
        let align = parse_alignments("|:---|:--:|--:|");
        assert_eq!(align, vec![0, 1, 2]);
    }

    #[test]
    fn test_pad_cell() {
        assert_eq!(pad_cell("hi", 5, &[0], 0), "hi   ");
        assert_eq!(pad_cell("hi", 5, &[1], 0), " hi  ");
        assert_eq!(pad_cell("hi", 5, &[2], 0), "   hi");
        assert_eq!(pad_cell("hello", 3, &[0], 0), "hel");
    }

    #[test]
    fn test_render_table_basic() {
        let header = vec!["Name".to_string(), "Age".to_string()];
        let rows = vec![
            vec!["Bob".to_string(), "30".to_string()],
            vec!["Alice".to_string(), "25".to_string()],
        ];
        let result = render_table(&header, &[0, 0], &rows, 80);
        let plain = strip_ansi(&result);
        assert!(plain.contains("Name"));
        assert!(plain.contains("Bob"));
        assert!(plain.contains("│"));
    }

    #[test]
    fn test_render_table_empty() {
        let result = render_table(&[], &[], &[], 80);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rewrap_inline_backtick_count() {
        // double-backtick content containing a single backtick must preserve
        // its delimiter count when rewrapped across lines
        let got = rewrap_inline("``the `x` long``", "the `x` long");
        assert_eq!(got, "``the `x` long``");
        let got = rewrap_inline("`plain`", "plain");
        assert_eq!(got, "`plain`");
    }

    #[test]
    fn test_render_table_body_text_not_border_colored() {
        let header = vec!["h".to_string()];
        let rows = vec![vec!["the `bar` value".to_string()]];
        let result = render_table(&header, &[0], &rows, 40);
        let text = crate::render::block::ansi_to_text(&result);
        for line in &text.lines {
            for span in &line.spans {
                if span.content.chars().any(|c| c.is_alphanumeric()) {
                    if let Some(ratatui::style::Color::Rgb(r, g, b)) = span.style.fg {
                        assert!(
                            !(r == 52 && g == 52 && b == 52),
                            "body text leaked border color: {:?}",
                            span.content
                        );
                    }
                }
            }
        }
    }
}
