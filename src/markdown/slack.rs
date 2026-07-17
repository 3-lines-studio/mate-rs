use super::*;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

static RE_TABLE_BLOCK: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)(^\|.+\|\s*$\n?)+").unwrap());
static RE_TABLE_SEP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\|[\s\-:|]+\|$").unwrap());
static RE_IMAGE: Lazy<Regex> = Lazy::new(|| Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap());
static RE_HR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^(-{3,}|\*{3,}|_{3,})$").unwrap());
static RE_CODE_LANG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)^```[a-zA-Z0-9_+-]*\s*\n").unwrap());

const P_CODE_BLOCK: &str = "\u{00a7}CB_";
const P_INLINE_CODE: &str = "\u{00a7}IC_";

static RE_INLINE_CODE_PLACEHOLDER: Lazy<Regex> = Lazy::new(|| {
    let pat = format!(
        "{}{}{}",
        regex::escape(P_INLINE_CODE),
        r"\d+",
        regex::escape(P_SUFFIX)
    );
    Regex::new(&pat).unwrap()
});

pub fn markdown_to_slack(text: &str) -> String {
    markdown_to_platform(
        text,
        P_CODE_BLOCK,
        P_INLINE_CODE,
        slack_pre_heading,
        slack_phase1,
        slack_phase2,
        slack_clean_cb,
    )
}

fn slack_pre_heading(
    text: &str,
    code_blocks: &mut Vec<String>,
    inline_codes: &mut [String],
) -> (String, HashMap<usize, bool>) {
    let mut resolved_inlines: HashMap<usize, bool> = HashMap::new();

    let text = RE_TABLE_BLOCK
        .replace_all(text, |caps: &regex::Captures| {
            let table_block = caps[0].to_string();
            let lines: Vec<&str> = table_block.trim().lines().collect();
            let data_lines: Vec<&str> = lines
                .into_iter()
                .filter(|l| !RE_TABLE_SEP.is_match(l))
                .collect();

            if data_lines.is_empty() {
                return table_block;
            }

            let mut rows: Vec<Vec<String>> = Vec::new();
            for line in &data_lines {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() <= 2 {
                    continue;
                }
                let cols: Vec<String> = parts[1..parts.len() - 1]
                    .iter()
                    .map(|c| c.trim().to_string())
                    .collect();
                rows.push(cols);
            }

            for row in &mut rows {
                for cell in row.iter_mut() {
                    *cell = RE_INLINE_CODE_PLACEHOLDER
                        .replace_all(cell, |ph_caps: &regex::Captures| {
                            let ph = &ph_caps[0];
                            let idx = parse_placeholder_idx(ph, P_INLINE_CODE);
                            if idx >= 0 && (idx as usize) < inline_codes.len() {
                                resolved_inlines.insert(idx as usize, true);
                                inline_codes[idx as usize].trim_matches('`').to_string()
                            } else {
                                ph.to_string()
                            }
                        })
                        .to_string();
                    *cell = RE_BOLD1.replace_all(cell, "$1").to_string();
                    *cell = RE_BOLD2.replace_all(cell, "$1").to_string();
                }
            }

            let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
            let mut widths: Vec<usize> = vec![0; col_count];
            for row in &rows {
                for i in 0..row.len() {
                    if row[i].len() > widths[i] {
                        widths[i] = row[i].len();
                    }
                }
            }

            let mut formatted = String::new();
            for (ri, row) in rows.iter().enumerate() {
                for i in 0..col_count {
                    let cell = if i < row.len() { &row[i] } else { "" };
                    formatted.push_str(cell);
                    if i < col_count - 1 {
                        let pad = widths[i].saturating_sub(cell.len()) + 3;
                        formatted.push_str(&" ".repeat(pad));
                    }
                }
                if ri < rows.len() - 1 {
                    formatted.push('\n');
                }
            }

            let block = format!("```\n{}\n```", formatted);
            let block_idx = code_blocks.len();
            code_blocks.push(block);
            format!("{}{}{}\n", P_CODE_BLOCK, block_idx, P_SUFFIX)
        })
        .to_string();

    (text, resolved_inlines)
}

