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

// ---- OAuth token (separate file from machine.toml) ----
//
// Stored as a plain single-line file at ~/.config/claude-sandbox/oauth_token
// with mode 600. NOT mixed into machine.toml because:
//   1. It's a secret — easier to lock down a dedicated file than a config
//      that may be edited by `claude-sandbox cfg` re-runs and ends up
//      readable by anything that reads the config.
//   2. Rotating the token shouldn't trip cosmetic edits to machine.toml's
//      mtime or hash; the two concerns are independent.
//   3. Plain-text storage matches what `claude setup-token` emits, so the
//      user can `cat` and `cp` without a TOML round-trip in the way.
//
// Generated via `claude setup-token` on the host (browser OAuth flow);
// captured by the `cfg` wizard and injected into containers as
// `CLAUDE_CODE_OAUTH_TOKEN` at create time. Long-lived (~1y), doesn't
// rotate on use, so concurrent containers + host share auth cleanly
// without the refresh-rotation contention that plagues the shared
// `~/.claude/.credentials.json` file.

pub fn oauth_token_path() -> PathBuf {
    paths::config_dir().join("oauth_token")
}

pub fn oauth_token_exists() -> bool {
    oauth_token_path().exists()
}

/// Read the token from disk. None if the file is absent (user hasn't
/// configured one — host falls back to bind-mounted `.credentials.json`
/// the legacy way). Whitespace is trimmed to absorb the trailing newline
/// that `claude setup-token` adds when piped to a file.
pub fn load_oauth_token() -> Result<Option<String>> {
    let p = oauth_token_path();
    if !p.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&p)
        .map_err(|e| Error::Config(format!("reading {}: {e}", p.display())))?;
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(trimmed))
}

/// Write the token mode 600. Creates parent dir if needed. The unix
/// permissions are critical — anyone who can read the file holds a
/// year-long credential to the user's Anthropic subscription.
pub fn save_oauth_token(token: &str) -> Result<()> {
    use std::io::Write;
    let p = oauth_token_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::Config(format!("creating {}: {e}", parent.display())))?;
    }
    // Write via a tempfile + rename so a crash partway through doesn't
    // leave a half-written / world-readable file. set_permissions before
    // the rename so the final file lands with mode 600 atomically.
    let tmp = p.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)
            .map_err(|e| Error::Config(format!("creating {}: {e}", tmp.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            f.set_permissions(perms)
                .map_err(|e| Error::Config(format!("chmod {}: {e}", tmp.display())))?;
        }
        f.write_all(token.trim().as_bytes())
            .map_err(|e| Error::Config(format!("writing {}: {e}", tmp.display())))?;
        f.write_all(b"\n")
            .map_err(|e| Error::Config(format!("writing {}: {e}", tmp.display())))?;
    }
    std::fs::rename(&tmp, &p)
        .map_err(|e| Error::Config(format!("rename {} -> {}: {e}", tmp.display(), p.display())))?;
    Ok(())
}

/// Content hash of the oauth-token file. Used as a separate container
/// label (`cs-oauth-hash`) so token rotation triggers a container
/// recreate (env vars are baked at create time) WITHOUT triggering an
/// image rebuild (token doesn't appear in the Dockerfile).
///
/// Returns a stable sentinel when the file is absent — the absence
/// itself is part of the state we want to track, so transitioning
/// "no token → token configured" must invalidate the existing
/// container's label.
pub fn oauth_token_hash() -> String {
    let bytes = load_oauth_token()
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_bytes();
    fnv1a_64_hex(&bytes)
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
