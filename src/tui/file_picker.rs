use crate::tools::gitignore::GitignoreMatcher;
use walkdir::WalkDir;

/// Index files in the given root directory, respecting gitignore.
///
/// Single pass: parses each directory's `.gitignore` as the walk descends, so
/// there is no separate discovery walk.
pub fn index_files(root: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut ig = GitignoreMatcher::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                let name_l = name.to_ascii_lowercase();
                if e.depth() > 0
                    && (crate::tools::gitignore::should_skip_dir(&name_l) || name_l == "target")
                {
                    return false;
                }
                let rel = e.path().strip_prefix(root).unwrap_or(e.path());
                let rel_str = rel.to_string_lossy();
                if rel_str != "." && ig.is_ignored(rel_str.as_ref(), true) {
                    return false;
                }
                if rel_str != "." {
                    files.push(format!("{}/", rel_str));
                }
                let dir = if rel_str == "." {
                    String::new()
                } else {
                    rel_str.into_owned()
                };
                let gi = e.path().join(".gitignore");
                ig.add_file(&gi.to_string_lossy(), &dir);
                return true;
            }
            let rel = e.path().strip_prefix(root).unwrap_or(e.path());
            let rel_str = rel.to_string_lossy();
            if ig.is_ignored(rel_str.as_ref(), false) {
                return false;
            }
            files.push(rel_str.into_owned());
            true
        })
    {
        let _ = entry;
    }
    files.sort_unstable();
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

    #[test]
    fn test_index_files_respects_gitignore() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".gitignore"), "*.log\nignored/\n").unwrap();
        fs::write(dir.path().join("main.rs"), "").unwrap();
        fs::write(dir.path().join("debug.log"), "").unwrap();
        let igd = dir.path().join("ignored");
        fs::create_dir_all(&igd).unwrap();
        fs::write(igd.join("secret.txt"), "").unwrap();

        let files = index_files(&dir.path().to_string_lossy());
        assert!(files.contains(&"main.rs".to_string()));
        assert!(!files.iter().any(|f| f.ends_with(".log")));
        assert!(!files.iter().any(|f| f.starts_with("ignored")));
    }

    #[test]
    fn test_index_files_nested_gitignore() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("src");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join(".gitignore"), "*.tmp\n").unwrap();
        fs::write(sub.join("keep.rs"), "").unwrap();
        fs::write(sub.join("drop.tmp"), "").unwrap();
        fs::write(dir.path().join("keep.tmp"), "").unwrap();

        let files = index_files(&dir.path().to_string_lossy());
        assert!(files.contains(&"src/keep.rs".to_string()));
        assert!(!files.contains(&"src/drop.tmp".to_string()));
        assert!(files.contains(&"keep.tmp".to_string()));
    }
}
