use walkdir::WalkDir;

/// Index files in the given root directory, respecting gitignore.
pub fn index_files(root: &str) -> Vec<String> {
    let mut files = Vec::new();
    let ig = crate::tools::gitignore::parse_gitignore(root);

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            if e.file_type().is_dir() {
                if name == ".git" || name == "node_modules" || name == "vendor" || name == "target"
                {
                    return false;
                }
                let rel = e.path().strip_prefix(root).unwrap_or(e.path());
                let rel_str = rel.to_string_lossy().to_string();
                if rel_str != "." && ig.is_ignored(&rel_str, true) {
                    return false;
                }
                if rel_str != "." {
                    files.push(format!("{}/", rel_str));
                }
                return true;
            }
            let rel = e.path().strip_prefix(root).unwrap_or(e.path());
            let rel_str = rel.to_string_lossy().to_string();
            if ig.is_ignored(&rel_str, false) {
                return false;
            }
            files.push(rel_str);
            true
        })
    {
        let _ = entry;
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_index_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("lib.rs"), "").unwrap();
        let sub = dir.path().join("src");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("mod.rs"), "").unwrap();

        let files = index_files(&dir.path().to_string_lossy());
        assert!(files.contains(&"main.rs".to_string()));
        assert!(files.contains(&"lib.rs".to_string()));
    }
}
