use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub listen: String,
    /// Map of route key (lowercased) to backend address (host:port)
    pub routes: HashMap<String, String>,
    /// Optional default backend if no route matches
    pub default: Option<String>,
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        let path = path.unwrap_or_else(|| PathBuf::from("mc-router.toml"));
        let text = fs::read_to_string(&path).with_context(|| format!("reading {:?}", path))?;
        let mut cfg: Config = toml::from_str(&text).with_context(|| "parsing config")?;
        // normalize keys
        let routes = cfg
            .routes
            .drain()
            .map(|(k, v)| (k.to_ascii_lowercase(), v))
            .collect();
        cfg.routes = routes;
        Ok(cfg)
    }
}
