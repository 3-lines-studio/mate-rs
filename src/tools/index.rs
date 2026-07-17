use crate::tools::define_tool;
use crate::tools::gitignore::{parse_gitignore, should_skip_dir};
use crate::tools::Tool;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
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
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub file: String,
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
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexData {
    files: HashMap<String, f64>,
    defs: Vec<IndexDef>,
}

const SUPPORTED_EXTS: &[&str] = &["rs", "go", "ts", "tsx", "jsx", "css"];
const INDEX_DIR: &str = ".mate";
const INDEX_FILE: &str = ".mate/index.json";

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
    let mut params = HashMap::new();
    params.insert("type".to_string(), serde_json::json!("object"));
    let mut properties: HashMap<String, serde_json::Value> = HashMap::new();
    properties.insert(
        "kind".to_string(),
        serde_json::json!({"type": "string", "description": "Query kind: \"find\" (definitions), \"refs\" (call sites), or \"list\" (symbols in a file)"}),
    );
    properties.insert(
        "name".to_string(),
        serde_json::json!({"type": "string", "description": "Symbol name (exact match) for find/refs queries"}),
    );
    properties.insert(
        "file".to_string(),
        serde_json::json!({"type": "string", "description": "Relative file path for list query"}),
    );
    properties.insert(
        "path".to_string(),
        serde_json::json!({"type": "string", "description": "Root directory (default: \".\")"}),
    );
    properties.insert(
        "max_results".to_string(),
        serde_json::json!({"type": "integer", "description": "Maximum results (default: 100)"}),
    );
    params.insert("properties".to_string(), serde_json::json!(properties));
    params.insert("required".to_string(), serde_json::json!(["kind"]));

    define_tool(
        "symbols",
        "Symbol-aware code navigation for Rust, Go, TS/TSX/JSX, CSS — faster and more precise than grep, which also matches text inside strings and comments (this reads the syntax tree, so results are real definitions/call sites, not stray mentions). kind=\"find\" + name: where a symbol is defined. kind=\"refs\" + name: every call site. kind=\"list\" + file: all symbols in a file. Example: {\"kind\":\"find\",\"name\":\"Parser\"}. Output: tab-separated rows of name, kind, file:line. Prefer this over grep for any symbol lookup.",
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
        }
    } else {
        load_index(&p.path)
    };

    let root = Path::new(&p.path);
    let ig = parse_gitignore(&p.path);

    let mut to_process: Vec<String> = Vec::new();
    collect_files(root, root, &ig, &mut to_process);

    let mut new_files: HashMap<String, f64> = HashMap::new();
    let mut new_defs: Vec<IndexDef> = Vec::new();
    let force = p.force;

    for rel in &to_process {
        let abs_path = root.join(rel);
        let current_mtime = get_mtime(&abs_path).unwrap_or(0.0);

        if !force {
            if let Some(&stored_mtime) = index.files.get(rel) {
                if stored_mtime >= current_mtime {
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
            }
        }

        let defs = process_file(root, rel, current_mtime)?;
        new_files.insert(rel.clone(), current_mtime);
        new_defs.extend(defs);
    }

    index.files = new_files;
    index.defs = new_defs;

    save_index(&p.path, &index)?;

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

    let index_path = Path::new(&p.path).join(INDEX_FILE);
    if !index_path.exists() {
        execute_index_build(IndexBuildParams {
            path: p.path.clone(),
            force: false,
        })?;
    }

    let index = load_index(&p.path);

    match p.kind.as_str() {
        "find" => {
            if p.name.is_empty() {
                return Err("index: 'name' is required for find query".to_string());
            }
            let results: Vec<String> = index
                .defs
                .iter()
                .filter(|d| d.name == p.name)
                .take(p.max_results as usize)
                .map(|d| format!("{}\t{}\t{}:{}", d.name, d.kind, d.file, d.line))
                .collect();
            if results.is_empty() {
                Ok(String::new())
            } else {
                Ok(results.join("\n"))
            }
        }
        "refs" => {
            if p.name.is_empty() {
                return Err("index: 'name' is required for refs query".to_string());
            }
            find_refs(&p.path, &p.name, p.max_results)
        }
        "list" => {
            if p.file.is_empty() {
                return Err("index: 'file' is required for list query".to_string());
            }
            let results: Vec<String> = index
                .defs
                .iter()
                .filter(|d| d.file == p.file)
                .take(p.max_results as usize)
                .map(|d| format!("{}\t{}\t{}", d.name, d.kind, d.line))
                .collect();
            if results.is_empty() {
                Ok(String::new())
            } else {
                Ok(results.join("\n"))
            }
        }
        _ => Err(format!(
            "index: unknown query kind '{}', expected 'find', 'refs', or 'list'",
            p.kind
        )),
    }
}

