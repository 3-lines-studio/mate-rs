pub mod block;
pub mod highlight;
pub mod table;
pub mod theme;

pub use block::strip_ansi;
use block::{header_style, inline_format, styled, truncate, visible_width, wordwrap};
use table::{
    is_horizontal_rule, is_table_line, is_table_separator, parse_alignments, parse_table_cells,
    render_table, TBL_BODY, TBL_HEADER, TBL_NONE,
};

pub use highlight::{highlight, highlight_code};
pub use theme::VESPER;

pub struct StreamRenderer {
    width: usize,
}

impl StreamRenderer {
    pub fn new(width: usize) -> Self {
        StreamRenderer { width }
    }

    fn format_paragraph(&self, para_buf: &[String]) -> String {
        if para_buf.is_empty() {
            return String::new();
        }
        let para = para_buf.join(" ");
        let formatted = inline_format(&para);
        if self.width > 0 {
            wordwrap(&formatted, self.width, "") + "\n"
        } else {
            formatted + "\n"
        }
    }

    fn format_table(&self, tbl_buf: &[String], tbl_state: i32) -> String {
        if tbl_buf.is_empty() {
            return String::new();
        }
        if tbl_state != TBL_BODY {
            let mut sb = String::new();
            for l in tbl_buf {
                sb.push_str(&inline_format(l));
                sb.push('\n');
            }
            return sb;
        }
        let header = parse_table_cells(&tbl_buf[0]);
        let mut align = Vec::new();
        let mut rows = Vec::new();
        let mut body_start = 1;
        if tbl_buf.len() > 1 && is_table_separator(&tbl_buf[1]) {
            align = parse_alignments(&tbl_buf[1]);
            body_start = 2;
        }
        for rl in &tbl_buf[body_start..] {
            rows.push(parse_table_cells(rl));
        }
        render_table(&header, &align, &rows, self.width) + "\n"
    }

    fn render_header(&self, line: &str) -> String {
        let level = line.chars().take_while(|&c| c == '#').count();
        if level > 6 || (level < line.len() && line.as_bytes().get(level) != Some(&b' ')) {
            return inline_format(line);
        }
        let text = line[level..].trim_start();
        let formatted = inline_format(text);
        let truncated = truncate(&formatted, self.width, "\u{2026}");
        let (fg, bold, italic) = header_style();
        styled(&truncated, fg.as_deref(), bold, italic)
    }

