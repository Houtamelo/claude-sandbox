use std::path::Path;

use crate::error::{Error, Result};
use crate::paths;

use super::ConfigFile;

pub fn load(path: &Path) -> Result<ConfigFile> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| Error::Config(format!("reading {}: {e}", path.display())))?;
    load_from_str(&raw, &path.display().to_string())
}

pub fn load_optional(path: &Path) -> Result<Option<ConfigFile>> {
    if !path.exists() {
        return Ok(None);
    }
    load(path).map(Some)
}

/// Parse + validate a config from an in-memory string. `source_label` is
/// used only in error messages (e.g. `<embedded>`, `/usr/share/...`).
pub fn load_from_str(contents: &str, source_label: &str) -> Result<ConfigFile> {
    let cfg: ConfigFile = toml::from_str(contents)
        .map_err(|e| Error::Config(format!("parsing {}: {e}", source_label)))?;
    validate(&cfg, Path::new(source_label))?;
    Ok(cfg)
}

pub fn validate(cfg: &ConfigFile, path: &Path) -> Result<()> {
    for m in &cfg.mount {
        // `~` and `$VAR` are accepted (and expanded at mount-build time)
        // because the project is bind-mounted at the same path inside as
        // outside — so `~/.foo` is unambiguous and the same on both sides.
        //
        // Optional mounts skip the absolute-path check: unresolved `$VAR`
        // is the expected state when the env var isn't set on this host
        // (e.g. `$SSH_AUTH_SOCK` on a headless box), and the mount gets
        // filtered out at build time via `spec_to_volume_optional`.
        if m.optional {
            continue;
        }
        let expanded = paths::expand(&m.container);
        if !std::path::Path::new(&expanded).is_absolute() {
            return Err(Error::Config(format!(
                "{}: mount.container '{}' must be absolute \
                 (after `~`/`$VAR` expansion)",
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
