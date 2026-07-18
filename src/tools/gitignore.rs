use globset::Glob;
use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;
use std::time::SystemTime;

#[derive(Debug, Clone)]
struct GitignoreRule {
    pattern: String,
    negate: bool,
    dir_only: bool,
    anchored: bool,
    dir: String,
    glob: Option<Glob>,
}

#[derive(Debug, Clone)]
pub struct GitignoreMatcher {
    rules: Vec<GitignoreRule>,
}

struct CacheEntry {
    matcher: GitignoreMatcher,
    modtimes: HashMap<String, SystemTime>,
}

static CACHE: once_cell::sync::Lazy<RwLock<HashMap<String, CacheEntry>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

pub(crate) fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "vendor" | ".idea" | ".vscode" | "__pycache__" | ".pytest_cache"
    )
}

pub fn walk_files(
    base: &Path,
    ig: &GitignoreMatcher,
    extra_skip: &[&str],
    on_file: &mut dyn FnMut(&Path, &str) -> bool,
) {
    walk_files_impl(base, base, ig, extra_skip, on_file);
}

fn walk_files_impl(
    base: &Path,
    dir: &Path,
    ig: &GitignoreMatcher,
    extra_skip: &[&str],
    on_file: &mut dyn FnMut(&Path, &str) -> bool,
) -> bool {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return false,
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let rel = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if should_skip_dir(&name) {
                continue;
            }
            if extra_skip.contains(&name.as_str()) {
                continue;
            }
            if ig.is_ignored(&rel, true) {
                continue;
            }
            if walk_files_impl(base, &path, ig, extra_skip, on_file) {
                return true;
            }
        } else {
            if ig.is_ignored(&rel, false) {
                continue;
            }
            if !on_file(&path, &rel) {
                return true;
            }
        }
    }
    false
}

pub fn parse_gitignore(root: &str) -> GitignoreMatcher {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| Path::new(root).to_path_buf());
    let root_str = root.to_string_lossy().to_string();

    let mut paths: Vec<String> = Vec::new();

    let empty_ig = GitignoreMatcher { rules: Vec::new() };
    walk_files(&root, &empty_ig, &[".git"], &mut |full_path, _rel| {
        if full_path.file_name().is_some_and(|n| n == ".gitignore") {
            paths.push(full_path.to_string_lossy().to_string());
        }
        true
    });

    {
        let cache = CACHE.read().unwrap();
        if let Some(entry) = cache.get(&root_str) {
            let mut valid = true;
            for p in &paths {
                if let Ok(meta) = std::fs::metadata(p) {
                    let modtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    if entry.modtimes.get(p) != Some(&modtime) {
                        valid = false;
                        break;
                    }
                } else {
                    valid = false;
                    break;
                }
            }
            if valid && entry.modtimes.len() == paths.len() {
                return entry.matcher.clone();
            }
        }
    }

    let mut m = GitignoreMatcher { rules: Vec::new() };
    let mut modtimes: HashMap<String, SystemTime> = HashMap::new();

    for p in &paths {
        if let Ok(meta) = std::fs::metadata(p) {
            modtimes.insert(p.clone(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));

            let rel_dir = Path::new(p).parent().unwrap_or_else(|| Path::new("."));
            let rel: String = if rel_dir == root {
                String::new()
            } else if let Ok(stripped) = rel_dir.strip_prefix(&root) {
                let s = stripped.to_string_lossy().to_string();
                if s == "." {
                    String::new()
                } else {
                    s
                }
            } else {
                String::new()
            };

            let rules = parse_gitignore_file(p, &rel);
            m.rules.extend(rules);
        }
    }

    {
        let mut cache = CACHE.write().unwrap();
        cache.insert(
            root_str,
            CacheEntry {
                matcher: m.clone(),
                modtimes,
            },
        );
    }

    m
}

