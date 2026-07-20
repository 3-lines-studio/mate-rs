use crate::tools::Tool;
use crate::tools::define_tool;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct EditParams {
    pub path: String,
    pub edits: Vec<EditOp>,
}

#[derive(Debug, Deserialize)]
pub struct EditOp {
    #[serde(rename = "oldText")]
    pub old_text: String,
    #[serde(rename = "newText")]
    pub new_text: String,
}

pub fn tool() -> Tool {
    let params = crate::tools::object_schema(
        &[
            (
                "path",
                serde_json::json!({"type": "string", "description": "Path to the file to edit (relative or absolute)"}),
            ),
            (
                "edits",
                serde_json::json!({
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "oldText": {"type": "string", "description": "Exact text to replace"},
                            "newText": {"type": "string", "description": "Replacement text"}
                        },
                        "required": ["oldText", "newText"]
                    }
                }),
            ),
        ],
        &["path", "edits"],
    );

    define_tool(
        "edit_file",
        "Edit a file using exact text replacement. Each oldText must be unique in the file. Multiple edits are applied in order.",
        params,
        |p: EditParams| async move { execute_edit(p) },
    )
}

fn execute_edit(p: EditParams) -> Result<String, String> {
    let data =
        std::fs::read_to_string(&p.path).map_err(|e| format!("read file {}: {}", p.path, e))?;

    if p.edits.is_empty() {
        crate::tools::write_file::atomic_write(&p.path, data.as_bytes())
            .map_err(|e| format!("write file {}: {}", p.path, e))?;
        return Ok(format!("Applied 0 edit(s) to {}", p.path));
    }

    let mut ranges: Vec<(usize, usize, usize)> = Vec::new();
    let mut applied = 0;

    for (idx, edit) in p.edits.iter().enumerate() {
        let matches: Vec<_> = data.match_indices(&edit.old_text).collect();
        let count = matches.len();
        if count == 0 {
            let mut msg = format!("oldText not found in {}", p.path);
            if let Some(hint) = find_similar_context(&data, &edit.old_text) {
                msg.push_str(&format!("\n\nDid you mean:\n```\n{}\n```", hint));
            }
            msg.push_str(&format!(
                "\n\nFile preview:\n```\n{}\n```",
                build_file_preview(&data, 20)
            ));
            return Err(msg);
        }
        if count > 1 {
            let mut msg = format!(
                "oldText found {} times in {}, must be unique. Matches:",
                count, p.path
            );
            for (i, (pos, _)) in matches.iter().enumerate().take(2) {
                let line_num = count_lines_before(&data, *pos);
                msg.push_str(&format!(
                    "\n\nMatch {} (line {}):\n```\n{}\n```",
                    i + 1,
                    line_num,
                    get_line_context(&data, line_num, 1)
                ));
            }
            if count > 2 {
                msg.push_str(&format!("\n\n...and {} more", count - 2));
            }
            return Err(msg);
        }
        let (start, _) = matches[0];
        let end = start + edit.old_text.len();
        ranges.push((start, end, idx));
        applied += 1;
    }

    for i in 0..ranges.len() {
        for j in (i + 1)..ranges.len() {
            let (s1, e1, _) = ranges[i];
            let (s2, e2, _) = ranges[j];
            if s1 < e2 && s2 < e1 {
                return Err(format!("edits overlap in {}", p.path));
            }
        }
    }

    ranges.sort_by_key(|r| std::cmp::Reverse(r.0));
    let mut result = data;
    for (start, end, idx) in &ranges {
        let edit = &p.edits[*idx];
        let left = &result[..*start];
        let right = &result[*end..];
        result = left.to_string() + &edit.new_text + right;
    }

    crate::tools::write_file::atomic_write(&p.path, result.as_bytes())
        .map_err(|e| format!("write file {}: {}", p.path, e))?;

    Ok(format!("Applied {} edit(s) to {}", applied, p.path))
}

fn count_lines_before(content: &str, byte_pos: usize) -> usize {
    content
        .char_indices()
        .take_while(|(i, _)| *i < byte_pos)
        .filter(|(_, c)| *c == '\n')
        .count()
        + 1
}

fn get_line_context(content: &str, target_line: usize, context: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start = target_line.saturating_sub(context + 1);
    let end = (target_line + context).min(lines.len());
    lines[start..end].join("\n")
}

fn find_similar_context(content: &str, search: &str) -> Option<String> {
    let first_line = search.lines().next()?.trim();
    if first_line.is_empty() {
        return None;
    }
    for (i, line) in content.lines().enumerate() {
        if line.contains(first_line) || first_line.contains(line.trim()) {
            return Some(get_line_context(content, i + 1, 2));
        }
    }
    None
}

