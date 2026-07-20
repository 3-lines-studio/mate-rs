use crate::tools::Tool;
use crate::tools::define_tool;
use crate::tools::gitignore::{parse_gitignore, walk_files};
use regex::Regex;
use serde::Deserialize;
use std::io::BufRead;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct GrepParams {
    pub pattern: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub glob: String,
    #[serde(default)]
    pub max_results: i32,
    #[serde(default)]
    pub regex: bool,
}

const MAX_LINE_LENGTH: usize = 64 * 1024;
const MAX_MATCHES_PER_FILE: i32 = 1000;

pub fn tool() -> Tool {
    let params = crate::tools::object_schema(
        &[
            (
                "pattern",
                serde_json::json!({"type": "string", "description": "Text or regex pattern to search for"}),
            ),
            (
                "path",
                serde_json::json!({"type": "string", "description": "File or directory to search in (default: current working directory)"}),
            ),
            (
                "glob",
                serde_json::json!({"type": "string", "description": "Filter file names, e.g. \"*.go\", \"*_test.go\""}),
            ),
            (
                "max_results",
                serde_json::json!({"type": "integer", "description": "Maximum matches to return (default: 30)"}),
            ),
            (
                "regex",
                serde_json::json!({"type": "boolean", "description": "Treat pattern as regex (default: false, literal match)"}),
            ),
        ],
        &["pattern"],
    );

    define_tool(
        "grep",
        "Search for a pattern in files. Returns matching lines with file path and line number. Skips binary files and common VCS/dependency directories.",
        params,
        |p: GrepParams| async move { execute_grep(p) },
    )
}

fn execute_grep(mut p: GrepParams) -> Result<String, String> {
    if p.path.is_empty() {
        p.path = ".".to_string();
    }
    if p.max_results <= 0 {
        p.max_results = 30;
    }

    let matcher = build_matcher(&p.pattern, p.regex)?;

    let path = Path::new(&p.path);
    let meta = std::fs::metadata(path).map_err(|e| format!("grep: {}", e))?;

    if !meta.is_dir() {
        if is_binary_file(path) {
            return Ok(String::new());
        }
        return grep_file(path, &matcher, p.max_results);
    }

    let ig = parse_gitignore(&p.path);

    let mut results: Vec<String> = Vec::new();
    let max = p.max_results as usize;

    let glob_matcher = if p.glob.is_empty() {
        None
    } else {
        Some(
            globset::GlobBuilder::new(&p.glob)
                .literal_separator(true)
                .build()
                .map_err(|e| format!("grep: invalid glob: {}", e))?
                .compile_matcher(),
        )
    };

    walk_files(path, &ig, &[], &mut |full_path, _rel| {
        if let Some(ref gm) = glob_matcher {
            let fname = full_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !gm.is_match(&fname) {
                return true;
            }
        }
        if is_binary_file(full_path) {
            return true;
        }
        let remaining = max - results.len();
        if remaining == 0 {
            return false;
        }
        if let Ok(m) = grep_file(full_path, &*matcher, remaining as i32)
            && !m.is_empty()
        {
            results.push(m);
        }
        results.len() < max
    });

    Ok(results.join("\n"))
}

#[allow(clippy::type_complexity)]
fn build_matcher(pattern: &str, is_regex: bool) -> Result<Box<dyn Fn(&str) -> bool>, String> {
    if is_regex {
        let re = Regex::new(pattern).map_err(|e| format!("invalid regex: {}", e))?;
        Ok(Box::new(move |line: &str| re.is_match(line)))
    } else {
        let pat = pattern.to_string();
        Ok(Box::new(move |line: &str| line.contains(&pat)))
    }
}

fn grep_file(
    path: &Path,
    matcher: &dyn Fn(&str) -> bool,
    max_results: i32,
) -> Result<String, String> {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Ok(String::new()),
    };

    let max_results = if max_results > MAX_MATCHES_PER_FILE {
        MAX_MATCHES_PER_FILE
    } else {
        max_results
    };

    let reader = std::io::BufReader::with_capacity(MAX_LINE_LENGTH * 2, f);
    let mut results: Vec<String> = Vec::new();
    let mut line_num = 0;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.len() > MAX_LINE_LENGTH {
            continue;
        }

        line_num += 1;

        if matcher(&line) {
            let display = path.to_string_lossy();
            let display = display.strip_prefix("./").unwrap_or(&display);
            results.push(format!("{}:{}: {}", display, line_num, line));
            if results.len() >= max_results as usize {
                break;
            }
        }
    }

    Ok(results.join("\n"))
}

