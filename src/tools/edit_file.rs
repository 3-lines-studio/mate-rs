use crate::tools::define_tool;
use crate::tools::Tool;
use serde::Deserialize;
use std::collections::HashMap;

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
    let mut params = HashMap::new();
    params.insert("type".to_string(), serde_json::json!("object"));
    let mut properties: HashMap<String, serde_json::Value> = HashMap::new();
    properties.insert(
        "path".to_string(),
        serde_json::json!({"type": "string", "description": "Path to the file to edit (relative or absolute)"}),
    );
    properties.insert(
        "edits".to_string(),
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
    );
    params.insert("properties".to_string(), serde_json::json!(properties));
    params.insert("required".to_string(), serde_json::json!(["path", "edits"]));

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

    let mut content = data;
    let mut applied = 0;

    for edit in &p.edits {
        let count = content.matches(&edit.old_text).count();
        if count == 0 {
            return Err(format!(
                "oldText not found in {}: {:?}",
                p.path, edit.old_text
            ));
        }
        if count > 1 {
            return Err(format!(
                "oldText found {} times in {}, must be unique: {:?}",
                count, p.path, edit.old_text
            ));
        }
        content = content.replacen(&edit.old_text, &edit.new_text, 1);
        applied += 1;
    }

    crate::tools::write_file::atomic_write(&p.path, content.as_bytes())
        .map_err(|e| format!("write file {}: {}", p.path, e))?;

    Ok(format!("Applied {} edit(s) to {}", applied, p.path))
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
