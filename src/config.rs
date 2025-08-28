use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf};

// Public canonical backend configuration after normalization.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct BackendConfig {
    pub address: String,
    pub use_haproxy: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub management_listen: Option<String>,
    pub listen: String,
    /// Map of route key (lowercased) to backend config
    pub routes: HashMap<String, BackendConfig>,
    /// Optional default backend if no route matches
    pub default: Option<BackendConfig>,
}

// Intermediate representation to allow backward-compatible parsing of either a simple
// string backend or a table with extra options.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
enum RawBackend {
    Simple(String),
    Detailed {
        address: String,
        #[serde(default, rename = "useHAProxy")]
        use_haproxy: bool,
    },
}

#[derive(Debug, Deserialize, Clone)]
struct RawConfig {
    management_listen: Option<String>,
    listen: String,
    routes: HashMap<String, RawBackend>,
    #[serde(default)]
    default: Option<RawBackend>,
}

impl From<RawBackend> for BackendConfig {
    fn from(r: RawBackend) -> Self {
        match r {
            RawBackend::Simple(address) => BackendConfig { address, use_haproxy: false },
            RawBackend::Detailed { address, use_haproxy } => BackendConfig { address, use_haproxy },
        }
    }
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        let path = path.unwrap_or_else(|| PathBuf::from("mc-router.toml"));
        let text = fs::read_to_string(&path).with_context(|| format!("reading {:?}", path))?;
        let mut raw: RawConfig = toml::from_str(&text).with_context(|| "parsing config")?;

        // normalize keys & convert
        let mut routes: HashMap<String, BackendConfig> = HashMap::new();
        for (k, v) in raw.routes.drain() {
            routes.insert(k.to_ascii_lowercase(), v.into());
        }
        let default = raw.default.map(|d| d.into());
        Ok(Config { management_listen: raw.management_listen, listen: raw.listen, routes, default })
    }
}