fn is_binary_file(path: &Path) -> bool {
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return true,
    };
    use std::io::Read;
    let mut buf = [0u8; 8192];
    let n = f.read(&mut buf).unwrap_or(0);
    buf[..n].contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grep_literal_match() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("a.go"),
            "package p\n\nfunc Handle() {}\nfunc handleError() {}\n",
        )
        .unwrap();

        let result = execute_grep(GrepParams {
            pattern: "Handle".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 0,
            regex: false,
        })
        .unwrap();
        assert!(result.contains("Handle"));
        assert!(!result.contains("handleError"));
    }

    #[test]
    fn test_grep_regex_match() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("a.go"),
            "package p\n\nfunc Handle() {}\nfunc handler() {}\nfunc Banana() {}\n",
        )
        .unwrap();

        let result = execute_grep(GrepParams {
            pattern: r"func [A-Z]\w+".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 0,
            regex: true,
        })
        .unwrap();
        assert!(result.contains("Handle"));
        assert!(!result.contains("handler"));
        assert!(result.contains("Banana"));
    }

    #[test]
    fn test_grep_glob_filter() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.go"), "hello").unwrap();
        std::fs::write(dir.path().join("b_test.go"), "hello test").unwrap();
        std::fs::write(dir.path().join("c.md"), "hello markdown").unwrap();

        let result = execute_grep(GrepParams {
            pattern: "hello".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: "*_test.go".to_string(),
            max_results: 0,
            regex: false,
        })
        .unwrap();
        assert!(!result.contains("a.go"));
        assert!(result.contains("b_test.go"));
        assert!(!result.contains("c.md"));
    }

    #[test]
    fn test_grep_single_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("main.go");
        std::fs::write(&path, "line one\nline two\nline three\n").unwrap();

        let result = execute_grep(GrepParams {
            pattern: "two".to_string(),
            path: path.to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 0,
            regex: false,
        })
        .unwrap();
        assert!(result.contains("line two"));
    }

    #[test]
    fn test_grep_max_results() {
        let dir = tempfile::TempDir::new().unwrap();
        let lines: Vec<String> = (0..10)
            .map(|i| format!("line {}", (b'a' + i) as char))
            .collect();
        std::fs::write(dir.path().join("a.go"), lines.join("\n")).unwrap();

        let result = execute_grep(GrepParams {
            pattern: "line".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 3,
            regex: false,
        })
        .unwrap();
        assert_eq!(result.lines().count(), 3);
    }

    #[test]
    fn test_grep_no_match() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.go"), "hello world").unwrap();

        let result = execute_grep(GrepParams {
            pattern: "NONEXISTENT".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 0,
            regex: false,
        })
        .unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_grep_skips_binary() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.go"), "hello\x00world").unwrap();

        let result = execute_grep(GrepParams {
            pattern: "hello".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 0,
            regex: false,
        })
        .unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_grep_skips_vcs_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        let git_dir = dir.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("config"), "hello world").unwrap();

        let result = execute_grep(GrepParams {
            pattern: "hello".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 0,
            regex: false,
        })
        .unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_grep_line_numbers() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.go"), "package p\n\nfunc main() {}\n").unwrap();

        let result = execute_grep(GrepParams {
            pattern: "func".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 0,
            regex: false,
        })
        .unwrap();
        assert!(result.contains(":3:"));
    }

    #[test]
    fn test_grep_invalid_regex() {
        let result = execute_grep(GrepParams {
            pattern: "[unclosed".to_string(),
            path: ".".to_string(),
            glob: String::new(),
            max_results: 0,
            regex: true,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_grep_respects_gitignore() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "ignored_dir/\n*.log\n").unwrap();
        let ignored_dir = dir.path().join("ignored_dir");
        std::fs::create_dir_all(&ignored_dir).unwrap();
        std::fs::write(ignored_dir.join("secret.go"), "hello world").unwrap();
        std::fs::write(dir.path().join("debug.log"), "hello world").unwrap();
        std::fs::write(dir.path().join("main.go"), "hello world").unwrap();

        let result = execute_grep(GrepParams {
            pattern: "hello".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 0,
            regex: false,
        })
        .unwrap();
        assert!(!result.contains("ignored_dir"));
        assert!(!result.contains("debug.log"));
        assert!(result.contains("main.go"));
    }

    #[test]
    fn test_grep_max_results_stops_early() {
        let dir = tempfile::TempDir::new().unwrap();
        for i in 0..50 {
            std::fs::write(dir.path().join(format!("f{}.txt", i)), "match").unwrap();
        }

        let result = execute_grep(GrepParams {
            pattern: "match".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 3,
            regex: false,
        })
        .unwrap();
        assert_eq!(result.lines().count(), 3);
    }

    #[test]
    fn test_grep_respects_gitignore_nested() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "*.go\n!important.go\n").unwrap();
        std::fs::write(dir.path().join("main.go"), "hello").unwrap();
        std::fs::write(dir.path().join("important.go"), "hello").unwrap();

        let result = execute_grep(GrepParams {
            pattern: "hello".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            glob: String::new(),
            max_results: 0,
            regex: false,
        })
        .unwrap();
        assert!(!result.contains("main.go"));
        assert!(result.contains("important.go"));
    }
}
