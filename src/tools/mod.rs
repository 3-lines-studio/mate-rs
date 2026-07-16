mod bash;
mod edit_file;
pub mod gitignore;
mod glob;
mod grep;
mod read_file;
pub mod webfetch;
mod write_file;

use crate::message::ToolDef;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: HashMap<String, Value>,
    pub execute: Arc<
        dyn Fn(
                Value,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>>
            + Send
            + Sync,
    >,
}

#[derive(Clone)]
pub struct Registry {
    tools: HashMap<String, Tool>,
    order: Vec<String>,
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            tools: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn register(&mut self, tool: Tool) -> Result<(), String> {
        if self.tools.contains_key(&tool.name) {
            return Err(format!("tool {:?} already registered", tool.name));
        }
        self.order.push(tool.name.clone());
        self.tools.insert(tool.name.clone(), tool);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.get(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.order.clone()
    }

    pub fn tool_defs(&self) -> Vec<ToolDef> {
        let mut defs = Vec::new();
        for name in &self.order {
            if let Some(t) = self.tools.get(name) {
                defs.push(ToolDef {
                    def_type: "function".to_string(),
                    function: crate::message::ToolDefFunction {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.parameters.clone(),
                    },
                });
            }
        }
        defs
    }
}

static CATALOG: once_cell::sync::OnceCell<std::sync::Mutex<HashMap<String, Tool>>> =
    once_cell::sync::OnceCell::new();

fn catalog() -> &'static std::sync::Mutex<HashMap<String, Tool>> {
    CATALOG.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

pub fn register(name: &str, tool: Tool) {
    let mut cat = catalog().lock().unwrap();
    cat.insert(name.to_string(), tool);
}

pub fn lookup(name: &str) -> Option<Tool> {
    let cat = catalog().lock().unwrap();
    cat.get(name).cloned()
}

pub fn catalog_names() -> Vec<String> {
    let cat = catalog().lock().unwrap();
    let mut names: Vec<String> = cat.keys().cloned().collect();
    names.sort();
    names
}

pub fn standard() -> Vec<Tool> {
    vec![
        bash::tool(),
        read_file::tool(),
        write_file::tool(),
        edit_file::tool(),
        grep::tool(),
        glob::tool(),
    ]
}

pub fn define_tool<P, F, Fut>(
    name: &str,
    description: &str,
    params_schema: HashMap<String, Value>,
    f: F,
) -> Tool
where
    P: serde::de::DeserializeOwned + Send + 'static,
    F: Fn(P) -> Fut + Send + Sync + Clone + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    let name = name.to_string();
    let description = description.to_string();
    Tool {
        name: name.clone(),
        description,
        parameters: params_schema,
        execute: Arc::new(
            move |raw: Value| -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, String>> + Send>,
            > {
                let result = serde_json::from_value::<P>(raw);
                let name = name.clone();
                let f = f.clone();
                Box::pin(async move {
                    let p = result.map_err(|e| {
                        format!("invalid parameters for tool {}: {}", name, e)
                    })?;
                    f(p).await
                })
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn test_new_registry() {
        let r = Registry::new();
        assert!(r.names().is_empty());
    }

    #[test]
    fn test_register() {
        let mut r = Registry::new();
        let tool = Tool {
            name: "test".to_string(),
            description: "desc".to_string(),
            parameters: HashMap::new(),
            execute: Arc::new(|_| Box::pin(async { Ok("ok".to_string()) })),
        };
        assert!(r.register(tool).is_ok());
        assert_eq!(r.names(), vec!["test"]);
    }

    #[test]
    fn test_register_duplicate() {
        let mut r = Registry::new();
        let tool = Tool {
            name: "dup".to_string(),
            description: String::new(),
            parameters: HashMap::new(),
            execute: Arc::new(|_| Box::pin(async { Ok("ok".to_string()) })),
        };
        let _ = r.register(tool);
        let tool2 = Tool {
            name: "dup".to_string(),
            description: String::new(),
            parameters: HashMap::new(),
            execute: Arc::new(|_| Box::pin(async { Ok("ok".to_string()) })),
        };
        assert!(r.register(tool2).is_err());
    }

    #[test]
    fn test_get() {
        let mut r = Registry::new();
        let tool = Tool {
            name: "findme".to_string(),
            description: "hello".to_string(),
            parameters: HashMap::new(),
            execute: Arc::new(|_| Box::pin(async { Ok("ok".to_string()) })),
        };
        let _ = r.register(tool);
        let found = r.get("findme");
        assert!(found.is_some());
        assert_eq!(found.unwrap().description, "hello");
        assert!(r.get("nope").is_none());
    }

    #[test]
    fn test_names_ordering() {
        let mut r = Registry::new();
        let make_tool = |n: &str| Tool {
            name: n.to_string(),
            description: String::new(),
            parameters: HashMap::new(),
            execute: Arc::new(|_| Box::pin(async { Ok("ok".to_string()) })),
        };
        let _ = r.register(make_tool("b"));
        let _ = r.register(make_tool("a"));
        let _ = r.register(make_tool("c"));
        assert_eq!(r.names(), vec!["b", "a", "c"]);
    }

    #[test]
    fn test_names_copy() {
        let mut r = Registry::new();
        let tool = Tool {
            name: "x".to_string(),
            description: String::new(),
            parameters: HashMap::new(),
            execute: Arc::new(|_| Box::pin(async { Ok("ok".to_string()) })),
        };
        let _ = r.register(tool);
        let mut names = r.names();
        names[0] = "hacked".to_string();
        assert_eq!(r.names()[0], "x");
    }

    #[test]
    fn test_tool_defs() {
        let mut r = Registry::new();
        let mut params = HashMap::new();
        params.insert("type".to_string(), serde_json::json!("object"));
        let mut props = HashMap::new();
        props.insert(
            "command".to_string(),
            serde_json::json!({"type": "string"}),
        );
        params.insert("properties".to_string(), serde_json::json!(props));

        let tool = Tool {
            name: "bash".to_string(),
            description: "run command".to_string(),
            parameters: params,
            execute: Arc::new(|_| Box::pin(async { Ok("ok".to_string()) })),
        };
        let _ = r.register(tool);
        let defs = r.tool_defs();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].def_type, "function");
        assert_eq!(defs[0].function.name, "bash");
    }

    #[test]
    fn test_tool_defs_empty() {
        let r = Registry::new();
        assert!(r.tool_defs().is_empty());
    }

    #[test]
    fn test_standard() {
        let tools = standard();
        assert_eq!(tools.len(), 6);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        for n in &["bash", "read_file", "write_file", "edit_file", "grep", "glob"] {
            assert!(names.contains(n), "missing {}", n);
        }
    }

    #[derive(Debug, Deserialize)]
    struct GreetParams {
        name: String,
        #[allow(dead_code)]
        age: i32,
    }

    #[test]
    fn test_define_tool_valid_params() {
        let tool = define_tool("greet", "says hello", HashMap::new(), |p: GreetParams| async move {
            Ok(format!("hello {}", p.name))
        });

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((tool.execute)(
            serde_json::json!({"name": "Don", "age": 30}),
        ));
        assert_eq!(result.unwrap(), "hello Don");
    }

    #[test]
    fn test_define_tool_invalid_params() {
        let tool = define_tool("greet", "says hello", HashMap::new(), |p: GreetParams| async move {
            Ok(format!("hello {}", p.name))
        });

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((tool.execute)(
            serde_json::json!({}),
        ));
        assert!(result.is_err());
    }

    #[derive(Debug, Deserialize)]
    struct ContextParams {
        val: String,
    }

    #[test]
    fn test_define_tool_context_propagation() {
        let tool = define_tool("ctx", "test", HashMap::new(), |p: ContextParams| async move {
            Ok(format!("got: {}", p.val))
        });

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((tool.execute)(
            serde_json::json!({"val": "x"}),
        ));
        assert_eq!(result.unwrap(), "got: x");
    }

    #[test]
    fn test_catalog_lookup() {
        let tool = Tool {
            name: "cat_test".to_string(),
            description: "catalog test".to_string(),
            parameters: HashMap::new(),
            execute: Arc::new(|_| Box::pin(async { Ok("found".to_string()) })),
        };
        register("cat_test", tool);
        let found = lookup("cat_test");
        assert!(found.is_some());
        let t = found.unwrap();
        assert_eq!(t.name, "cat_test");
        assert_eq!(t.description, "catalog test");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((t.execute)(serde_json::json!({})));
        assert_eq!(result.unwrap(), "found");
    }

    #[test]
    fn test_catalog_lookup_missing() {
        assert!(lookup("nonexistent_tool_xyz").is_none());
    }

    #[test]
    fn test_catalog_names() {
        let names = catalog_names();
        assert!(names.contains(&"cat_test".to_string()));
    }
}