fn load_index(root: &str) -> IndexData {
    let path = Path::new(root).join(INDEX_FILE);
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or(IndexData {
            files: HashMap::new(),
            defs: Vec::new(),
        }),
        Err(_) => IndexData {
            files: HashMap::new(),
            defs: Vec::new(),
        },
    }
}

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn save_index(root: &str, index: &IndexData) -> Result<(), String> {
    let dir = Path::new(root).join(INDEX_DIR);
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

fn collect_files(
    base: &Path,
    dir: &Path,
    ig: &crate::tools::gitignore::GitignoreMatcher,
    results: &mut Vec<String>,
) {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
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
            if ig.is_ignored(&rel, true) {
                continue;
            }
            if name == INDEX_DIR {
                continue;
            }
            collect_files(base, &path, ig, results);
        } else {
            if ig.is_ignored(&rel, false) {
                continue;
            }
            if let Some(ext) = extract_extension(&rel) {
                if SUPPORTED_EXTS.contains(&ext) {
                    results.push(rel);
                }
            }
        }
    }
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

fn extract_defs(
    source: &str,
    rel_path: &str,
    language: tree_sitter::Language,
    query_str: &str,
) -> Result<Vec<IndexDef>, String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .map_err(|e| format!("index: {}", e))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "index: parse returned None".to_string())?;

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
            defs.push(IndexDef {
                name: name.to_string(),
                kind: kind.to_string(),
                file: rel_path.to_string(),
                line,
            });
        }
    }
    Ok(defs)
}

fn find_refs(root: &str, target: &str, max_results: i32) -> Result<String, String> {
    let root_path = Path::new(root);
    let index = load_index(root);

    let mut candidate_files: Vec<String> = index.files.keys().cloned().collect();
    if candidate_files.is_empty() {
        candidate_files = Vec::new();
        let ig = parse_gitignore(root);
        collect_files(root_path, root_path, &ig, &mut candidate_files);
    }

    let mut results: Vec<String> = Vec::new();
    let max = max_results as usize;

    for rel in &candidate_files {
        if results.len() >= max {
            break;
        }
        let ext = match extract_extension(rel) {
            Some(e) => e,
            None => continue,
        };
        let (_, language, _) = match lang_for_ext(ext) {
            Some(l) => l,
            None => continue,
        };

        let abs_path = root_path.join(rel);
        let source = match std::fs::read_to_string(&abs_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&language).is_err() {
            continue;
        }
        let tree = match parser.parse(&source, None) {
            Some(t) => t,
            None => continue,
        };

        let remaining = max - results.len();
        walk_tree_for_refs(
            tree.root_node(),
            source.as_bytes(),
            rel,
            target,
            remaining,
            &mut results,
        );
    }

    if results.is_empty() {
        Ok(String::new())
    } else {
        results.truncate(max);
        Ok(results.join("\n"))
    }
}

const REF_NODE_TYPES: &[&str] = &[
    "identifier",
    "type_identifier",
    "field_identifier",
    "property_identifier",
];

fn walk_tree_for_refs(
    node: tree_sitter::Node,
    source: &[u8],
    file: &str,
    target: &str,
    max: usize,
    results: &mut Vec<String>,
) {
    if results.len() >= max {
        return;
    }

    if REF_NODE_TYPES.contains(&node.kind()) {
        if let Ok(text) = node.utf8_text(source) {
            if text == target {
                let line = node.start_position().row + 1;
                results.push(format!("{}:{}", file, line));
                if results.len() >= max {
                    return;
                }
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            walk_tree_for_refs(child, source, file, target, max, results);
        }
        if results.len() >= max {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_dir() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
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
            kind: "find".to_string(),
            name: "foo".to_string(),
            file: String::new(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        assert!(result.contains("foo"));
        assert!(!result.contains("bar"));
    }

    #[test]
    fn test_refs_query() {
        let dir = setup_dir();
        std::fs::write(
            dir.path().join("a.rs"),
            "fn foo() {} fn bar() { foo(); foo(); }",
        )
        .unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        let result = execute_symbols(SymbolsParams {
            kind: "refs".to_string(),
            name: "foo".to_string(),
            file: String::new(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        assert_eq!(result.lines().count(), 3);
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
            kind: "list".to_string(),
            name: String::new(),
            file: "a.rs".to_string(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        assert!(result.contains("alpha"));
        assert!(result.contains("beta"));
        assert_eq!(result.lines().count(), 2);
    }

    #[test]
    fn test_auto_build_on_query() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn hello() {}").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        let result = execute_symbols(SymbolsParams {
            kind: "find".to_string(),
            name: "hello".to_string(),
            file: String::new(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();
        assert!(result.contains("hello"));

        assert!(dir.path().join(".mate/index.json").exists());
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
            kind: "find".to_string(),
            name: "nonexistent".to_string(),
            file: String::new(),
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
            kind: "list".to_string(),
            name: String::new(),
            file: "a.rs".to_string(),
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
    fn test_cross_file_refs() {
        let dir = setup_dir();
        std::fs::write(dir.path().join("a.rs"), "fn foo() {}").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn caller() {\n    foo();\n}\n").unwrap();
        let root = dir.path().to_string_lossy().to_string();

        execute_index_build(IndexBuildParams {
            path: root.clone(),
            force: false,
        })
        .unwrap();

        let result = execute_symbols(SymbolsParams {
            kind: "refs".to_string(),
            name: "foo".to_string(),
            file: String::new(),
            path: root.clone(),
            max_results: 0,
        })
        .unwrap();

        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2, "expected def in a.rs + call in b.rs");
        assert!(
            result.contains("a.rs:1"),
            "def site in a.rs must be found: {}",
            result
        );
        assert!(
            result.contains("b.rs:2"),
            "call site in b.rs must be found: {}",
            result
        );
    }
}
