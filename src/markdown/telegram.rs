use super::*;
use once_cell::sync::Lazy;
use regex::Regex;

const P_CODE_BLOCK: &str = "\u{00a7}C";
const P_INLINE_CODE: &str = "\u{00a7}I";
const P_ITALIC_S: &str = "\u{00a7}IS\u{00a7}";
const P_ITALIC_E: &str = "\u{00a7}IE\u{00a7}";
const P_STRIKE_S: &str = "\u{00a7}SS\u{00a7}";
const P_STRIKE_E: &str = "\u{00a7}SE\u{00a7}";
const P_LINK_S: &str = "\u{00a7}LS\u{00a7}";
const P_LINK_SEP: &str = "\u{00a7}LP\u{00a7}";
const P_LINK_E: &str = "\u{00a7}LE\u{00a7}";

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
    markdown_to_platform(
        text,
        P_CODE_BLOCK,
        P_INLINE_CODE,
        telegram_pre_heading,
        telegram_phase1,
        telegram_phase2,
        telegram_clean_cb,
    )
}

fn telegram_pre_heading(
    text: &str,
    _code_blocks: &mut Vec<String>,
    _inline_codes: &mut [String],
) -> (String, HashMap<usize, bool>) {
    (text.to_string(), HashMap::new())
}

fn telegram_phase1(text: &str) -> String {
    let text = RE_ITALIC
        .replace_all(text, |caps: &regex::Captures| {
            format!("{}{}{}", P_ITALIC_S, &caps[1], P_ITALIC_E)
        })
        .to_string();
    let text = RE_STRIKE
        .replace_all(&text, |caps: &regex::Captures| {
            format!("{}{}{}", P_STRIKE_S, &caps[1], P_STRIKE_E)
        })
        .to_string();
    RE_LINK
        .replace_all(&text, |caps: &regex::Captures| {
            format!(
                "{}{}{}{}{}",
                P_LINK_S, &caps[1], P_LINK_SEP, &caps[2], P_LINK_E
            )
        })
        .to_string()
}

fn telegram_phase2(text: &str) -> String {
    let text = escape_telegram(text);

    let text = RE_BOLD_PLACEHOLDER
        .replace_all(&text, |caps: &regex::Captures| {
            let inner = caps[1].to_string();
            let inner = escape_nested(&inner, &['*', '_']);
            format!("*{}*", inner)
        })
        .to_string();

    let text = RE_ITALIC_PLACEHOLDER
        .replace_all(&text, |caps: &regex::Captures| {
            let inner = caps[1].to_string();
            let inner = escape_nested(&inner, &['_']);
            format!("_{}_", inner)
        })
        .to_string();

    let text = RE_STRIKE_PLACEHOLDER
        .replace_all(&text, |caps: &regex::Captures| {
            let inner = caps[1].to_string();
            let inner = escape_nested(&inner, &['~']);
            format!("~{}~", inner)
        })
        .to_string();

    RE_LINK_PLACEHOLDER
        .replace_all(&text, "[${1}](${2})")
        .to_string()
}

fn telegram_clean_cb(block: &str) -> String {
    block.to_string()
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
        assert!(
            got.contains("code"),
            "code content should be preserved: {}",
            got
        );
    }

    #[test]
    fn test_markdown_to_telegram_inline_code() {
        let got = markdown_to_telegram("run `ls -la` now");
        assert!(
            got.contains("`ls -la`"),
            "inline code should be preserved: {}",
            got
        );
    }

    #[test]
    fn test_markdown_to_telegram_list() {
        let got = markdown_to_telegram("- item one\n- item two");
        assert!(
            got.contains("item one"),
            "list items should be preserved: {}",
            got
        );
        assert!(
            !got.contains("- "),
            "list bullets should be converted: {}",
            got
        );
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
        assert!(
            got.contains("\\_"),
            "underscore outside formatting should be escaped: {}",
            got
        );
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
