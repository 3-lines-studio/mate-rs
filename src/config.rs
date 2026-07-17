use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub base_url: String,
    #[serde(skip)]
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub provider: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub context_window: i32,
    #[serde(default)]
    pub max_output_tokens: i32,
    #[serde(default)]
    pub thinking_type: String,
    #[serde(default)]
    pub reasoning_effort: String,
    #[serde(default)]
    pub reasoning_max_tokens: i32,
    #[serde(default)]
    pub input_price: f64,
    #[serde(default)]
    pub cached_input_price: f64,
    #[serde(default)]
    pub output_price: f64,
    #[serde(default)]
    pub prompt_cache: bool,
    #[serde(default)]
    pub prompt_cache_ttl: String,
    #[serde(default)]
    pub fallback_models: Vec<String>,
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub provider_sort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: i32,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub compaction_model: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            max_tool_rounds: default_max_tool_rounds(),
            tools: vec![],
            interfaces: vec![],
            prompt: String::new(),
            compaction_model: String::new(),
        }
    }
}

fn default_max_tool_rounds() -> i32 {
    99
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    pub id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionConfig {
    #[serde(default)]
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlackConfig {
    #[serde(default)]
    #[serde(skip_serializing)]
    pub bot_token: String,
    #[serde(default)]
    #[serde(skip_serializing)]
    pub app_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramConfig {
    #[serde(default)]
    #[serde(skip_serializing)]
    pub bot_token: String,
    #[serde(default)]
    pub allowed_users: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScheduledJob {
    pub cron: String,
    pub prompt: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScheduleConfig {
    #[serde(default)]
    pub jobs: Vec<ScheduledJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TUIConfig {
    #[serde(default)]
    pub tools_expanded: bool,
    #[serde(default)]
    pub show_thinking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub models: Vec<ModelConfig>,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub subagents: Vec<SubagentConfig>,
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub slack: SlackConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub services: HashMap<String, HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub tui: TUIConfig,
    #[serde(default)]
    pub schedule: ScheduleConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Secrets {
    #[serde(default)]
    pub providers: HashMap<String, String>,
    #[serde(default)]
    pub slack: SlackSecrets,
    #[serde(default)]
    pub telegram: TelegramSecrets,
    #[serde(default)]
    pub services: HashMap<String, HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlackSecrets {
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub app_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramSecrets {
    #[serde(default)]
    pub bot_token: String,
}

impl Config {
    pub fn merge(&mut self, secrets: Secrets) {
        for provider in &mut self.providers {
            if let Some(key) = secrets.providers.get(&provider.id) {
                provider.api_key = key.clone();
            }
        }
        if !secrets.slack.bot_token.is_empty() {
            self.slack.bot_token = secrets.slack.bot_token;
        }
        if !secrets.slack.app_token.is_empty() {
            self.slack.app_token = secrets.slack.app_token;
        }
        if !secrets.telegram.bot_token.is_empty() {
            self.telegram.bot_token = secrets.telegram.bot_token;
        }
        if !secrets.services.is_empty() {
            self.services = secrets.services;
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::default_for(&dir())
    }
}

impl Config {
    pub fn default_for(dir: &str) -> Self {
        let mut session_dir = PathBuf::from(dir);
        session_dir.push("sessions");
        Self {
            providers: vec![],
            models: vec![],
            agent: AgentConfig::default(),
            subagents: vec![],
            session: SessionConfig {
                dir: session_dir.to_string_lossy().to_string(),
            },
            slack: SlackConfig::default(),
            telegram: TelegramConfig::default(),
            services: HashMap::new(),
            tui: TUIConfig::default(),
            schedule: ScheduleConfig::default(),
        }
    }
}

pub fn dir_for_env(name: &str, xdg: Option<&str>, home: &str) -> String {
    if let Some(xdg) = xdg {
        let mut p = PathBuf::from(xdg);
        p.push(name);
        p.to_string_lossy().to_string()
    } else {
        let mut p = PathBuf::from(home);
        p.push(".config");
        p.push(name);
        p.to_string_lossy().to_string()
    }
}

pub fn dir_for(name: &str) -> String {
    dir_for_env(
        name,
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        &dirs_home(),
    )
}

pub fn dir() -> String {
    dir_for("mate")
}

fn dirs_home() -> String {
    std::env::var("HOME")
        .or_else(|_| {
            std::env::var("USERPROFILE").or_else(|_| {
                let home = std::env::var("HOMEDRIVE").unwrap_or_default();
                let home_path = std::env::var("HOMEPATH").unwrap_or_default();
                Ok::<String, std::env::VarError>(format!("{}{}", home, home_path))
            })
        })
        .unwrap_or_default()
}

fn config_path(dir: &str) -> PathBuf {
    let mut p = PathBuf::from(dir);
    p.push("config.toml");
    p
}

pub fn save_tui(
    dir: &str,
    tools_expanded: bool,
    show_thinking: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut data: toml::Table = load_config_map(dir)?;
    set_nested(
        &mut data,
        "tui",
        "tools_expanded",
        toml::Value::Boolean(tools_expanded),
    );
    set_nested(
        &mut data,
        "tui",
        "show_thinking",
        toml::Value::Boolean(show_thinking),
    );
    write_config_map(dir, &data)
}

pub fn save_config(
    dir: &str,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut data = load_config_map(dir)?;
    let edited = toml::Value::try_from(config)?;
    if let toml::Value::Table(mut t) = edited {
        t.remove("services");
        for (key, val) in t {
            data.insert(key, val);
        }
    }
    write_config_map(dir, &data)
}

fn load_config_map(dir: &str) -> Result<toml::Table, Box<dyn std::error::Error + Send + Sync>> {
    let cfg_path = config_path(dir);
    match std::fs::read_to_string(&cfg_path) {
        Ok(content) => {
            let table: toml::Table = toml::from_str(&content)?;
            Ok(table)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(e) => Err(Box::new(e)),
    }
}

fn write_config_map(
    dir: &str,
    data: &toml::Table,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cfg_path = config_path(dir);
    let content = toml::to_string(data)?;
    std::fs::write(&cfg_path, content)?;
    Ok(())
}

fn set_nested(data: &mut toml::Table, table: &str, key: &str, value: toml::Value) {
    let sub = data
        .entry(table)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let toml::Value::Table(t) = sub {
        t.insert(key.to_string(), value);
    }
}

pub fn load_from(dir: &str) -> Result<Config, Box<dyn std::error::Error + Send + Sync>> {
    let mut cfg = Config::default_for(dir);

    let cfg_path = config_path(dir);
    if cfg_path.exists() {
        let content = std::fs::read_to_string(&cfg_path)?;
        cfg = toml::from_str(&content)?;
        if cfg.session.dir.is_empty() {
            let mut session_dir = PathBuf::from(dir);
            session_dir.push("sessions");
            cfg.session.dir = session_dir.to_string_lossy().to_string();
        }
    }

    let mut secrets_path = PathBuf::from(dir);
    secrets_path.push("secrets.toml");
    if secrets_path.exists() {
        let content = std::fs::read_to_string(&secrets_path)?;
        let secrets: Secrets = toml::from_str(&content)?;
        cfg.merge(secrets);
    }

    Ok(cfg)
}

pub fn load() -> Result<Config, Box<dyn std::error::Error + Send + Sync>> {
    load_from(&dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(path: &std::path::Path, content: &str) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_save_tui_preserves_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let mate_dir = dir.path().join("mate");
        std::fs::create_dir_all(&mate_dir).unwrap();
        let cfg_path = mate_dir.join("config.toml");
        write_file(&cfg_path, "[agent]\nmax_tool_rounds = 10\n");

        save_tui(&mate_dir.to_string_lossy(), true, false).unwrap();

        let data = std::fs::read_to_string(&cfg_path).unwrap();
        let cfg: toml::Table = toml::from_str(&data).unwrap();
        let tui = cfg["tui"].as_table().unwrap();
        assert!(tui["tools_expanded"].as_bool().unwrap());
        assert!(!tui["show_thinking"].as_bool().unwrap());
        let agent = cfg["agent"].as_table().unwrap();
        assert_eq!(agent["max_tool_rounds"].as_integer().unwrap(), 10);
    }

    #[test]
    fn test_save_tui_no_existing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let mate_dir = dir.path().join("mate");
        std::fs::create_dir_all(&mate_dir).unwrap();

        save_tui(&mate_dir.to_string_lossy(), true, true).unwrap();

        let cfg_path = mate_dir.join("config.toml");
        let data = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(data.contains("tools_expanded = true"));
    }

    #[test]
    fn test_dir_xdg_env() {
        assert_eq!(
            dir_for_env("mate", Some("/tmp/xdg"), "/tmp/home"),
            "/tmp/xdg/mate"
        );
    }

    #[test]
    fn test_dir_no_xdg() {
        assert_eq!(
            dir_for_env("mate", None, "/tmp/home"),
            "/tmp/home/.config/mate"
        );
    }

    #[test]
    fn test_dir_for_custom_name() {
        assert_eq!(
            dir_for_env("alfred", Some("/tmp/xdg"), "/tmp/home"),
            "/tmp/xdg/alfred"
        );
    }

    #[test]
    fn test_dir_for_different_names_isolate() {
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg");
        let a = dir_for("agent-a");
        let b = dir_for("agent-b");
        assert_ne!(a, b);
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    #[test]
    fn test_default_for_custom_dir() {
        let cfg = Config::default_for("/custom/dir");
        assert_eq!(cfg.session.dir, "/custom/dir/sessions");
    }

    #[test]
    fn test_load_from_custom_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg_path = dir.path().join("config.toml");
        write_file(&cfg_path, "[agent]\nmax_tool_rounds = 55\n");

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(cfg.agent.max_tool_rounds, 55);
        assert_eq!(
            cfg.session.dir,
            format!("{}/sessions", dir.path().to_string_lossy())
        );
    }

    #[test]
    fn test_load_from_no_config_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(cfg.agent.max_tool_rounds, 99);
    }

    #[test]
    fn test_default() {
        let cfg = Config::default();
        assert_eq!(cfg.agent.max_tool_rounds, 99);
        assert!(cfg.agent.tools.is_empty());
        assert!(!cfg.session.dir.is_empty());
    }

    #[test]
    fn test_load_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(cfg.agent.max_tool_rounds, 99);
    }

    #[test]
    fn test_load_config_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg_path = dir.path().join("config.toml");
        write_file(&cfg_path, "[agent]\nmax_tool_rounds = 42\n");

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(cfg.agent.max_tool_rounds, 42);
    }

    #[test]
    fn test_load_secrets_provider_merge() {
        let dir = tempfile::TempDir::new().unwrap();

        write_file(
            &dir.path().join("config.toml"),
            r#"
[[providers]]
id = "openai"
base_url = "https://api.openai.com/v1"

[[providers]]
id = "anthropic"
base_url = "https://api.anthropic.com/v1"
"#,
        );
        write_file(
            &dir.path().join("secrets.toml"),
            r#"
[providers]
openai = "sk-openai-key"
anthropic = "sk-anthropic-key"
"#,
        );

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(cfg.providers.len(), 2);
        assert_eq!(cfg.providers[0].api_key, "sk-openai-key");
        assert_eq!(cfg.providers[0].base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.providers[1].api_key, "sk-anthropic-key");
        assert_eq!(cfg.providers[1].base_url, "https://api.anthropic.com/v1");
    }

    #[test]
    fn test_load_secrets_partial_keys() {
        let dir = tempfile::TempDir::new().unwrap();

        write_file(
            &dir.path().join("config.toml"),
            r#"
[[providers]]
id = "openai"
base_url = "https://api.openai.com/v1"

[[providers]]
id = "anthropic"
base_url = "https://api.anthropic.com/v1"
"#,
        );
        write_file(
            &dir.path().join("secrets.toml"),
            r#"
[providers]
openai = "sk-openai-key"
"#,
        );

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(cfg.providers[0].api_key, "sk-openai-key");
        assert_eq!(cfg.providers[1].api_key, "");
        assert_eq!(cfg.providers[1].base_url, "https://api.anthropic.com/v1");
    }

    #[test]
    fn test_load_config_provider_no_api_key() {
        let dir = tempfile::TempDir::new().unwrap();

        write_file(
            &dir.path().join("config.toml"),
            r#"
[[providers]]
id = "openai"
base_url = "https://api.openai.com/v1"
api_key = "sk-should-be-ignored"

[[models]]
id = "gpt4"
provider = "openai"
name = "gpt-4"
context_window = 128000
"#,
        );

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(cfg.providers.len(), 1);
        assert_eq!(cfg.providers[0].id, "openai");
        assert_eq!(cfg.providers[0].api_key, "");
        assert_eq!(cfg.models.len(), 1);
        assert_eq!(cfg.models[0].name, "gpt-4");
    }

    #[test]
    fn test_load_invalid_toml() {
        let dir = tempfile::TempDir::new().unwrap();
        write_file(&dir.path().join("config.toml"), "this is not toml {{{");
        let result = load_from(&dir.path().to_string_lossy());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_model_price_fields() {
        let dir = tempfile::TempDir::new().unwrap();

        write_file(
            &dir.path().join("config.toml"),
            r#"
[[providers]]
id = "openai"
base_url = "https://api.openai.com/v1"

[[models]]
id = "gpt-4"
provider = "openai"
name = "gpt-4"
context_window = 128000
input_price = 3.0
cached_input_price = 0.3
output_price = 15.0
"#,
        );

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        let m = &cfg.models[0];
        assert_eq!(m.input_price, 3.0);
        assert_eq!(m.cached_input_price, 0.3);
        assert_eq!(m.output_price, 15.0);
    }

    #[test]
    fn test_load_model_price_fields_default_to_zero() {
        let dir = tempfile::TempDir::new().unwrap();

        write_file(
            &dir.path().join("config.toml"),
            r#"
[[providers]]
id = "openai"
base_url = "https://api.openai.com/v1"

[[models]]
id = "gpt-4"
provider = "openai"
name = "gpt-4"
context_window = 128000
"#,
        );

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        let m = &cfg.models[0];
        assert_eq!(m.input_price, 0.0);
        assert_eq!(m.cached_input_price, 0.0);
        assert_eq!(m.output_price, 0.0);
    }

    #[test]
    fn test_load_tui_config() {
        let dir = tempfile::TempDir::new().unwrap();

        write_file(
            &dir.path().join("config.toml"),
            r#"
[tui]
tools_expanded = true
show_thinking = true
"#,
        );

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        assert!(cfg.tui.tools_expanded);
        assert!(cfg.tui.show_thinking);
    }

    #[test]
    fn test_load_secrets_slack_merge() {
        let dir = tempfile::TempDir::new().unwrap();

        write_file(
            &dir.path().join("secrets.toml"),
            r#"
[slack]
bot_token = "xoxb-123"
app_token = "xapp-456"
"#,
        );

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(cfg.slack.bot_token, "xoxb-123");
        assert_eq!(cfg.slack.app_token, "xapp-456");
    }

    #[test]
    fn test_load_secrets_services_merge() {
        let dir = tempfile::TempDir::new().unwrap();

        write_file(
            &dir.path().join("secrets.toml"),
            r#"
[services.picsel]
connection_string = "postgres://localhost:5432/db"
api_key = "svc-key"
"#,
        );

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        let picsel = cfg.services.get("picsel").unwrap();
        assert!(picsel
            .get("connection_string")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("postgres://"));
    }

    #[test]
    fn test_load_secrets_does_not_override_config() {
        let dir = tempfile::TempDir::new().unwrap();

        write_file(
            &dir.path().join("config.toml"),
            "[agent]\nmax_tool_rounds = 10\n",
        );
        write_file(
            &dir.path().join("secrets.toml"),
            "[agent]\nmax_tool_rounds = 77\n",
        );

        let cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(cfg.agent.max_tool_rounds, 10);
    }

    #[test]
    fn test_save_config_secrets_not_leaked() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg_path = dir.path().join("config.toml");
        write_file(
            &cfg_path,
            r#"
[agent]
max_tool_rounds = 10

[slack]
bot_token = "xoxb-old"
app_token = "xapp-old"

[telegram]
bot_token = "tele-old"
allowed_users = [123]
"#,
        );

        let mut cfg = load_from(&dir.path().to_string_lossy()).unwrap();
        cfg.slack.bot_token = "SECRET-xoxb".to_string();
        cfg.slack.app_token = "SECRET-xapp".to_string();
        cfg.telegram.bot_token = "SECRET-tele".to_string();

        save_config(&dir.path().to_string_lossy(), &cfg).unwrap();

        let written = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(!written.contains("SECRET"));
        assert!(written.contains("allowed_users"));
        assert!(written.contains("max_tool_rounds = 10"));
    }

    #[test]
    fn test_save_config_empty_file() {
        let dir = tempfile::TempDir::new().unwrap();

        let cfg = Config::default_for(&dir.path().to_string_lossy());
        save_config(&dir.path().to_string_lossy(), &cfg).unwrap();

        let cfg_path = dir.path().join("config.toml");
        let data = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(data.contains("max_tool_rounds"));
    }

    #[test]
    fn test_save_config_does_not_leak_service_secrets() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path();
        write_file(&p.join("config.toml"), "[agent]\nmax_tool_rounds = 5\n");
        write_file(
            &p.join("secrets.toml"),
            "[services.picsel]\nconnection_string = \"postgres://secret-db\"\napi_key = \"SVC-SECRET-KEY\"\n",
        );

        let cfg = load_from(&p.to_string_lossy()).unwrap();
        save_config(&p.to_string_lossy(), &cfg).unwrap();

        let written = std::fs::read_to_string(p.join("config.toml")).unwrap();
        assert!(!written.contains("secret-db"));
        assert!(!written.contains("SVC-SECRET-KEY"));
    }
}
