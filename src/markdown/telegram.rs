use once_cell::sync::Lazy;
use regex::Regex;

static RE_CODE_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)```[\s\S]*?```").unwrap());
static RE_INLINE_CODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"`[^`\n]+`").unwrap());
static RE_HEADING: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^#{1,6}\s+(.+)$").unwrap());
static RE_BOLD1: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*\*(.+?)\*\*").unwrap());
static RE_BOLD2: Lazy<Regex> = Lazy::new(|| Regex::new(r"__(.+?)__").unwrap());
static RE_ITALIC: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*(.+?)\*").unwrap());
static RE_STRIKE: Lazy<Regex> = Lazy::new(|| Regex::new(r"~~(.+?)~~").unwrap());
static RE_LINK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap());
static RE_LIST: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^(\s*)[-*]\s+").unwrap());
static RE_MULTI_NL: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").unwrap());

const P_CODE_BLOCK: &str = "\u{00a7}C";
const P_INLINE_CODE: &str = "\u{00a7}I";
const P_BOLD_S: &str = "\u{00a7}BS\u{00a7}";
const P_BOLD_E: &str = "\u{00a7}BE\u{00a7}";
const P_ITALIC_S: &str = "\u{00a7}IS\u{00a7}";
const P_ITALIC_E: &str = "\u{00a7}IE\u{00a7}";
const P_STRIKE_S: &str = "\u{00a7}SS\u{00a7}";
const P_STRIKE_E: &str = "\u{00a7}SE\u{00a7}";
const P_LINK_S: &str = "\u{00a7}LS\u{00a7}";
const P_LINK_SEP: &str = "\u{00a7}LP\u{00a7}";
const P_LINK_E: &str = "\u{00a7}LE\u{00a7}";
const P_SUFFIX: &str = "\u{00a7}";

static RE_BOLD_PLACEHOLDER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        "{}{}{}",
        regex::escape(P_BOLD_S),
        r"(.+?)",
        regex::escape(P_BOLD_E)
    ))
    .unwrap()
});

static RE_ITALIC_PLACEHOLDER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        "{}{}{}",
        regex::escape(P_ITALIC_S),
        r"(.+?)",
        regex::escape(P_ITALIC_E)
    ))
    .unwrap()
});

static RE_STRIKE_PLACEHOLDER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        "{}{}{}",
        regex::escape(P_STRIKE_S),
        r"(.+?)",
        regex::escape(P_STRIKE_E)
    ))
    .unwrap()
});

static RE_LINK_PLACEHOLDER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        "{}{}{}{}{}",
        regex::escape(P_LINK_S),
        r"(.+?)",
        regex::escape(P_LINK_SEP),
        r"(.+?)",
        regex::escape(P_LINK_E)
    ))
    .unwrap()
});

static RE_ANY_PLACEHOLDER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        "{}{}{}|{}{}{}|{}{}{}|{}{}{}|{}{}{}|{}{}{}{}{}",
        regex::escape(P_CODE_BLOCK),
        r"\d+",
        regex::escape(P_SUFFIX),
        regex::escape(P_INLINE_CODE),
        r"\d+",
        regex::escape(P_SUFFIX),
        regex::escape(P_BOLD_S),
        r".+?",
        regex::escape(P_BOLD_E),
        regex::escape(P_ITALIC_S),
        r".+?",
        regex::escape(P_ITALIC_E),
        regex::escape(P_STRIKE_S),
        r".+?",
        regex::escape(P_STRIKE_E),
        regex::escape(P_LINK_S),
        r".+?",
        regex::escape(P_LINK_SEP),
        r".+?",
        regex::escape(P_LINK_E),
    ))
    .unwrap()
});