    fn wrap_list_item(&self, prefix: &str, text: &str) -> String {
        let formatted = inline_format(text);
        if self.width == 0 {
            return format!("{}{}", prefix, formatted);
        }
        let prefix_width = visible_width(prefix);
        let content_width = if self.width > prefix_width {
            self.width - prefix_width
        } else {
            1
        };
        let wrapped = wordwrap(&formatted, content_width, "");
        let lines: Vec<&str> = wrapped.split('\n').collect();
        if lines.len() <= 1 {
            return format!("{}{}", prefix, formatted);
        }
        let indent = " ".repeat(prefix_width);
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                result.push_str(&format!("{}{}", prefix, line));
            } else {
                result.push_str(&format!("{}{}", indent, line));
            }
            result.push('\n');
        }
        result.trim_end().to_string()
    }

    fn render_bullet(&self, line: &str) -> String {
        let rest = line[1..].trim_start();
        let bullet = styled(&line[..1], Some(theme::VESPER.muted), false, false);

        if let Some(stripped) = rest.strip_prefix("[ ] ") {
            let check = styled("[ ]", Some(theme::VESPER.muted), false, false);
            self.wrap_list_item(&format!("{} {} ", bullet, check), stripped)
        } else if rest.starts_with("[x] ") || rest.starts_with("[X] ") {
            let check = styled(
                &format!("[{}]", &rest[1..2]),
                Some(theme::VESPER.string),
                false,
                false,
            );
            self.wrap_list_item(&format!("{} {} ", bullet, check), &rest[4..])
        } else {
            self.wrap_list_item(&format!("{} ", bullet), rest)
        }
    }

    fn render_nested_bullet(&self, line: &str, indent: usize) -> String {
        let trimmed = line.trim_start();
        let bullet = styled(&trimmed[..1], Some(theme::VESPER.muted), false, false);
        let rest = trimmed[1..].trim_start();
        let spaces = " ".repeat(indent);
        self.wrap_list_item(&format!("{}{} ", spaces, bullet), rest)
    }

    fn render_numbered(&self, line: &str) -> String {
        let dot = line.find('.').unwrap();
        let num = styled(&line[..=dot], Some(theme::VESPER.number), false, false);
        let rest = line[dot + 2..].trim_start();
        self.wrap_list_item(&format!("{} ", num), rest)
    }

    fn render_blockquote(&self, line: &str) -> String {
        let rest = line[1..].trim_start();
        let marker = styled("|", Some(theme::VESPER.muted), false, false);
        self.wrap_list_item(&format!("{} ", marker), rest)
    }

    pub fn render(&self, text: &str) -> String {
        let lines: Vec<&str> = text.split('\n').collect();
        let mut out = String::new();
        let mut para_buf: Vec<String> = Vec::new();
        let mut code_buf: Vec<String> = Vec::new();
        let mut code_lang = String::new();
        let mut in_code = false;

        let mut tbl_buf: Vec<String> = Vec::new();
        let mut tbl_state = TBL_NONE;

        let flush_para = |para_buf: &mut Vec<String>, out: &mut String, this: &Self| {
            if !para_buf.is_empty() {
                out.push_str(&this.format_paragraph(para_buf));
                para_buf.clear();
            }
        };

        let flush_table =
            |tbl_buf: &mut Vec<String>, tbl_state: &mut i32, out: &mut String, this: &Self| {
                if !tbl_buf.is_empty() {
                    out.push_str(&this.format_table(tbl_buf, *tbl_state));
                    tbl_buf.clear();
                    *tbl_state = TBL_NONE;
                }
            };

        for line in &lines {
            if is_code_fence(line) {
                flush_para(&mut para_buf, &mut out, self);
                flush_table(&mut tbl_buf, &mut tbl_state, &mut out, self);
                if in_code {
                    code_buf.push(line.to_string());
                    out.push_str(&highlight_code(&code_buf.join("\n"), &code_lang));
                    out.push('\n');
                    code_buf.clear();
                    code_lang.clear();
                    in_code = false;
                } else {
                    in_code = true;
                    code_lang = extract_code_lang(line).to_string();
                    code_buf = vec![line.to_string()];
                }
                continue;
            }

            if in_code {
                code_buf.push(line.to_string());
                continue;
            }

            if line.is_empty() {
                flush_para(&mut para_buf, &mut out, self);
                flush_table(&mut tbl_buf, &mut tbl_state, &mut out, self);
                out.push('\n');
                continue;
            }

            if is_horizontal_rule(line) {
                flush_para(&mut para_buf, &mut out, self);
                flush_table(&mut tbl_buf, &mut tbl_state, &mut out, self);
                let rule = styled(
                    &"─".repeat(self.width),
                    Some(theme::VESPER.border),
                    false,
                    false,
                );
                out.push_str(&rule);
                out.push('\n');
                continue;
            }

            if is_header(line) {
                flush_para(&mut para_buf, &mut out, self);
                flush_table(&mut tbl_buf, &mut tbl_state, &mut out, self);
                out.push_str(&self.render_header(line));
                out.push('\n');
                continue;
            }

            if is_table_line(line) {
                flush_para(&mut para_buf, &mut out, self);
                match tbl_state {
                    TBL_NONE => {
                        tbl_buf = vec![line.to_string()];
                        tbl_state = TBL_HEADER;
                    }
                    TBL_HEADER => {
                        tbl_buf.push(line.to_string());
                        tbl_state = TBL_BODY;
                    }
                    TBL_BODY => {
                        tbl_buf.push(line.to_string());
                    }
                    _ => {}
                }
                continue;
            }

            if tbl_state != TBL_NONE {
                flush_table(&mut tbl_buf, &mut tbl_state, &mut out, self);
            }

            if is_blockquote(line) {
                flush_para(&mut para_buf, &mut out, self);
                out.push_str(&self.render_blockquote(line));
                out.push('\n');
                continue;
            }

            if let Some((indent, _)) = is_nested_bullet(line) {
                flush_para(&mut para_buf, &mut out, self);
                out.push_str(&self.render_nested_bullet(line, indent));
                out.push('\n');
                continue;
            }

            if is_numbered_list(line) {
                flush_para(&mut para_buf, &mut out, self);
                out.push_str(&self.render_numbered(line));
                out.push('\n');
                continue;
            }

            if is_bullet(line) {
                flush_para(&mut para_buf, &mut out, self);
                out.push_str(&self.render_bullet(line));
                out.push('\n');
                continue;
            }

            para_buf.push(line.to_string());
        }

        flush_para(&mut para_buf, &mut out, self);
        flush_table(&mut tbl_buf, &mut tbl_state, &mut out, self);

        if in_code && !code_buf.is_empty() {
            out.push_str(&code_buf.join("\n"));
            out.push('\n');
        }

        out = out.trim_end_matches('\n').to_string();
        out
    }
}

fn is_code_fence(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn extract_code_lang(line: &str) -> &str {
    let trimmed = line.trim();
    if trimmed.len() > 3 {
        &trimmed[3..]
    } else {
        ""
    }
}

fn is_header(line: &str) -> bool {
    line.starts_with('#')
}

fn is_blockquote(line: &str) -> bool {
    line.starts_with('>')
}

fn is_numbered_list(line: &str) -> bool {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b.is_ascii_digit() {
            continue;
        }
        return b == b'.' && i > 0 && i + 1 < bytes.len() && bytes[i + 1] == b' ';
    }
    false
}