fn parse_gitignore_file(path: &str, dir: &str) -> Vec<GitignoreRule> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut rules = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut negate = false;
        let mut line = line.to_string();
        if line.starts_with('!') {
            negate = true;
            line = line[1..].to_string();
        }

        let mut dir_only = false;
        if line.ends_with('/') {
            dir_only = true;
            line.pop();
        }

        let mut anchored = false;
        if let Some(stripped) = line.strip_prefix('/') {
            anchored = true;
            line = stripped.to_string();
        }

        let matcher = globset::GlobBuilder::new(&line)
            .literal_separator(true)
            .build()
            .ok();

        rules.push(GitignoreRule {
            pattern: line,
            negate,
            dir_only,
            anchored,
            dir: dir.to_string(),
            glob: matcher,
        });
    }
    rules
}

impl GitignoreMatcher {
    pub fn is_ignored(&self, rel_path: &str, is_dir: bool) -> bool {
        let mut ignored = false;
        for rule in &self.rules {
            if !is_dir && rule.dir_only {
                continue;
            }

            if !rule.dir.is_empty()
                && !rel_path.starts_with(&format!("{}/", rule.dir))
                && rel_path != rule.dir
            {
                continue;
            }

            let target = if rule.anchored {
                if rule.dir.is_empty() {
                    rel_path.to_string()
                } else {
                    rel_path
                        .strip_prefix(&format!("{}/", rule.dir))
                        .unwrap_or(rel_path)
                        .to_string()
                }
            } else if !rule.pattern.contains('/') {
                Path::new(rel_path)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| rel_path.to_string())
            } else if !rule.dir.is_empty() {
                rel_path
                    .strip_prefix(&format!("{}/", rule.dir))
                    .unwrap_or(rel_path)
                    .to_string()
            } else {
                rel_path.to_string()
            };

            if let Some(g) = &rule.glob {
                if g.compile_matcher().is_match(&target) {
                    ignored = !rule.negate;
                }
            }
        }
        ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gitignore_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "ignored_dir/\n*.log\n").unwrap();
        let ignored_dir = dir.path().join("ignored_dir");
        std::fs::create_dir_all(&ignored_dir).unwrap();

        let matcher = parse_gitignore(&dir.path().to_string_lossy());

        assert!(matcher.is_ignored("ignored_dir", true));
        assert!(matcher.is_ignored("debug.log", false));
        assert!(!matcher.is_ignored("main.go", false));
    }

    #[test]
    fn test_gitignore_negation() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "*.go\n!important.go\n").unwrap();

        let matcher = parse_gitignore(&dir.path().to_string_lossy());

        assert!(matcher.is_ignored("main.go", false));
        assert!(!matcher.is_ignored("important.go", false));
    }

    #[test]
    fn test_gitignore_nested() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "*.log\n").unwrap();
        let sub_dir = dir.path().join("sub");
        std::fs::create_dir_all(&sub_dir).unwrap();
        std::fs::write(sub_dir.join(".gitignore"), "*.go\n").unwrap();

        let matcher = parse_gitignore(&dir.path().to_string_lossy());

        assert!(matcher.is_ignored("debug.log", false));
        assert!(matcher.is_ignored("sub/helper.go", false));
        assert!(!matcher.is_ignored("main.go", false));
        assert!(matcher.is_ignored("sub/data.log", false));
    }

    #[test]
    fn test_gitignore_leading_slash_anchored() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "/target\n").unwrap();
        std::fs::create_dir_all(dir.path().join("target")).unwrap();
        std::fs::create_dir_all(dir.path().join("sub").join("target")).unwrap();

        let matcher = parse_gitignore(&dir.path().to_string_lossy());

        assert!(matcher.is_ignored("target", true));
        assert!(!matcher.is_ignored("sub/target", true));
    }

    #[test]
    fn test_gitignore_no_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let matcher = parse_gitignore(&dir.path().to_string_lossy());
        assert!(!matcher.is_ignored("anything.go", false));
    }
}