pub fn markdown_to_telegram(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    let mut code_blocks: Vec<String> = Vec::new();
    let mut inline_codes: Vec<String> = Vec::new();

    let mut text = RE_CODE_BLOCK
        .replace_all(text, |caps: &regex::Captures| {
            let idx = code_blocks.len();
            code_blocks.push(caps[0].to_string());
            format!("{}{}{}", P_CODE_BLOCK, idx, P_SUFFIX)
        })
        .to_string();

    text = RE_INLINE_CODE
        .replace_all(&text, |caps: &regex::Captures| {
            let idx = inline_codes.len();
            inline_codes.push(caps[0].to_string());
            format!("{}{}{}", P_INLINE_CODE, idx, P_SUFFIX)
        })
        .to_string();

    text = RE_HEADING
        .replace_all(&text, |caps: &regex::Captures| {
            let content = &caps[1];
            let content = strip_formatting(content);
            format!("{}{}{}", P_BOLD_S, content, P_BOLD_E)
        })
        .to_string();

    text = RE_BOLD1
        .replace_all(&text, |caps: &regex::Captures| {
            format!("{}{}{}", P_BOLD_S, &caps[1], P_BOLD_E)
        })
        .to_string();
    text = RE_BOLD2
        .replace_all(&text, |caps: &regex::Captures| {
            format!("{}{}{}", P_BOLD_S, &caps[1], P_BOLD_E)
        })
        .to_string();

    text = RE_ITALIC
        .replace_all(&text, |caps: &regex::Captures| {
            format!("{}{}{}", P_ITALIC_S, &caps[1], P_ITALIC_E)
        })
        .to_string();

    text = RE_STRIKE
        .replace_all(&text, |caps: &regex::Captures| {
            format!("{}{}{}", P_STRIKE_S, &caps[1], P_STRIKE_E)
        })
        .to_string();

    text = RE_LINK
        .replace_all(&text, |caps: &regex::Captures| {
            format!(
                "{}{}{}{}{}",
                P_LINK_S, &caps[1], P_LINK_SEP, &caps[2], P_LINK_E
            )
        })
        .to_string();

    text = RE_LIST
        .replace_all(&text, "${1}\u{2022}  ")
        .to_string();

    text = escape_telegram(&text);

    text = RE_BOLD_PLACEHOLDER
        .replace_all(&text, |caps: &regex::Captures| {
            let inner = caps[1].to_string();
            let inner = escape_nested(&inner, &['*', '_']);
            format!("*{}*", inner)
        })
        .to_string();

    text = RE_ITALIC_PLACEHOLDER
        .replace_all(&text, |caps: &regex::Captures| {
            let inner = caps[1].to_string();
            let inner = escape_nested(&inner, &['_']);
            format!("_{}_", inner)
        })
        .to_string();

    text = RE_STRIKE_PLACEHOLDER
        .replace_all(&text, |caps: &regex::Captures| {
            let inner = caps[1].to_string();
            let inner = escape_nested(&inner, &['~']);
            format!("~{}~", inner)
        })
        .to_string();

    text = RE_LINK_PLACEHOLDER
        .replace_all(&text, "[${1}](${2})")
        .to_string();

    for (i, block) in code_blocks.iter().enumerate() {
        let placeholder = format!("{}{}{}", P_CODE_BLOCK, i, P_SUFFIX);
        text = text.replace(&placeholder, &format!("\n{}\n", block));
    }

    for (i, code) in inline_codes.iter().enumerate() {
        let placeholder = format!("{}{}{}", P_INLINE_CODE, i, P_SUFFIX);
        text = text.replace(&placeholder, code);
    }

    text = RE_MULTI_NL.replace_all(&text, "\n\n").to_string();

    text = text.trim_start_matches('\n').to_string();
    text = text.trim_end_matches('\n').to_string();

    text
}

