use std::path::PathBuf;

use super::path::{config_path, dir};
use super::types::{Config, Secrets};

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
