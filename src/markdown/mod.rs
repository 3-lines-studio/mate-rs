pub mod slack;
pub mod telegram;

use once_cell::sync::Lazy;
use regex::Regex;

static RE_CODE_BLOCK_GLOBAL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)```[\s\S]*?```").unwrap());

struct CodeRange {
    start: usize,
    end: usize,
}

fn find_code_block_ranges(text: &str) -> Vec<CodeRange> {
    RE_CODE_BLOCK_GLOBAL
        .find_iter(text)
        .map(|m| CodeRange {
            start: m.start(),
            end: m.end(),
        })
        .collect()
}

fn last_index(s: &str, sep: &str) -> Option<usize> {
    if sep.is_empty() {
        Some(s.len())
    } else {
        s.rfind(sep)
    }
}

pub fn split_text(text: &str, max_len: usize) -> Vec<String> {
    if text.is_empty() || text.len() <= max_len {
        if text.is_empty() {
            return Vec::new();
        }
        return vec![text.to_string()];
    }

    let block_ranges = find_code_block_ranges(text);

    let is_inside =
        |idx: usize| -> bool { block_ranges.iter().any(|r| idx > r.start && idx < r.end) };

    let mut chunks: Vec<String> = Vec::new();
    let mut pos = 0;
    let bytes = text.as_bytes();

    while pos < text.len() {
        if pos + max_len >= text.len() {
            let chunk = text[pos..].trim_matches('\n');
            if !chunk.is_empty() {
                chunks.push(chunk.to_string());
            }
            break;
        }

        let search_end = pos + max_len;
        let mut split_at: Option<usize> = None;

        for sep in ["\n\n", "\n"] {
            let mut idx = last_index(&text[..search_end], sep);
            while idx.is_some_and(|i| i > pos) {
                let i = idx.unwrap();
                if !is_inside(i) {
                    split_at = Some(i);
                    break;
                }
                idx = last_index(&text[..i], sep);
            }
            if split_at.is_some() {
                break;
            }
        }

        let split_at = split_at.unwrap_or_else(|| {
            let mut containing_end = None;
            for r in &block_ranges {
                if search_end > r.start && search_end < r.end {
                    containing_end = Some(r.end);
                    break;
                }
            }
            if let Some(end) = containing_end {
                let after = &bytes[end..];
                if let Some(nl_pos) = after.iter().position(|&b| b == b'\n') {
                    end + nl_pos
                } else {
                    end
                }
            } else {
                search_end
            }
        });

        let chunk = text[pos..split_at].trim_matches('\n');
        if !chunk.is_empty() {
            chunks.push(chunk.to_string());
        }

        pos = split_at;
        while pos < text.len() && bytes[pos] == b'\n' {
            pos += 1;
        }
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_code_block_ranges_empty() {
        let ranges = find_code_block_ranges("no code here");
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_find_code_block_ranges_one() {
        let ranges = find_code_block_ranges("before\n```go\ncode\n```\nafter");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 7);
    }

    #[test]
    fn test_find_code_block_ranges_multiple() {
        let ranges = find_code_block_ranges("```a\nx\n```\n---\n```b\ny\n```");
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn test_last_index_found() {
        assert_eq!(last_index("hello world hello", "hello"), Some(12));
    }

    #[test]
    fn test_last_index_not_found() {
        assert_eq!(last_index("hello world", "xyz"), None);
    }

    #[test]
    fn test_last_index_empty_sep() {
        assert_eq!(last_index("hello", ""), Some(5));
    }

    #[test]
    fn test_split_text_short() {
        let got = split_text("hello", 100);
        assert_eq!(got, vec!["hello"]);
    }

    #[test]
    fn test_split_text_empty() {
        let got: Vec<String> = split_text("", 100);
        assert!(got.is_empty());
    }

    #[test]
    fn test_split_text_splits() {
        let got = split_text("line1\n\nline2\n\nline3", 6);
        assert!(got.len() >= 2, "expected multiple chunks, got {:?}", got);
    }
}