fn build_file_preview(content: &str, max_lines: usize) -> String {
    if content.is_empty() {
        return "(file is empty)".to_string();
    }
    let lines: Vec<&str> = content.lines().collect();
    let preview_end = lines.len().min(max_lines);
    let mut preview = lines[..preview_end]
        .iter()
        .enumerate()
        .map(|(index, line)| format!("{:>4}: {}", index + 1, line))
        .collect::<Vec<_>>()
        .join("\n");
    if lines.len() > preview_end {
        preview.push_str(&format!("\n... ({} more lines)", lines.len() - preview_end));
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_file_single_edit() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "hello world").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "hello".to_string(),
                new_text: "hi".to_string(),
            }],
        })
        .unwrap();
        assert!(result.contains("Applied 1"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hi world");
    }

    #[test]
    fn test_edit_file_multiple_edits() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "foo bar baz").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![
                EditOp {
                    old_text: "foo".to_string(),
                    new_text: "1".to_string(),
                },
                EditOp {
                    old_text: "bar".to_string(),
                    new_text: "2".to_string(),
                },
                EditOp {
                    old_text: "baz".to_string(),
                    new_text: "3".to_string(),
                },
            ],
        })
        .unwrap();
        assert!(result.contains("Applied 3"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1 2 3");
    }

    #[test]
    fn test_edit_file_old_text_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "hello world").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "nope".to_string(),
                new_text: "x".to_string(),
            }],
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_edit_file_old_text_not_unique() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "hello hello").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "hello".to_string(),
                new_text: "x".to_string(),
            }],
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_edit_file_not_found() {
        let result = execute_edit(EditParams {
            path: "/nonexistent/edit.txt".to_string(),
            edits: vec![EditOp {
                old_text: "a".to_string(),
                new_text: "b".to_string(),
            }],
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_edit_file_sequential_dependency() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "replace me please").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "replace me".to_string(),
                new_text: "edited".to_string(),
            }],
        })
        .unwrap();
        assert!(result.contains("Applied 1"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited please");
    }

    #[test]
    fn test_edit_file_empty_edits() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "content").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![],
        })
        .unwrap();
        assert!(result.contains("Applied 0"));
    }

    #[test]
    fn test_edit_file_preserves_content_outside_edit() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "line1\nline2\nline3\n").unwrap();

        execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "line2".to_string(),
                new_text: "middle".to_string(),
            }],
        })
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "line1\nmiddle\nline3\n"
        );
    }

    #[test]
    fn test_edit_file_no_match_with_similar_hint() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "line1\nhello world\nline3\n").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "hello world extra".to_string(),
                new_text: "x".to_string(),
            }],
        });
        let err = result.unwrap_err();
        assert!(err.contains("Did you mean"));
    }

    #[test]
    fn test_edit_file_no_match_file_preview() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "line1\nline2\nline3\n").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "zzzzzzz".to_string(),
                new_text: "x".to_string(),
            }],
        });
        let err = result.unwrap_err();
        assert!(err.contains("File preview"));
        assert!(err.contains("   1: line1"));
    }

    #[test]
    fn test_edit_file_empty_file_preview() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "a".to_string(),
                new_text: "b".to_string(),
            }],
        });
        let err = result.unwrap_err();
        assert!(err.contains("(file is empty)"));
    }

    #[test]
    fn test_edit_file_multiple_matches_context() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "foo\nbar\nfoo\nbaz\nfoo\n").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "foo".to_string(),
                new_text: "x".to_string(),
            }],
        });
        let err = result.unwrap_err();
        assert!(err.contains("Match 1 (line 1)"));
        assert!(err.contains("Match 2"));
    }

    #[test]
    fn test_edit_file_multiple_matches_more() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "x\nx\nx\nx\nx\n").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "x".to_string(),
                new_text: "y".to_string(),
            }],
        });
        let err = result.unwrap_err();
        assert!(err.contains("...and 3 more"));
    }

    #[test]
    fn test_edit_file_overlapping_edits_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "abcdef").unwrap();

        let result = execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![
                EditOp {
                    old_text: "abcd".to_string(),
                    new_text: "X".to_string(),
                },
                EditOp {
                    old_text: "cdef".to_string(),
                    new_text: "Y".to_string(),
                },
            ],
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("overlap"));
    }

    #[test]
    fn test_edit_file_preserves_mode() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("script.sh");
        std::fs::write(&path, "#!/bin/sh\necho hi").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        execute_edit(EditParams {
            path: path.to_string_lossy().to_string(),
            edits: vec![EditOp {
                old_text: "hi".to_string(),
                new_text: "hello".to_string(),
            }],
        })
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&path).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o755);
        }
    }
}
