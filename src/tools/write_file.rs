use crate::tools::define_tool;
use crate::tools::Tool;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct WriteFileParams {
    pub path: String,
    pub content: String,
}

pub fn tool() -> Tool {
    let params = crate::tools::object_schema(
        &[
            (
                "path",
                serde_json::json!({"type": "string", "description": "Path to the file to write (relative or absolute)"}),
            ),
            (
                "content",
                serde_json::json!({"type": "string", "description": "Content to write to the file"}),
            ),
        ],
        &["path", "content"],
    );

    define_tool(
        "write_file",
        "Create or overwrite a file with the given content. Creates parent directories automatically.",
        params,
        |p: WriteFileParams| async move { execute_write(p) },
    )
}

fn execute_write(p: WriteFileParams) -> Result<String, String> {
    let dir = Path::new(&p.path)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("create parent directories for {}: {}", p.path, e))?;

    let len = p.content.len();
    atomic_write(&p.path, p.content.as_bytes())
        .map_err(|e| format!("write file {}: {}", p.path, e))?;

    Ok(format!("Wrote {} bytes to {}", len, p.path))
}

pub(crate) fn atomic_write(path_str: &str, data: &[u8]) -> Result<(), String> {
    let path = Path::new(path_str);

    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    let perm = if let Ok(meta) = std::fs::symlink_metadata(&resolved) {
        // Follow symlink to get target permissions
        if meta.file_type().is_symlink() {
            if let Ok(target_meta) = std::fs::metadata(&resolved) {
                target_meta.permissions()
            } else {
                // Broken symlink: use default
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::Permissions::from_mode(0o644)
                }
                #[cfg(not(unix))]
                {
                    std::fs::Permissions::new()
                }
            }
        } else {
            meta.permissions()
        }
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::Permissions::from_mode(0o644)
        }
        #[cfg(not(unix))]
        {
            std::fs::Permissions::new()
        }
    };

    let dir = resolved.parent().unwrap_or_else(|| Path::new("."));

    let mut tmp = tempfile::Builder::new()
        .prefix(".mate-tmp-")
        .tempfile_in(dir)
        .map_err(|e| format!("temp file: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file_mut()
            .set_permissions(std::fs::Permissions::from_mode(perm.mode()))
            .map_err(|e| format!("chmod: {}", e))?;
    }

    std::io::Write::write_all(&mut tmp, data).map_err(|e| format!("write tmp: {}", e))?;
    tmp.as_file_mut()
        .sync_all()
        .map_err(|e| format!("sync: {}", e))?;

    let tmp_path = tmp.path().to_path_buf();
    std::fs::rename(&tmp_path, &resolved).map_err(|e| format!("rename: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_file_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("out.txt");

        let result = execute_write(WriteFileParams {
            path: path.to_string_lossy().to_string(),
            content: "hello".to_string(),
        })
        .unwrap();
        assert!(result.contains("Wrote"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn test_write_file_creates_parent_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sub").join("deep").join("out.txt");

        execute_write(WriteFileParams {
            path: path.to_string_lossy().to_string(),
            content: "nested".to_string(),
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "nested");
    }

    #[test]
    fn test_write_file_overwrites() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("out.txt");
        std::fs::write(&path, "original").unwrap();

        execute_write(WriteFileParams {
            path: path.to_string_lossy().to_string(),
            content: "updated".to_string(),
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "updated");
    }

    #[test]
    fn test_write_file_empty_content() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("empty.txt");

        let result = execute_write(WriteFileParams {
            path: path.to_string_lossy().to_string(),
            content: String::new(),
        })
        .unwrap();
        assert!(result.contains("0 bytes"));
    }

    #[test]
    fn test_write_file_binary_content() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bin.bin");

        let _content = String::from_utf8(vec![0, 1, 2, 3]).unwrap_or_else(|e| {
            let v = e.into_bytes();
            unsafe { String::from_utf8_unchecked(v) }
        });
        // Use raw bytes
        let content_bytes: Vec<u8> = vec![0, 1, 2, 3];
        let content = unsafe { String::from_utf8_unchecked(content_bytes.clone()) };
        execute_write(WriteFileParams {
            path: path.to_string_lossy().to_string(),
            content,
        })
        .unwrap();
        let data = std::fs::read(&path).unwrap();
        assert_eq!(data.len(), 4);
    }

    #[test]
    fn test_write_file_preserves_mode() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("script.sh");
        std::fs::write(&path, "#!/bin/sh\necho hi").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        execute_write(WriteFileParams {
            path: path.to_string_lossy().to_string(),
            content: "#!/bin/sh\necho hello".to_string(),
        })
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&path).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o755);
        }
    }

    #[test]
    fn test_write_file_default_mode_new_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("new.txt");

        execute_write(WriteFileParams {
            path: path.to_string_lossy().to_string(),
            content: "hello".to_string(),
        })
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&path).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o644);
        }
    }
}
