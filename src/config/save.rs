use super::load::load_from;
use super::path::config_path;
use super::types::Config;

pub fn save_tui(
    dir: &str,
    tools_expanded: bool,
    show_thinking: bool,
    show_subagent_calls: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut config = load_from(dir)?;
    config.tui.tools_expanded = tools_expanded;
    config.tui.show_thinking = show_thinking;
    config.tui.show_subagent_calls = show_subagent_calls;
    save_config(dir, &config)
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