fn slack_phase1(text: &str) -> String {
    let mut text = RE_ITALIC.replace_all(text, "_${1}_").to_string();
    text = RE_STRIKE.replace_all(&text, "~${1}~").to_string();
    text = RE_IMAGE.replace_all(&text, "<${2}|${1}>").to_string();
    text = RE_LINK.replace_all(&text, "<${2}|${1}>").to_string();
    text = RE_HR
        .replace_all(&text, "\u{2014}\u{2014}\u{2014}")
        .to_string();
    text
}

fn slack_phase2(text: &str) -> String {
    RE_BOLD_PLACEHOLDER.replace_all(text, "*${1}*").to_string()
}

fn slack_clean_cb(block: &str) -> String {
    RE_CODE_LANG.replace_all(block, "```\n").to_string()
}

fn parse_placeholder_idx(s: &str, prefix: &str) -> isize {
    let s = s.strip_prefix(prefix).unwrap_or(s);
    let s = s.strip_suffix(P_SUFFIX).unwrap_or(s);
    let mut n: isize = 0;
    for c in s.chars() {
        if !c.is_ascii_digit() {
            return -1;
        }
        n = n * 10 + (c as isize - '0' as isize);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_formatting_bold() {
        let got = strip_formatting("hello **world** here");
        assert!(!got.contains("**"), "bold markers should be removed");
        assert!(got.contains("world"), "bold text should remain");
    }

    #[test]
    fn test_strip_formatting_underscore_bold() {
        let got = strip_formatting("hello __world__ here");
        assert!(!got.contains("__"), "bold markers should be removed");
        assert!(got.contains("world"), "bold text should remain");
    }

    #[test]
    fn test_strip_formatting_italic() {
        let got = strip_formatting("hello *world* here");
        assert!(!got.contains('*'), "italic markers should be removed");
        assert!(got.contains("world"), "italic text should remain");
    }

    #[test]
    fn test_strip_formatting_strikethrough() {
        let got = strip_formatting("hello ~~world~~ here");
        assert!(
            got.contains("~~world~~"),
            "strikethrough is not stripped by strip_formatting"
        );
    }

    #[test]
    fn test_strip_formatting_no_markup() {
        let got = strip_formatting("plain text");
        assert_eq!(got, "plain text");
    }

    #[test]
    fn test_parse_placeholder_idx_valid() {
        let got = parse_placeholder_idx("\u{00a7}IC_42\u{00a7}", "\u{00a7}IC_");
        assert_eq!(got, 42);
    }

    #[test]
    fn test_parse_placeholder_idx_invalid() {
        let got = parse_placeholder_idx("not-a-placeholder", "\u{00a7}IC_");
        assert_eq!(got, -1);
    }

    #[test]
    fn test_markdown_to_slack_empty() {
        let got = markdown_to_slack("");
        assert_eq!(got, "");
    }

    #[test]
    fn test_markdown_to_slack_plain_text() {
        let got = markdown_to_slack("hello world");
        assert_eq!(got, "hello world");
    }

    #[test]
    fn test_markdown_to_slack_bold() {
        let got = markdown_to_slack("this is **bold** text");
        assert!(!got.contains("**"), "bold markers should be removed");
        assert!(
            !got.contains("\u{00a7}BS\u{00a7}"),
            "bold placeholders should be resolved"
        );
    }

    #[test]
    fn test_markdown_to_slack_italic() {
        let got = markdown_to_slack("this is *italic* text");
        assert!(!got.contains('*'), "asterisks should be removed");
    }

    #[test]
    fn test_markdown_to_slack_code_block() {
        let got = markdown_to_slack("text\n```\ncode\n```\nmore");
        assert!(
            got.contains("code"),
            "code content should be preserved: {}",
            got
        );
    }

    #[test]
    fn test_markdown_to_slack_list() {
        let got = markdown_to_slack("- item one\n- item two");
        assert!(
            got.contains("item one"),
            "list items should be preserved: {}",
            got
        );
    }
}
