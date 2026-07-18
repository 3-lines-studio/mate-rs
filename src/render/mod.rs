pub mod block;
pub mod highlight;
pub mod mdcat;
pub mod theme;

pub use block::strip_ansi;

pub use highlight::{highlight, highlight_code};
pub use theme::VESPER;

pub struct StreamRenderer {
    width: usize,
}

impl StreamRenderer {
    pub fn new(width: usize) -> Self {
        StreamRenderer { width }
    }

    pub fn render(&self, text: &str) -> String {
        use pulldown_cmark::Parser;
        let settings = mdcat::Settings {
            syntax_set: &highlight::SYNTAX_SET,
            terminal_capabilities: mdcat::terminal::TerminalProgram::Ansi.capabilities(),
            terminal_size: mdcat::terminal::TerminalSize {
                columns: self.width as u16,
                rows: 24,
                pixels: None,
                cell: None,
            },
            theme: mdcat::Theme::default(),
            syntax_theme: Some(highlight::VESPER_THEME.clone()),
        };
        let environment = mdcat::Environment {
            base_url: url::Url::parse("file:///").expect("valid base url"),
            hostname: "localhost".into(),
        };
        let mut sink = Vec::new();
        let source = Parser::new_ext(mdcat::strip_frontmatter(text), mdcat::markdown_options());
        if let Err(e) = mdcat::push_tty(
            &settings,
            &environment,
            &mdcat::resources::NoopResourceHandler,
            &mut sink,
            source,
        ) {
            log::error!("mdcat push_tty failed: {e}");
        }
        String::from_utf8_lossy(&sink)
            .trim_end_matches('\n')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_empty_input() {
        let r = StreamRenderer::new(80);
        assert!(r.render("").trim().is_empty());
    }

    #[test]
    fn smoke_paragraph_no_panic() {
        let r = StreamRenderer::new(80);
        let out = r.render("Hello world.");
        let plain = block::strip_ansi(&out);
        assert!(plain.contains("Hello world."));
    }

    #[test]
    fn smoke_inline_formatting_stripped() {
        let r = StreamRenderer::new(80);
        let out = r.render("**bold** *italic* `code` ~~strike~~ [link](https://x.com)");
        let plain = block::strip_ansi(&out);
        assert!(plain.contains("bold"));
        assert!(plain.contains("italic"));
        assert!(plain.contains("code"));
        assert!(plain.contains("strike"));
        assert!(plain.contains("link"));
    }

    #[test]
    fn smoke_code_block_no_fences() {
        let r = StreamRenderer::new(80);
        let out = r.render("```\nfn main() {}\n```");
        let plain = block::strip_ansi(&out);
        assert!(plain.contains("fn main() {}"));
        assert!(!plain.contains("```"));
    }

    #[test]
    fn smoke_heading() {
        let r = StreamRenderer::new(80);
        let out = r.render("# Hello");
        let plain = block::strip_ansi(&out);
        assert!(plain.contains("Hello"));
    }
}
