use crate::tools::Tool;
use crate::tools::define_tool;
use crate::tools::gitignore::{parse_gitignore, walk_files};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tree_sitter::StreamingIterator;

#[derive(Debug, Deserialize)]
pub struct IndexBuildParams {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
pub struct SymbolsParams {
    pub query: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub max_results: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct IndexDef {
    name: String,
    kind: String,
    file: String,
    line: usize,
    #[serde(default)]
    signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexData {
    files: HashMap<String, f64>,
    defs: Vec<IndexDef>,
    #[serde(default)]
    version: u32,
}

const INDEX_VERSION: u32 = 1;

const SUPPORTED_EXTS: &[&str] = &["rs", "go", "ts", "tsx", "jsx", "css"];
fn project_hash(root: &str) -> String {
    let canon = std::fs::canonicalize(root).unwrap_or_else(|_| PathBuf::from(root));
    let mut hasher = Sha256::new();
    hasher.update(canon.to_string_lossy().as_bytes());
    hex::encode(&hasher.finalize()[..8])
}

fn index_store_dir(root: &str) -> PathBuf {
    PathBuf::from(crate::config::dir())
        .join("index")
        .join(project_hash(root))
}

const RUST_QUERY: &str = r#"
(function_item name: (identifier) @definition.function)
(struct_item name: (type_identifier) @definition.struct)
(enum_item name: (type_identifier) @definition.enum)
(trait_item name: (type_identifier) @definition.trait)
(type_item name: (type_identifier) @definition.type)
(const_item name: (identifier) @definition.const)
(static_item name: (identifier) @definition.static)
(mod_item name: (identifier) @definition.module)
(macro_definition name: (identifier) @definition.macro)
"#;

const GO_QUERY: &str = r#"
(function_declaration name: (identifier) @definition.function)
(method_declaration name: (field_identifier) @definition.method)
(type_declaration
  (type_spec name: (type_identifier) @definition.type))
"#;

const TS_QUERY: &str = r#"
(function_declaration name: (identifier) @definition.function)
(class_declaration name: (type_identifier) @definition.class)
(interface_declaration name: (type_identifier) @definition.interface)
(type_alias_declaration name: (type_identifier) @definition.type)
(variable_declarator name: (identifier) @definition.variable)
(method_definition name: (property_identifier) @definition.method)
(public_field_definition name: (property_identifier) @definition.field)
"#;

const CSS_QUERY: &str = r#"
(class_selector (class_name) @definition.class)
(id_selector (id_name) @definition.id)
"#;

fn lang_for_ext(ext: &str) -> Option<(&'static str, tree_sitter::Language, &'static str)> {
    match ext {
        "rs" => Some((
            "rust",
            tree_sitter::Language::from(tree_sitter_rust::LANGUAGE),
            RUST_QUERY,
        )),
        "go" => Some((
            "go",
            tree_sitter::Language::from(tree_sitter_go::LANGUAGE),
            GO_QUERY,
        )),
        "ts" => Some((
            "typescript",
            tree_sitter::Language::from(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
            TS_QUERY,
        )),
        "tsx" | "jsx" => Some((
            "tsx",
            tree_sitter::Language::from(tree_sitter_typescript::LANGUAGE_TSX),
            TS_QUERY,
        )),
        "css" => Some((
            "css",
            tree_sitter::Language::from(tree_sitter_css::LANGUAGE),
            CSS_QUERY,
        )),
        _ => None,
    }
}

pub fn build_index_background(cwd: &str) {
    let cwd = cwd.to_string();
    std::thread::spawn(move || {
        if let Err(e) = execute_index_build(IndexBuildParams {
            path: cwd,
            force: false,
        }) {
            log::warn!("index build: {}", e);
        }
    });
}

pub fn symbols_tool() -> Tool {
    let params = crate::tools::object_schema(
        &[
            (
                "query",
                serde_json::json!({"type": "string", "description": "Case-insensitive substring match over symbol name and file"}),
            ),
            (
                "path",
                serde_json::json!({"type": "string", "description": "Root directory (default: \".\")"}),
            ),
            (
                "max_results",
                serde_json::json!({"type": "integer", "description": "Maximum results (default: 100)"}),
            ),
        ],
        &["query"],
    );

    define_tool(
        "symbols",
        "Symbol-aware code navigation for Rust, Go, TS/TSX/JSX, CSS — faster and more precise than grep (reads syntax tree). Case-insensitive substring match over symbol name and file. Output: name\\tkind\\tfile:line\\tsignature. For occurrences/references, use grep.",
        params,
        |p: SymbolsParams| async move { execute_symbols(p) },
    )
}

fn execute_index_build(mut p: IndexBuildParams) -> Result<String, String> {
    if p.path.is_empty() {
        p.path = ".".to_string();
    }

    let mut index = if p.force {
        IndexData {
            files: HashMap::new(),
            defs: Vec::new(),
            version: INDEX_VERSION,
        }
    } else {
        load_index(&p.path)
    };

    let root = Path::new(&p.path);
    let ig = parse_gitignore(&p.path);

    let mut to_process: Vec<String> = Vec::new();
    walk_files(root, &ig, &[], &mut |_full_path, rel| {
        if let Some(ext) = extract_extension(rel)
            && SUPPORTED_EXTS.contains(&ext)
        {
            to_process.push(rel.to_string());
        }
        true
    });

    let mut new_files: HashMap<String, f64> = HashMap::new();
    let mut new_defs: Vec<IndexDef> = Vec::new();
    let force = p.force;
    let mut dirty = p.force;

    for rel in &to_process {
        let abs_path = root.join(rel);
        let current_mtime = get_mtime(&abs_path).unwrap_or(0.0);

        if !force
            && let Some(&stored_mtime) = index.files.get(rel)
            && stored_mtime >= current_mtime
        {
            new_files.insert(rel.clone(), stored_mtime);
            let kept: Vec<IndexDef> = index
                .defs
                .iter()
                .filter(|d| &d.file == rel)
                .cloned()
                .collect();
            new_defs.extend(kept);
            continue;
        }

        let defs = process_file(root, rel, current_mtime)?;
        new_files.insert(rel.clone(), current_mtime);
        new_defs.extend(defs);
        dirty = true;
    }

    if index.files.len() != new_files.len() {
        dirty = true;
    }

    index.files = new_files;
    index.defs = new_defs;
    index.version = INDEX_VERSION;

    if dirty {
        save_index(&p.path, &index)?;
    }

    let count = index.defs.len();
    Ok(format!(
        "indexed {} definitions in {} files",
        count,
        index.files.len()
    ))
}

fn execute_symbols(mut p: SymbolsParams) -> Result<String, String> {
    if p.path.is_empty() {
        p.path = ".".to_string();
    }
    if p.max_results <= 0 {
        p.max_results = 100;
    }

    execute_index_build(IndexBuildParams {
        path: p.path.clone(),
        force: false,
    })?;

    if p.query.is_empty() {
        return Ok(String::new());
    }

    let index = load_index(&p.path);
    let query_lower = p.query.to_lowercase();
    let mut scored: Vec<(usize, usize, String, &IndexDef)> = Vec::new();

    for d in &index.defs {
        let name_lower = d.name.to_lowercase();
        let file_lower = d.file.to_lowercase();
        let score = if name_lower == query_lower {
            0
        } else if name_lower.starts_with(&query_lower) {
            1
        } else if name_lower.contains(&query_lower) {
            2
        } else if file_lower.contains(&query_lower) {
            3
        } else {
            continue;
        };
        scored.push((score, d.name.len(), d.file.clone(), d));
    }

    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

    let results: Vec<String> = scored
        .iter()
        .take(p.max_results as usize)
        .map(|(_, _, _, d)| {
            format!(
                "{}\t{}\t{}:{}\t{}",
                d.name, d.kind, d.file, d.line, d.signature
            )
        })
        .collect();

    if results.is_empty() {
        Ok(String::new())
    } else {
        Ok(results.join("\n"))
    }
}

fn load_index(root: &str) -> IndexData {
    let path = index_store_dir(root).join("index.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let index: IndexData = serde_json::from_str(&s).unwrap_or_else(|e| {
                log::warn!("index parse failed, rebuilding {}: {}", path.display(), e);
                IndexData {
                    files: HashMap::new(),
                    defs: Vec::new(),
                    version: 0,
                }
            });
            if index.version != INDEX_VERSION {
                log::info!(
                    "index version mismatch ({} != {}), rebuilding",
                    index.version,
                    INDEX_VERSION
                );
                return IndexData {
                    files: HashMap::new(),
                    defs: Vec::new(),
                    version: INDEX_VERSION,
                };
            }
            index
        }
        Err(_) => IndexData {
            files: HashMap::new(),
            defs: Vec::new(),
            version: INDEX_VERSION,
        },
    }
}

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn save_index(root: &str, index: &IndexData) -> Result<(), String> {
    let dir = index_store_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("index: {}", e))?;
    let json = serde_json::to_string(index).map_err(|e| format!("index: {}", e))?;
    let final_path = dir.join("index.json");
    let tmp_path = dir.join(format!(
        "index.json.tmp.{}",
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&tmp_path, &json).map_err(|e| format!("index: {}", e))?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| format!("index: {}", e))
}

