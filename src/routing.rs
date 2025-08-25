use crate::config::Config;

pub fn route_backend(requested: &str, cfg: &Config) -> Option<String> {
    // Exact match first
    if let Some(t) = cfg.routes.get(requested) { return Some(t.clone()); }
    // Prefix match (e.g., "mini-" matches entries) choose the longest matched key.
    let mut best: Option<(&String, &String)> = None;
    for (k, v) in &cfg.routes {
        if requested.starts_with(k) {
            if let Some((bk, _)) = &best {
                if k.len() > bk.len() { best = Some((k, v)); }
            } else { best = Some((k, v)); }
        }
    }
    if let Some((_, v)) = best { return Some(v.clone()); }
    if let Some(d) = &cfg.default { return Some(d.clone()); }
    None
}

pub fn sanitize_address(raw: &str) -> String {
    // Take substring before first NUL (Forge/Fabric extra data), trim spaces, strip trailing dot, lower-case.
    let primary = raw.split('\0').next().unwrap_or(raw);
    primary.trim().trim_end_matches('.').to_ascii_lowercase()
}