fn is_bullet(line: &str) -> bool {
    (line.starts_with("- ") || line.starts_with("* ")) && !line.starts_with("**")
}

fn is_nested_bullet(line: &str) -> Option<(usize, bool)> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if indent == 0 {
        return None;
    }
    if trimmed.starts_with("- ") || (trimmed.starts_with("* ") && !trimmed.starts_with("**")) {
        Some((indent, true))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::block::strip_ansi;

    fn strip_ansi_str(s: &str) -> String {
        strip_ansi(s)
    }

    fn lines(s: &str) -> Vec<&str> {
        if s.is_empty() {
            return vec![];
        }
        s.split('\n').collect()
    }

    #[test]
    fn test_render_empty() {
        let r = StreamRenderer::new(80);
        let out = r.render("");
        assert_eq!(out, "");
    }

    #[test]
    fn test_render_plain_text() {
        let r = StreamRenderer::new(80);
        let out = r.render("hello world");
        assert_eq!(strip_ansi_str(&out), "hello world");
    }

    #[test]
    fn test_render_text_wrapping() {
        let r = StreamRenderer::new(20);
        let out = r.render("hello world this is a long sentence");
        let ls = lines(&out);
        assert!(ls.len() >= 2);
        for l in ls {
            let w = unicode_width::UnicodeWidthStr::width(strip_ansi_str(l).as_str());
            assert!(w <= 20, "line exceeds width 20: {l} (width {w})");
        }
    }

    #[test]
    fn test_render_paragraphs() {
        let r = StreamRenderer::new(80);
        let out = r.render("first paragraph\n\nsecond paragraph");
        let plain = strip_ansi_str(&out);
        assert!(plain.contains("first paragraph"));
        assert!(plain.contains("second paragraph"));
    }

    #[test]
    fn test_render_inline_bold() {
        let r = StreamRenderer::new(80);
        let out = r.render("this is **bold** text");
        let plain = strip_ansi_str(&out);
        assert_eq!(plain, "this is bold text");
    }

    #[test]
    fn test_render_inline_italic() {
        let r = StreamRenderer::new(80);
        let out = r.render("this is *italic* text");
        let plain = strip_ansi_str(&out);
        assert_eq!(plain, "this is italic text");
    }

    #[test]
    fn test_render_inline_code() {
        let r = StreamRenderer::new(80);
        let out = r.render("use `fmt.Println()` to print");
        let plain = strip_ansi_str(&out);
        assert!(plain.contains("fmt.Println()"));
    }

    #[test]
    fn test_render_inline_strikethrough() {
        let r = StreamRenderer::new(80);
        let out = r.render("this is ~~struck~~ text");
        let plain = strip_ansi_str(&out);
        assert_eq!(plain, "this is struck text");
    }

    #[test]
    fn test_render_inline_link() {
        let r = StreamRenderer::new(80);
        let out = r.render("visit [example](https://example.com) today");
        let plain = strip_ansi_str(&out);
        assert!(plain.contains("example"));
        assert!(!plain.contains("https://example.com"));
    }

    #[test]
    fn test_render_header() {
        let r = StreamRenderer::new(80);
        let out = r.render("# Hello");
        assert!(strip_ansi_str(&out).contains("Hello"));
    }

    #[test]
    fn test_render_header_no_space_passthrough() {
        let r = StreamRenderer::new(80);
        let out = r.render("#NoSpace");
        assert!(strip_ansi_str(&out).contains("#NoSpace"));
    }

    #[test]
    fn test_render_bullet() {
        let r = StreamRenderer::new(80);
        let out = r.render("- item one");
        assert!(strip_ansi_str(&out).contains("item one"));
    }

    #[test]
    fn test_render_star_bullet() {
        let r = StreamRenderer::new(80);
        let out = r.render("* star item");
        assert!(strip_ansi_str(&out).contains("star item"));
    }

    #[test]
    fn test_render_task_todo() {
        let r = StreamRenderer::new(80);
        let out = r.render("- [ ] todo item");
        let plain = strip_ansi_str(&out);
        assert!(plain.contains("todo item"));
        assert!(plain.contains("[ ]"));
    }

    #[test]
    fn test_render_task_done() {
        let r = StreamRenderer::new(80);
        let out = r.render("- [x] done item");
        let plain = strip_ansi_str(&out);
        assert!(plain.contains("done item"));
        assert!(plain.contains("[x]"));
    }

    #[test]
    fn test_render_numbered_list() {
        let r = StreamRenderer::new(80);
        let out = r.render("1. first item");
        let plain = strip_ansi_str(&out);
        assert!(plain.contains("1."));
        assert!(plain.contains("first item"));
    }

    #[test]
    fn test_render_blockquote() {
        let r = StreamRenderer::new(80);
        let out = r.render("> quoted text");
        assert!(strip_ansi_str(&out).contains("quoted text"));
    }

    #[test]
    fn test_render_code_fence() {
        let r = StreamRenderer::new(80);
        let out = r.render("```go\nfunc main() {}\n```");
        let plain = strip_ansi_str(&out);
        assert!(plain.contains("func main()"));
        assert!(!plain.contains("```"));
    }

    #[test]
    fn test_render_horizontal_rule() {
        let r = StreamRenderer::new(80);
        for hr in &["---", "***", "___"] {
            let out = r.render(hr);
            assert!(strip_ansi_str(&out).contains("─"), "hr {hr} not rendered");
        }
    }

    #[test]
    fn test_render_horizontal_rule_short() {
        let r = StreamRenderer::new(80);
        let out = r.render("--");
        assert!(!strip_ansi_str(&out).contains("─"));
    }

    #[test]
    fn test_render_no_trailing_newline() {
        let r = StreamRenderer::new(80);
        let out = r.render("single line");
        assert!(!out.ends_with('\n'));
    }

    #[test]
    fn test_render_mixed_content() {
        let r = StreamRenderer::new(80);
        let input = "# Title\n\nSome paragraph with **bold**.\n\n- bullet one\n- bullet two\n\n| A | B |\n|---|---|\n| 1 | 2 |\n";
        let out = r.render(input);
        let plain = strip_ansi_str(&out);
        assert!(plain.contains("Title"));
        assert!(plain.contains("bold"));
        assert!(plain.contains("bullet one"));
        assert!(plain.contains("bullet two"));
        assert!(plain.contains("│"));
    }

    #[test]
    fn test_render_golden_headers_and_inline() {
        let r = StreamRenderer::new(40);
        let input = "# Title\n\nThis is **bold** and *italic*.";
        let got = strip_ansi_str(&r.render(input));
        let want = "Title\n\nThis is bold and italic.";
        assert_eq!(got, want);
    }

    #[test]
    fn test_render_golden_lists_and_blockquote() {
        let r = StreamRenderer::new(40);
        let input = "- Item one\n- Item two\n\n> A quote";
        let got = strip_ansi_str(&r.render(input));
        let want = "- Item one\n- Item two\n\n| A quote";
        assert_eq!(got, want);
    }

    #[test]
    fn test_render_golden_kitchen_sink() {
        let r = StreamRenderer::new(40);
        let input = "# Title\n\npara\n\n- a\n- b\n\n| A | B |\n|---|---|\n| 1 | 2 |";
        let got = strip_ansi_str(&r.render(input));
        assert!(got.contains("Title"));
        assert!(got.contains("para"));
        assert!(got.contains("a"));
        assert!(got.contains("b"));
        assert!(got.contains("┌"));
        assert!(got.contains("┐"));
    }

    #[test]
    fn test_render_bullet_wrapping() {
        let r = StreamRenderer::new(30);
        let out = r.render(
            "- this is a very long bullet point that should wrap to the next line properly",
        );
        let ls = lines(&out);
        assert!(
            ls.len() >= 2,
            "expected wrapping, got {} lines: {out}",
            ls.len()
        );
        if ls.len() >= 2 {
            let indent = strip_ansi_str(ls[1]);
            assert!(
                indent.starts_with("  "),
                "continuation should be indented: {indent}"
            );
        }
    }

    #[test]
    fn test_is_code_fence() {
        assert!(is_code_fence("```go"));
        assert!(is_code_fence("~~~"));
        assert!(!is_code_fence("``"));
    }

    #[test]
    fn test_is_header() {
        assert!(is_header("# H1"));
        assert!(is_header("###### H6"));
        assert!(!is_header("not a header"));
    }

    #[test]
    fn test_is_blockquote() {
        assert!(is_blockquote("> quote"));
        assert!(!is_blockquote("not > quote"));
    }

    #[test]
    fn test_is_numbered_list() {
        assert!(is_numbered_list("1. item"));
        assert!(is_numbered_list("10. item"));
        assert!(!is_numbered_list("item"));
        assert!(!is_numbered_list("123.45"));
    }

    #[test]
    fn test_is_bullet() {
        assert!(is_bullet("- item"));
        assert!(is_bullet("* item"));
        assert!(!is_bullet("**bold**"));
        assert!(!is_bullet("item"));
    }

    #[test]
    fn test_is_nested_bullet_fn() {
        let (indent, ok) = is_nested_bullet("  - nested").unwrap();
        assert_eq!(indent, 2);
        assert!(ok);
        assert!(is_nested_bullet("- top level").is_none());
    }
}