fn get_mtime(path: &Path) -> Option<f64> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let dur = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_secs_f64())
}

fn extract_extension(path: &str) -> Option<&str> {
    Path::new(path).extension().and_then(|e| e.to_str())
}

fn process_file(root: &Path, rel: &str, mtime: f64) -> Result<Vec<IndexDef>, String> {
    let ext = extract_extension(rel).unwrap_or("");
    let (_, language, query_str) =
        lang_for_ext(ext).ok_or_else(|| format!("index: unsupported extension: {}", ext))?;

    let abs_path = root.join(rel);
    let source = std::fs::read_to_string(&abs_path)
        .map_err(|e| format!("index: cannot read {}: {}", rel, e))?;

    let defs = extract_defs(&source, rel, language, query_str)?;

    let _ = mtime;
    Ok(defs)
}

fn parse_source(
    source: &str,
    language: &tree_sitter::Language,
) -> Result<tree_sitter::Tree, String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(language)
        .map_err(|e| format!("index: {}", e))?;
    parser
        .parse(source, None)
        .ok_or_else(|| "index: parse returned None".to_string())
}

fn extract_defs(
    source: &str,
    rel_path: &str,
    language: tree_sitter::Language,
    query_str: &str,
) -> Result<Vec<IndexDef>, String> {
    let tree = parse_source(source, &language)?;

    let query = tree_sitter::Query::new(&language, query_str)
        .map_err(|e| format!("index: query error: {}", e))?;
    let mut cursor = tree_sitter::QueryCursor::new();
    let capture_names = query.capture_names();

    let mut defs: Vec<IndexDef> = Vec::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let capture_name = capture_names[cap.index as usize];
            let kind = capture_name
                .strip_prefix("definition.")
                .unwrap_or(capture_name);
            let node = cap.node;
            let name = node
                .utf8_text(source.as_bytes())
                .map_err(|e| format!("index: {}", e))?;
            let line = node.start_position().row + 1;
            let signature = node
                .parent()
                .and_then(|p| p.utf8_text(source.as_bytes()).ok())
                .and_then(|t| t.lines().next().map(|l| l.to_string()))
                .unwrap_or_default();
            defs.push(IndexDef {
                name: name.to_string(),
                kind: kind.to_string(),
                file: rel_path.to_string(),
                line,
                signature,
            });
        }
    }
    Ok(defs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    // index_store_dir reads XDG_CONFIG_HOME; parallel tests must not race on it.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct TestDir {
        dir: tempfile::TempDir,
        _lock: MutexGuard<'static, ()>,
    }

    impl TestDir {
        fn path(&self) -> &std::path::Path {
            self.dir.path()
        }
    }

    fn setup_dir() -> TestDir {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::TempDir::new().unwrap();
        unsafe { std::env::set_var("XDG_CONFIG_HOME", dir.path().join("xdg")) };
        TestDir { dir, _lock: lock }
    }

    #[test]
    fn test_rust_extraction() {
        let dir = setup_dir();
        let src = r#"
fn foo() {}
pub struct Bar { x: i32 }
enum Baz { A, B }
trait Quux {}
const N: i32 = 42;
mod sub {}
type T = u32;
static S: i32 = 1;
macro_rules! my_macro { () => {} }
"#;
        std::fs::write(dir.path().join("test.rs"), src).unwrap();

        let lang = tree_sitter::Language::from(tree_sitter_rust::LANGUAGE);
        let defs = extract_defs(src, "test.rs", lang, RUST_QUERY).unwrap();

        let mut found: Vec<(&str, &str, usize)> = defs
            .iter()
            .map(|d| (d.name.as_str(), d.kind.as_str(), d.line))
            .collect();
        found.sort_by_key(|f| f.2);

        assert!(found.contains(&("foo", "function", 2)));
        assert!(found.contains(&("Bar", "struct", 3)));
        assert!(found.contains(&("Baz", "enum", 4)));
        assert!(found.contains(&("Quux", "trait", 5)));
        assert!(found.contains(&("N", "const", 6)));
        assert!(found.contains(&("sub", "module", 7)));
        assert!(found.contains(&("T", "type", 8)));
        assert!(found.contains(&("S", "static", 9)));
        assert!(found.contains(&("my_macro", "macro", 10)));
        assert_eq!(found.len(), 9);
    }

    #[test]
    fn test_go_extraction() {
        let dir = setup_dir();
        let src = r#"package p

func Foo() {}

func (r *R) Method() {}

type Bar struct { x int }

type Baz interface { Do() }
"#;
        std::fs::write(dir.path().join("test.go"), src).unwrap();

        let lang = tree_sitter::Language::from(tree_sitter_go::LANGUAGE);
        let defs = extract_defs(src, "test.go", lang, GO_QUERY).unwrap();

        let mut found: Vec<(&str, &str, usize)> = defs
            .iter()
            .map(|d| (d.name.as_str(), d.kind.as_str(), d.line))
            .collect();
        found.sort_by_key(|f| f.2);

        assert!(found.contains(&("Foo", "function", 3)));
        assert!(found.contains(&("Method", "method", 5)));
        assert!(found.contains(&("Bar", "type", 7)));
        assert!(found.contains(&("Baz", "type", 9)));
        assert_eq!(found.len(), 4);
    }

    #[test]
    fn test_ts_extraction() {
        let dir = setup_dir();
        let src = r#"
function foo() {}

class Bar {}

interface Baz {}

type Q = string;

const arrow = () => {};

class Container {
  method() {}
  public field: string = "";
}
"#;
        std::fs::write(dir.path().join("test.ts"), src).unwrap();

        let lang = tree_sitter::Language::from(tree_sitter_typescript::LANGUAGE_TYPESCRIPT);
        let defs = extract_defs(src, "test.ts", lang, TS_QUERY).unwrap();

        let mut found: Vec<(&str, &str, usize)> = defs
            .iter()
            .map(|d| (d.name.as_str(), d.kind.as_str(), d.line))
            .collect();
        found.sort_by_key(|f| f.2);

        assert!(found.contains(&("foo", "function", 2)));
        assert!(found.contains(&("Bar", "class", 4)));
        assert!(found.contains(&("Baz", "interface", 6)));
        assert!(found.contains(&("Q", "type", 8)));
        assert!(found.contains(&("arrow", "variable", 10)));
        assert!(found.contains(&("method", "method", 13)));
        assert!(found.contains(&("field", "field", 14)));
        assert!(found.len() >= 6);
    }

    #[test]
    fn test_tsx_extraction() {
        let dir = setup_dir();
        let src = r#"
function MyComp() { return <div />; }

const OtherComp = () => <span />;
"#;
        std::fs::write(dir.path().join("test.tsx"), src).unwrap();

        let lang = tree_sitter::Language::from(tree_sitter_typescript::LANGUAGE_TSX);
        let defs = extract_defs(src, "test.tsx", lang, TS_QUERY).unwrap();

        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"MyComp"));
        assert!(names.contains(&"OtherComp"));
    }

    #[test]
    fn test_css_extraction() {
        let dir = setup_dir();
        let src = r#"
.myclass { color: red; }
#myid { color: blue; }
"#;
        std::fs::write(dir.path().join("test.css"), src).unwrap();

        let lang = tree_sitter::Language::from(tree_sitter_css::LANGUAGE);
        let defs = extract_defs(src, "test.css", lang, CSS_QUERY).unwrap();

        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"myclass"));
        assert!(names.contains(&"myid"));
    }

    #[test]
    fn test_incremental_rebuild() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn one() {}").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        let r = execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();
        assert!(r.contains("indexed 1 definition"));

        std::fs::write(dir.path().join("b.rs"), "fn two() {}").unwrap();

        let r = execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();
        assert!(r.contains("indexed 2 definition"));

        let index = load_index(&root);
        let names: Vec<&str> = index.defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"one"));
        assert!(names.contains(&"two"));
    }

    #[test]
    fn test_force_rebuild() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn one() {}").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        std::fs::write(
            dir.path().join("a.rs"),
            "fn replaced() {} struct NewStruct {}",
        )
        .unwrap();

        let _r = execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: true,
        })
        .unwrap();

        let index = load_index(&root);
        let names: Vec<&str> = index.defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"replaced"));
        assert!(names.contains(&"NewStruct"));
        assert!(!names.contains(&"one"));
    }

    #[test]
    fn test_find_query() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn foo() {} fn bar() {}").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        let result = execute_symbols(SymbolsParams {
            query: "foo".to_string(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        assert!(result.contains("foo"));
        assert!(!result.contains("bar"));
    }

    #[test]
    fn test_list_query() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn alpha() {} fn beta() {}").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        let result = execute_symbols(SymbolsParams {
            query: "alpha".to_string(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        assert!(result.contains("alpha"));
        assert_eq!(result.lines().count(), 1);
    }

    #[test]
    fn test_auto_build_on_query() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn hello() {}").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        let result = execute_symbols(SymbolsParams {
            query: "hello".to_string(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        assert!(result.contains("hello"));

        assert!(index_store_dir(&root).join("index.json").exists());
    }

    #[test]
    fn test_empty_result() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn foo() {}").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        let result = execute_symbols(SymbolsParams {
            query: "nonexistent".to_string(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_max_results() {
        let dir = setup_dir();
        let mut src = String::new();
        for i in 0..10 {
            src.push_str(&format!("fn f{}() {{}}\n", i));
        }
        std::fs::write(dir.path().join("a.rs"), src).unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        let result = execute_symbols(SymbolsParams {
            query: "f".to_string(),
            path: root.clone(),
            max_results: 3,
        })
        .unwrap();
        assert_eq!(result.lines().count(), 3);
    }

    #[test]
    fn test_deleted_file_cleanup() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn one() {}").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn two() {}").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        let index = load_index(&root);
        assert_eq!(index.defs.len(), 2);
        assert!(index.files.contains_key("a.rs"));
        assert!(index.files.contains_key("b.rs"));

        std::fs::remove_file(dir.path().join("a.rs")).unwrap();

        let r = execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();
        assert!(r.contains("indexed 1 definitions in 1 files"));

        let index = load_index(&root);
        let names: Vec<&str> = index.defs.iter().map(|d| d.name.as_str()).collect();
        assert!(!names.contains(&"one"));
        assert!(names.contains(&"two"));
        assert!(!index.files.contains_key("a.rs"));
        assert!(index.files.contains_key("b.rs"));
    }

    #[test]
    fn test_empty_query() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn foo() {}").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        let result = execute_symbols(SymbolsParams {
            query: String::new(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_signature_output() {
        let dir = setup_dir();
        std::fs::write(
            dir.path().join("a.rs"),
            "fn my_func(x: i32) -> bool { true }",
        )
        .unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        let result = execute_symbols(SymbolsParams {
            query: "my_func".to_string(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        assert!(result.contains("my_func"));
        assert!(result.contains("fn my_func(x: i32) -> bool"));
        let parts: Vec<&str> = result.split('\t').collect();
        assert_eq!(parts.len(), 4, "expected name, kind, file:line, signature");
    }

    #[test]
    fn test_fuzzy_ranking() {
        let dir = setup_dir();
        std::fs::write(
            dir.path().join("a.rs"),
            "fn exact() {} fn exact_prefix_foo() {} fn contains_exact_x() {}",
        )
        .unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        let result = execute_symbols(SymbolsParams {
            query: "exact".to_string(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.len() >= 3);
        assert!(lines[0].starts_with("exact\t"));
    }

    #[test]
    fn test_version_mismatch() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn old() {}").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        let idx_path = index_store_dir(&root).join("index.json");
        let mut idx: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&idx_path).unwrap()).unwrap();
        idx["version"] = serde_json::Value::Number(0.into());
        std::fs::write(&idx_path, serde_json::to_string(&idx).unwrap()).unwrap();

        let index = load_index(&root);
        assert_eq!(index.version, INDEX_VERSION);
        assert!(index.defs.is_empty());
    }
}
