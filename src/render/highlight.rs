use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Color as SyntectColor, FontStyle, Style, StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use once_cell::sync::Lazy;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);

fn build_vesper_theme() -> Theme {
    let mut settings = ThemeSettings::default();

    let bg = SyntectColor {
        r: 0x14,
        g: 0x14,
        b: 0x14,
        a: 0xFF,
    };
    let fg = SyntectColor {
        r: 0xe1,
        g: 0xe1,
        b: 0xe1,
        a: 0xFF,
    };
    let keyword_color = SyntectColor {
        r: 0xbb,
        g: 0x9a,
        b: 0xf7,
        a: 0xFF,
    };
    let typ_color = SyntectColor {
        r: 0x7a,
        g: 0xa2,
        b: 0xf7,
        a: 0xFF,
    };
    let string_color = SyntectColor {
        r: 0x9e,
        g: 0xce,
        b: 0x6a,
        a: 0xFF,
    };
    let comment_color = SyntectColor {
        r: 0x6c,
        g: 0x6c,
        b: 0x6c,
        a: 0xFF,
    };
    let operator_color = SyntectColor {
        r: 0x89,
        g: 0xdd,
        b: 0xff,
        a: 0xFF,
    };
    let error_color = SyntectColor {
        r: 0xf7,
        g: 0x76,
        b: 0x8e,
        a: 0xFF,
    };
    let number_color = SyntectColor {
        r: 0xff,
        g: 0x9e,
        b: 0x64,
        a: 0xFF,
    };

    settings.background = Some(bg);
    settings.foreground = Some(fg);
    settings.caret = Some(fg);
    settings.selection = Some(SyntectColor {
        r: 0x24,
        g: 0x24,
        b: 0x24,
        a: 0xFF,
    });
    settings.line_highlight = Some(SyntectColor {
        r: 0x1c,
        g: 0x1c,
        b: 0x1c,
        a: 0xFF,
    });

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
        item("entity.name.function", mk(keyword_color)),
        item("entity.name.function.constructor", mk(keyword_color)),
        item("support.function", mk(keyword_color)),
        item("support.function.builtin", mk(keyword_color)),
        item("entity.name.tag", mk(keyword_color)),
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
