use crate::config::{Config, BackendConfig};

pub fn route_backend<'a>(requested: &str, cfg: &'a Config) -> Option<&'a BackendConfig> {
    // Exact match first
    if let Some(t) = cfg.routes.get(requested) { return Some(t); }
    // Prefix match choose the longest matched key.
    let mut best: Option<(&String, &BackendConfig)> = None;
    for (k, v) in &cfg.routes {
        if requested.starts_with(k) {
            if let Some((bk, _)) = &best {
                if k.len() > bk.len() { best = Some((k, v)); }
            } else { best = Some((k, v)); }
        }
    }
    if let Some((_, v)) = best { return Some(v); }
    if let Some(d) = &cfg.default { return Some(d); }
    None
}

pub fn sanitize_address(raw: &str) -> String {
    // Take substring before first NUL (Forge/Fabric extra data), trim spaces, strip trailing dot, lower-case.
    let primary = raw.split('\0').next().unwrap_or(raw);
    primary.trim().trim_end_matches('.').to_ascii_lowercase()
}
