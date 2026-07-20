use crate::tools::Tool;
use crate::tools::define_tool;
use serde::Deserialize;
use std::io::BufRead;

#[derive(Debug, Deserialize)]
pub struct ReadFileParams {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub offset: i32,
    #[serde(default)]
    pub limit: i32,
}

const DEFAULT_READ_LIMIT: i32 = 500;

pub fn tool() -> Tool {
    let params = crate::tools::object_schema(
        &[
            (
                "path",
                serde_json::json!({"type": "string", "description": "Path to the file to read (relative or absolute)"}),
            ),
            (
                "offset",
                serde_json::json!({"type": "integer", "description": "Line number to start reading from (1-indexed)"}),
            ),
            (
                "limit",
                serde_json::json!({"type": "integer", "description": "Maximum number of lines to read"}),
            ),
        ],
        &["path"],
    );

    define_tool(
        "read_file",
        "Read contents of a file. Supports text files. When no offset/limit is specified, returns up to 500 lines. Use offset/limit for large files.",
        params,
        |mut p: ReadFileParams| async move {
            if p.offset < 1 {
                p.offset = 1;
            }
            if p.limit <= 0 {
                p.limit = DEFAULT_READ_LIMIT;
            }
            read_file_lines(&p).map(|(content, _)| content)
        },
    )
}

fn read_file_lines(p: &ReadFileParams) -> Result<(String, i32), String> {
    let f = std::fs::File::open(&p.path).map_err(|e| format!("read file {}: {}", p.path, e))?;
    let mut reader = std::io::BufReader::with_capacity(1024 * 1024, f);

    let mut out: Vec<u8> = Vec::new();
    let mut line_num: i32 = 0;
    let start = p.offset - 1;
    let end = start + p.limit;
    let mut buf = Vec::with_capacity(1024);

    loop {
        buf.clear();
        let n = reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| format!("read file {}: {}", p.path, e))?;
        if n == 0 {
            break;
        }
        line_num += 1;
        if line_num <= start {
            continue;
        }
        if p.limit > 0 && line_num > end {
            break;
        }
        out.extend_from_slice(&buf);
    }

    if out.is_empty() && p.offset > line_num {
        return Ok((String::new(), line_num));
    }

    Ok((String::from_utf8_lossy(&out).into_owned(), line_num))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_file_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "line1\nline2\nline3\n").unwrap();

        let (content, _) = read_file_lines(&ReadFileParams {
            path: path.to_string_lossy().to_string(),
            offset: 0,
            limit: 0,
        })
        .unwrap();
        assert!(content.contains("line1"));
        assert!(content.contains("line2"));
        assert!(content.contains("line3"));
    }

    #[test]
    fn test_read_file_not_found() {
        let result = read_file_lines(&ReadFileParams {
            path: "/nonexistent/file.txt".to_string(),
            offset: 0,
            limit: 0,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_read_file_with_offset() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("big.txt");
        let lines: Vec<String> = (0..10).map(|_| "line".to_string()).collect();
        std::fs::write(&path, lines.join("\n")).unwrap();

        let (content, _) = read_file_lines(&ReadFileParams {
            path: path.to_string_lossy().to_string(),
            offset: 3,
            limit: 2,
        })
        .unwrap();
        let got: Vec<&str> = content.lines().collect();
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn test_read_file_offset_zero() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "a\nb\nc\n").unwrap();

        let (content, _) = read_file_lines(&ReadFileParams {
            path: path.to_string_lossy().to_string(),
            offset: 0,
            limit: 0,
        })
        .unwrap();
        assert!(content.starts_with('a'));
    }

    #[test]
    fn test_read_file_offset_beyond_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("short.txt");
        std::fs::write(&path, "only\n").unwrap();

        let (content, _) = read_file_lines(&ReadFileParams {
            path: path.to_string_lossy().to_string(),
            offset: 100,
            limit: 0,
        })
        .unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn test_read_file_default_limit() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("big.txt");
        let lines: Vec<String> = (0..600).map(|_| "x".to_string()).collect();
        std::fs::write(&path, lines.join("\n")).unwrap();

        let tool = tool();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((tool.execute)(
            serde_json::json!({"path": path.to_string_lossy()}),
        ));
        let content = result.unwrap();
        let count = content.lines().count() as i32;
        assert_eq!(count, 500);
    }

    #[test]
    fn test_read_file_empty_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("empty.txt");
        std::fs::write(&path, "").unwrap();

        let (content, _) = read_file_lines(&ReadFileParams {
            path: path.to_string_lossy().to_string(),
            offset: 0,
            limit: 0,
        })
        .unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn test_read_file_offset_with_limit_zero() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "a\nb\nc\n").unwrap();

        let (content, _) = read_file_lines(&ReadFileParams {
            path: path.to_string_lossy().to_string(),
            offset: 2,
            limit: 0,
        })
        .unwrap();
        assert_eq!(content, "b\nc\n");
    }

    #[test]
    fn test_read_file_negative_offset() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "a\nb\n").unwrap();

        let (content, _) = read_file_lines(&ReadFileParams {
            path: path.to_string_lossy().to_string(),
            offset: -5,
            limit: 0,
        })
        .unwrap();
        assert!(content.contains('a'));
    }

    #[test]
    fn test_read_file_directory_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = read_file_lines(&ReadFileParams {
            path: dir.path().to_string_lossy().to_string(),
            offset: 0,
            limit: 0,
        });
        assert!(result.is_err());
    }
}
