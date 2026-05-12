//! Machine-wide setup for claude-sandbox.
//!
//! Lives at `~/.config/claude-sandbox/machine.toml` and answers host-
//! environment questions that the image needs at build time: what UID
//! the in-image user should be, whether SELinux is enabled, GPU vendor,
//! etc.
//!
//! Separate from `~/.config/claude-sandbox/config.toml` (which provides
//! defaults that merge into per-project `.claude-sandbox.toml`). The
//! two never propagate into the same place — `config.toml` shapes
//! per-project container creation, `machine.toml` shapes the image.
//!
//! Populated interactively via `claude-sandbox cfg`; every other
//! subcommand gates on its existence.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MachineConfig {
    pub host: HostSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HostSpec {
    /// Linux UID the in-image `claude` user is created with. Should match
    /// the host UID running claude-sandbox so bind-mounted files map
    /// 1:1 through the user namespace.
    pub uid: u32,
}

pub fn path() -> PathBuf {
    paths::config_dir().join("machine.toml")
}

pub fn exists() -> bool {
    path().exists()
}

pub fn load() -> Result<MachineConfig> {
    let p = path();
    let raw = std::fs::read_to_string(&p)
        .map_err(|e| Error::Config(format!("reading {}: {e}", p.display())))?;
    toml::from_str::<MachineConfig>(&raw)
        .map_err(|e| Error::Config(format!("parsing {}: {e}", p.display())))
}

pub fn save(cfg: &MachineConfig) -> Result<()> {
    let p = path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::Config(format!("creating {}: {e}", parent.display())))?;
    }
    let body = toml::to_string_pretty(cfg)
        .map_err(|e| Error::Config(format!("serializing machine.toml: {e}")))?;
    std::fs::write(&p, body)
        .map_err(|e| Error::Config(format!("writing {}: {e}", p.display())))?;
    Ok(())
}

/// Deterministic content hash. Re-serializes through `toml::to_string`
/// (canonical formatting) before hashing so cosmetic edits — comments,
/// whitespace, key reorders — don't trigger spurious image rebuilds.
/// Hash divergence here is what drives the auto-rebuild-on-change
/// machinery in `container::create::ensure_container`.
pub fn content_hash(cfg: &MachineConfig) -> String {
    let canonical = toml::to_string(cfg).unwrap_or_default();
    fnv1a_64_hex(canonical.as_bytes())
}

fn fnv1a_64_hex(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

/// Loud, descriptive error to surface from the host CLI's gate when
/// the user hasn't completed setup yet.
pub fn require_setup_done() -> Result<MachineConfig> {
    if !exists() {
        return Err(Error::Config(format!(
            "machine setup not done. Run `claude-sandbox cfg` to complete it.\n\
             Expected config at: {}",
            path().display()
        )));
    }
    load()
}
