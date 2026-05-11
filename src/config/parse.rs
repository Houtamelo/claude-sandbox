use std::path::Path;

use crate::error::{Error, Result};

use super::ConfigFile;

pub fn load(path: &Path) -> Result<ConfigFile> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| Error::Config(format!("reading {}: {e}", path.display())))?;
    let cfg: ConfigFile = toml::from_str(&raw)
        .map_err(|e| Error::Config(format!("parsing {}: {e}", path.display())))?;
    validate(&cfg, path)?;
    Ok(cfg)
}

pub fn load_optional(path: &Path) -> Result<Option<ConfigFile>> {
    if !path.exists() {
        return Ok(None);
    }
    load(path).map(Some)
}

pub fn validate(cfg: &ConfigFile, path: &Path) -> Result<()> {
    for m in &cfg.mount {
        if !std::path::Path::new(&m.container).is_absolute() {
            return Err(Error::Config(format!(
                "{}: mount.container '{}' must be absolute",
                path.display(),
                m.container
            )));
        }
    }
    if let Some(n) = &cfg.network {
        if !matches!(n.as_str(), "bridge" | "host" | "none") {
            return Err(Error::Config(format!(
                "{}: network '{}' must be one of: bridge, host, none",
                path.display(),
                n
            )));
        }
    }
    for p in &cfg.ports {
        let body = p.strip_prefix('!').unwrap_or(p);
        let (lhs, rhs) = body
            .split_once(':')
            .ok_or_else(|| Error::Config(format!("{}: bad port spec '{}'", path.display(), p)))?;
        if !lhs.is_empty() {
            lhs.parse::<u16>().map_err(|_| {
                Error::Config(format!("{}: bad host port in '{}'", path.display(), p))
            })?;
        }
        rhs.parse::<u16>().map_err(|_| {
            Error::Config(format!("{}: bad container port in '{}'", path.display(), p))
        })?;
    }
    Ok(())
}
