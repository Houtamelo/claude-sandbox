//! Three-tier lookup for the companion assets we ship alongside the
//! binary (Dockerfile, default-config.toml).
//!
//! Order, highest to lowest priority:
//! 1. `~/.config/claude-sandbox/<name>` — user override (populated on
//!    demand via [`populate_user_config`] from the cfg wizard).
//! 2. `$CS_SYSTEM_DATA_DIR/<name>` (default `/usr/share/claude-sandbox`)
//!    — the FHS-friendly location where a distro package drops these.
//! 3. Embedded into the binary via `include_str!` — guarantees
//!    `cargo install --path .` and similar workflows work without
//!    needing any out-of-tree file placement.
//!
//! The system tier is overridable so tests and packagers can pin it.

use std::path::PathBuf;

use crate::paths;

pub const DOCKERFILE_NAME: &str = "Dockerfile";
pub const DEFAULT_CONFIG_NAME: &str = "config.toml";

pub const EMBEDDED_DOCKERFILE: &str = include_str!("../assets/Dockerfile");
pub const EMBEDDED_DEFAULT_CONFIG: &str = include_str!("../assets/default-config.toml");

pub fn system_data_dir() -> PathBuf {
    match std::env::var_os("CS_SYSTEM_DATA_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from("/usr/share/claude-sandbox"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetSource {
    UserOverride(PathBuf),
    System(PathBuf),
    Embedded,
}

#[derive(Debug, Clone)]
pub struct ResolvedAsset {
    pub source: AssetSource,
    pub contents: String,
}

fn resolve(name: &str, embedded: &'static str) -> std::io::Result<ResolvedAsset> {
    let user = paths::config_dir().join(name);
    if user.is_file() {
        let contents = std::fs::read_to_string(&user)?;
        return Ok(ResolvedAsset { source: AssetSource::UserOverride(user), contents });
    }
    let sys = system_data_dir().join(name);
    if sys.is_file() {
        let contents = std::fs::read_to_string(&sys)?;
        return Ok(ResolvedAsset { source: AssetSource::System(sys), contents });
    }
    Ok(ResolvedAsset { source: AssetSource::Embedded, contents: embedded.to_string() })
}

pub fn resolve_dockerfile() -> std::io::Result<ResolvedAsset> {
    resolve(DOCKERFILE_NAME, EMBEDDED_DOCKERFILE)
}

pub fn resolve_default_config() -> std::io::Result<ResolvedAsset> {
    resolve(DEFAULT_CONFIG_NAME, EMBEDDED_DEFAULT_CONFIG)
}

/// Copies the embedded defaults into `~/.config/claude-sandbox/`. Used by
/// the cfg wizard's opt-in "copy defaults for editing" step.
///
/// When `force` is false, files that already exist are left untouched and
/// omitted from the returned list (so the wizard can show what actually
/// changed). When true, existing files are overwritten.
pub fn populate_user_config(force: bool) -> std::io::Result<Vec<PathBuf>> {
    let dir = paths::config_dir();
    std::fs::create_dir_all(&dir)?;
    let mut written = Vec::new();
    for (name, contents) in [
        (DOCKERFILE_NAME, EMBEDDED_DOCKERFILE),
        (DEFAULT_CONFIG_NAME, EMBEDDED_DEFAULT_CONFIG),
    ] {
        let p = dir.join(name);
        if !force && p.exists() {
            continue;
        }
        std::fs::write(&p, contents)?;
        written.push(p);
    }
    Ok(written)
}