fn escape_telegram(text: &str) -> String {
    let placeholder_matches: Vec<(usize, usize)> = RE_ANY_PLACEHOLDER
        .find_iter(text)
        .map(|m| (m.start(), m.end()))
        .collect();

    let mut buf = String::with_capacity(text.len());
    for (byte_idx, c) in text.char_indices() {
        let in_placeholder = placeholder_matches
            .iter()
            .any(|(start, end)| byte_idx >= *start && byte_idx < *end);

        if in_placeholder {
            buf.push(c);
            continue;
        }

        if "_*[]()~`>#+-=|{}.!".contains(c) {
            buf.push('\\');
        }
        buf.push(c);
    }

    buf
}

fn strip_formatting(s: &str) -> String {
    let s = RE_BOLD1.replace_all(s, "$1").to_string();
    let s = RE_BOLD2.replace_all(&s, "$1").to_string();
    RE_ITALIC.replace_all(&s, "$1").to_string()
}

fn escape_nested(s: &str, chars: &[char]) -> String {
    let mut buf = String::with_capacity(s.len());
    for c in s.chars() {
        if chars.contains(&c) {
            buf.push('\\');
        }
        buf.push(c);
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_to_telegram_empty() {
        let got = markdown_to_telegram("");
        assert_eq!(got, "");
    }

    #[test]
    fn test_markdown_to_telegram_plain_text() {
        let got = markdown_to_telegram("hello world");
        assert_eq!(got, "hello world");
    }

    #[test]
    fn test_markdown_to_telegram_bold() {
        let got = markdown_to_telegram("this is **bold** text");
        assert_eq!(got, "this is *bold* text");
    }

    #[test]
    fn test_markdown_to_telegram_italic() {
        let got = markdown_to_telegram("this is *italic* text");
        assert_eq!(got, "this is _italic_ text");
    }

    #[test]
    fn test_markdown_to_telegram_strikethrough() {
        let got = markdown_to_telegram("this is ~~strike~~ text");
        assert_eq!(got, "this is ~strike~ text");
    }

    #[test]
    fn test_markdown_to_telegram_code_block() {
        let got = markdown_to_telegram("text\n```\ncode\n```\nmore");
        assert!(got.contains("code"), "code content should be preserved: {}", got);
    }

    #[test]
    fn test_markdown_to_telegram_inline_code() {
        let got = markdown_to_telegram("run `ls -la` now");
        assert!(got.contains("`ls -la`"), "inline code should be preserved: {}", got);
    }

    #[test]
    fn test_markdown_to_telegram_list() {
        let got = markdown_to_telegram("- item one\n- item two");
        assert!(got.contains("item one"), "list items should be preserved: {}", got);
        assert!(!got.contains("- "), "list bullets should be converted: {}", got);
    }

    #[test]
    fn test_markdown_to_telegram_link() {
        let got = markdown_to_telegram("[click here](https://example.com)");
        assert_eq!(got, "[click here](https://example.com)");
    }

    #[test]
    fn test_markdown_to_telegram_heading() {
        let got = markdown_to_telegram("# Title here");
        assert_eq!(got, "*Title here*");
    }

    #[test]
    fn test_markdown_to_telegram_escape_specials() {
        let got = markdown_to_telegram("test _ underscore");
        assert!(got.contains("\\_"), "underscore outside formatting should be escaped: {}", got);
    }

    #[test]
    fn test_strip_formatting_bold() {
        let got = strip_formatting("hello **world** here");
        assert!(!got.contains("**"), "bold markers should be removed");
        assert!(got.contains("world"), "bold text should remain");
    }

    #[test]
    fn test_strip_formatting_no_markup() {
        let got = strip_formatting("plain text");
        assert_eq!(got, "plain text");
    }

    #[test]
    fn test_markdown_to_telegram_nested_bold_literal_asterisk() {
        let got = markdown_to_telegram("**2 * 3 = 6**");
        assert_eq!(got, "*2 \\* 3 = 6*");
    }

    #[test]
    fn test_markdown_to_telegram_nested_formatting() {
        let got = markdown_to_telegram("**bold *italic* bold**");
        assert_eq!(got, "*bold _italic_ bold*");
    }
}
