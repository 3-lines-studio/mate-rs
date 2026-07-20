use crate::tools::Tool;
use crate::tools::define_tool;
use crate::tools::gitignore::{parse_gitignore, walk_files};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GlobParams {
    pub pattern: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub max_results: i32,
}

pub fn tool() -> Tool {
    let params = crate::tools::object_schema(
        &[
            (
                "pattern",
                serde_json::json!({"type": "string", "description": "Glob pattern, e.g. \"**/*.go\", \"src/*_test.go\", \"*.md\""}),
            ),
            (
                "path",
                serde_json::json!({"type": "string", "description": "Root directory to search from (default: current working directory)"}),
            ),
            (
                "max_results",
                serde_json::json!({"type": "integer", "description": "Maximum results (default: 50)"}),
            ),
        ],
        &["pattern"],
    );

    define_tool(
        "glob",
        "Find files matching a glob pattern. Supports ** for recursive matching. Skips common VCS/dependency directories and respects .gitignore.",
        params,
        |p: GlobParams| async move { execute_glob(p) },
    )
}

fn execute_glob(mut p: GlobParams) -> Result<String, String> {
    if p.path.is_empty() {
        p.path = ".".to_string();
    }
    if p.max_results <= 0 {
        p.max_results = 50;
    }

    let glob = globset::GlobBuilder::new(&p.pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| format!("glob: {}", e))?;
    let matcher = glob.compile_matcher();

    let ig = parse_gitignore(&p.path);

    let mut results: Vec<String> = Vec::new();

    let root = std::path::Path::new(&p.path);
    let max = p.max_results as usize;
    walk_files(root, &ig, &[], &mut |_full_path, rel| {
        if matcher.is_match(rel) {
            results.push(rel.to_string());
        }
        results.len() < max
    });

    results.sort();
    Ok(results.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_basic_pattern() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.go"), "").unwrap();
        std::fs::write(dir.path().join("b.go"), "").unwrap();
        std::fs::write(dir.path().join("c.md"), "").unwrap();

        let result = execute_glob(GlobParams {
            pattern: "*.go".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            max_results: 0,
        })
        .unwrap();
        let files: Vec<&str> = result.lines().collect();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "a.go");
        assert_eq!(files[1], "b.go");
    }

    #[test]
    fn test_glob_recursive_double_star() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("root.go"), "").unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("sub.go"), "").unwrap();
        let deep = sub.join("deep");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("deep.go"), "").unwrap();

        let result = execute_glob(GlobParams {
            pattern: "**/*.go".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            max_results: 0,
        })
        .unwrap();
        assert_eq!(result.lines().count(), 3);
    }

    #[test]
    fn test_glob_nested_pattern() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("root.go"), "").unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("sub.go"), "").unwrap();
        std::fs::write(sub.join("sub_test.go"), "").unwrap();

        let result = execute_glob(GlobParams {
            pattern: "sub/*_test.go".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            max_results: 0,
        })
        .unwrap();
        assert!(result.contains("sub/sub_test.go"));
        assert!(!result.contains("sub/sub.go"));
    }

    #[test]
    fn test_glob_max_results() {
        let dir = tempfile::TempDir::new().unwrap();
        for i in 0..10 {
            std::fs::write(dir.path().join(format!("file_{}.go", i)), "").unwrap();
        }

        let result = execute_glob(GlobParams {
            pattern: "*.go".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            max_results: 5,
        })
        .unwrap();
        assert_eq!(result.lines().count(), 5);
    }

    #[test]
    fn test_glob_star_does_not_cross_slash() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("root.go"), "").unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("nested.go"), "").unwrap();

        let result = execute_glob(GlobParams {
            pattern: "*.go".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            max_results: 0,
        })
        .unwrap();
        assert_eq!(result, "root.go");
    }

    #[test]
    fn test_glob_no_match() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.go"), "").unwrap();

        let result = execute_glob(GlobParams {
            pattern: "*.md".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            max_results: 0,
        })
        .unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_glob_skips_vcs_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("root.go"), "").unwrap();
        let git_dir = dir.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("config.go"), "").unwrap();

        let result = execute_glob(GlobParams {
            pattern: "**/*.go".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            max_results: 0,
        })
        .unwrap();
        assert!(!result.contains(".git/config.go"));
    }

    #[test]
    fn test_glob_single_star() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("foo_test.go"), "").unwrap();
        std::fs::write(dir.path().join("bar_test.go"), "").unwrap();
        std::fs::write(dir.path().join("foo.go"), "").unwrap();

        let result = execute_glob(GlobParams {
            pattern: "*_test.go".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            max_results: 0,
        })
        .unwrap();
        assert_eq!(result.lines().count(), 2);
    }

    #[test]
    fn test_glob_question_mark() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("v1.go"), "").unwrap();
        std::fs::write(dir.path().join("v2.go"), "").unwrap();
        std::fs::write(dir.path().join("v10.go"), "").unwrap();
        std::fs::write(dir.path().join("version.go"), "").unwrap();

        let result = execute_glob(GlobParams {
            pattern: "v?.go".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            max_results: 0,
        })
        .unwrap();
        let files: Vec<&str> = result.lines().collect();
        assert_eq!(files.len(), 2);
        assert!(!result.contains("v10.go"));
    }

    #[test]
    fn test_glob_respects_gitignore() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "build/\n").unwrap();
        let build = dir.path().join("build");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("output.go"), "").unwrap();
        std::fs::write(dir.path().join("main.go"), "").unwrap();

        let result = execute_glob(GlobParams {
            pattern: "**/*.go".to_string(),
            path: dir.path().to_string_lossy().to_string(),
            max_results: 0,
        })
        .unwrap();
        assert!(!result.contains("build/"));
        assert!(result.contains("main.go"));
    }
}
