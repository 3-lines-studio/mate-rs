use crate::render::block::hex_to_rgb;
use crate::render::theme::VESPER;
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Color as SyntectColor, FontStyle, Style, StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use once_cell::sync::Lazy;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);

fn sc(hex: &str) -> SyntectColor {
    let (r, g, b) = hex_to_rgb(hex);
    SyntectColor { r, g, b, a: 0xFF }
}

fn build_vesper_theme() -> Theme {
    let mut settings = ThemeSettings::default();

    let bg = sc(VESPER.bg);
    let fg = sc(VESPER.fg);
    let keyword_color = sc(VESPER.keyword);
    let function_color = sc(VESPER.function);
    let typ_color = sc(VESPER.typ);
    let string_color = sc(VESPER.string);
    let comment_color = sc(VESPER.comment);
    let operator_color = sc(VESPER.operator);
    let error_color = sc(VESPER.error);
    let number_color = sc(VESPER.number);

    settings.background = Some(bg);
    settings.foreground = Some(fg);
    settings.caret = Some(fg);
    settings.selection = Some(sc(VESPER.selected));
    settings.line_highlight = Some(sc(VESPER.surface));

    let mk = |c: SyntectColor| -> StyleModifier {
        StyleModifier {
            foreground: Some(c),
            background: None,
            font_style: Some(FontStyle::empty()),
        }
    };
    let mk_bold = |c: SyntectColor| -> StyleModifier {
        StyleModifier {
            foreground: Some(c),
            background: None,
            font_style: Some(FontStyle::BOLD),
        }
    };
    let mk_italic = |c: SyntectColor| -> StyleModifier {
        StyleModifier {
            foreground: Some(c),
            background: None,
            font_style: Some(FontStyle::ITALIC),
        }
    };

    let item = |scope: &str, style: StyleModifier| -> ThemeItem {
        ThemeItem {
            scope: scope.parse().unwrap(),
            style,
        }
    };

    let scopes = vec![
        item("keyword", mk_bold(keyword_color)),
        item("keyword.control", mk_bold(keyword_color)),
        item("keyword.operator", mk(operator_color)),
        item("keyword.other", mk_bold(keyword_color)),
        item("storage.type", mk(typ_color)),
        item("storage.modifier", mk(keyword_color)),
        item("entity.name.type", mk(typ_color)),
        item("entity.name.class", mk(typ_color)),
        item("entity.name.namespace", mk(typ_color)),
        item("entity.name.function", mk(function_color)),
        item("entity.name.function.constructor", mk(function_color)),
        item("support.function", mk(function_color)),
        item("support.function.builtin", mk(function_color)),
        item("entity.name.tag", mk(function_color)),
        item("variable.other.member", mk(fg)),
        item("variable.parameter", mk(fg)),
        item("variable.language", mk(keyword_color)),
        item("constant.numeric", mk(number_color)),
        item("constant.language", mk(number_color)),
        item("constant.character", mk(string_color)),
        item("constant.other", mk(number_color)),
        item("string", mk(string_color)),
        item("string.quoted", mk(string_color)),
        item("string.regexp", mk(string_color)),
        item("comment", mk_italic(comment_color)),
        item("comment.line", mk_italic(comment_color)),
        item("comment.block", mk_italic(comment_color)),
        item("punctuation", mk(operator_color)),
        item("punctuation.definition", mk(operator_color)),
        item("meta", mk(fg)),
        item("invalid", mk(error_color)),
        item("invalid.deprecated", mk(error_color)),
    ];

    Theme {
        name: Some("Vesper".into()),
        author: Some("mate".into()),
        settings,
        scopes,
    }
}

static VESPER_THEME: Lazy<Theme> = Lazy::new(build_vesper_theme);

fn syntect_style_to_ansi(s: Style) -> String {
    let mut codes: Vec<String> = Vec::new();
    if s.font_style
        .contains(syntect::highlighting::FontStyle::BOLD)
    {
        codes.push("1".into());
    }
    if s.font_style
        .contains(syntect::highlighting::FontStyle::ITALIC)
    {
        codes.push("3".into());
    }
    if s.font_style
        .contains(syntect::highlighting::FontStyle::UNDERLINE)
    {
        codes.push("4".into());
    }
    codes.push(format!(
        "38;2;{};{};{}",
        s.foreground.r, s.foreground.g, s.foreground.b
    ));
    format!("\x1b[{}m", codes.join(";"))
}

fn syntect_reset() -> &'static str {
    "\x1b[0m"
}

pub fn highlight(lang: &str, content: &str) -> String {
    let syntax = SYNTAX_SET
        .find_syntax_by_token(lang)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(lang))
        .or_else(|| {
            let first_line = content.lines().next().unwrap_or("");
            SYNTAX_SET.find_syntax_by_first_line(first_line)
        });

    let syntax = match syntax {
        Some(s) => s,
        None => return content.to_string(),
    };

    let mut highlighter = HighlightLines::new(syntax, &VESPER_THEME);
    let mut out = String::with_capacity(content.len() * 2);

    for line in LinesWithEndings::from(content) {
        let ranges: Vec<(syntect::highlighting::Style, &str)> = highlighter
            .highlight_line(line, &SYNTAX_SET)
            .unwrap_or_default();

        for (style, text) in &ranges {
            out.push_str(&syntect_style_to_ansi(*style));
            out.push_str(text);
            out.push_str(syntect_reset());
        }
    }

    let trimmed = out.trim_end_matches('\n');
    if trimmed.is_empty() && !out.is_empty() {
        out
    } else {
        trimmed.to_string()
    }
}

pub fn highlight_code(code: &str, lang: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    if lines.len() >= 2 {
        let inner = &lines[1..lines.len() - 1];
        let content = inner.join("\n");
        return highlight(lang, &content);
    }
    highlight(lang, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_go() {
        let result = highlight("go", "func main() {}");
        assert!(!result.is_empty());
        assert!(result.contains("func"));
    }

    #[test]
    fn test_highlight_code_strips_fences() {
        let result = highlight_code("```go\nfunc main() {}\n```", "go");
        assert!(!result.contains("```"));
        assert!(result.contains("func"));
    }

    #[test]
    fn test_highlight_unknown_lang_fallback() {
        let result = highlight("some_unknown_lang", "hello world");
        assert!(result.contains("hello world"));
    }
}
