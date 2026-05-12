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
    /// Image-build settings. Optional in the on-disk schema so
    /// machine.toml files predating this section still parse cleanly;
    /// `#[serde(default)]` fills in the canonical Debian Trixie value.
    #[serde(default)]
    pub image: ImageSpec,
    /// GPU passthrough settings. Optional for back-compat; defaults to
    /// `vendor = "none"` so existing machine.toml files don't grow GPU
    /// behavior they didn't ask for.
    #[serde(default)]
    pub gpu: GpuSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HostSpec {
    /// Linux UID the in-image `claude` user is created with. Should match
    /// the host UID running claude-sandbox so bind-mounted files map
    /// 1:1 through the user namespace.
    pub uid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct ImageSpec {
    /// The `FROM` line of the in-house Dockerfile, e.g. `debian:trixie-slim`
    /// (default), `ubuntu:24.04`, `linuxmintd/mint22-amd64`. Currently
    /// must be apt-based: the Dockerfile hardcodes `apt-get install …`
    /// and a Debian-codename Tailscale repo. Non-Debian apt distros
    /// (Ubuntu, Mint) work but the Tailscale install layer will fail
    /// on rebuild — disable Tailscale or stay on Debian Trixie if you
    /// need it baked into the image.
    pub base: String,
}

impl Default for ImageSpec {
    fn default() -> Self {
        Self { base: "debian:trixie-slim".into() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct GpuSpec {
    /// Host GPU vendor — drives the canonical podman flags emitted
    /// when a project sets `gpu = true`. See `features::gpu::GpuVendor`.
    pub vendor: crate::features::gpu::GpuVendor,
    /// Verbatim flags appended to whatever the vendor emits. Escape
    /// hatch for kernel-driver / userspace quirks the built-in recipes
    /// don't cover (e.g. `--device /dev/dri/renderD129` for a specific
    /// secondary GPU, or `--security-opt label=type:container_runtime_t`
    /// for esoteric SELinux setups). Applied for every vendor including
    /// `none` and `custom`.
    pub extra_args: Vec<String>,
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

/// Result of probing Anthropic's API to verify the OAuth token. Three
/// states because network failures are real and we don't want to brick
/// the user's workflow just because they're temporarily offline:
///   - `Valid`  → API accepted the token (HTTP != 401/403).
///   - `Invalid` → API rejected it (HTTP 401/403 with auth error).
///     User must re-run `claude-sandbox cfg`.
///   - `Unknown` → network failure or 5xx; couldn't determine. Caller
///     decides whether to warn-and-proceed or block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenValidation {
    Valid,
    Invalid { detail: String },
    Unknown { reason: String },
}

/// Convert the (curl-exit, HTTP-code-string) pair to a validation result.
/// Split out for unit-testing without making real network calls.
pub fn parse_validation(curl_exit_ok: bool, http_code: &str) -> TokenValidation {
    let code = http_code.trim();
    if !curl_exit_ok && code == "000" {
        return TokenValidation::Unknown { reason: "network unreachable / timeout".into() };
    }
    match code {
        "401" | "403" => TokenValidation::Invalid {
            detail: format!("Anthropic API returned HTTP {code}"),
        },
        "000" => TokenValidation::Unknown { reason: "network unreachable / timeout".into() },
        c if c.starts_with('5') => TokenValidation::Unknown {
            reason: format!("HTTP {c} from Anthropic (likely an outage on their side)"),
        },
        // Any other 2xx/3xx/4xx (e.g. 200 / 400 / 422) means auth passed —
        // the request body failed validation, which is fine; we only
        // care about the auth verdict.
        c if !c.is_empty() => TokenValidation::Valid,
        _ => TokenValidation::Unknown { reason: "curl produced no HTTP status".into() },
    }
}

/// Probe Anthropic's API with the given token to determine whether it's
/// still accepted. Uses `POST /v1/messages/count_tokens` with no body —
/// auth is checked before body validation, so an empty body yields:
///   - 401 → revoked or wrong token
///   - 400 / 422 → auth ok, body missing (we treat this as Valid)
///
/// Shells to `curl` (a host prereq), times out after 5s. Falls back to
/// `Unknown` on any transport failure so a flaky network doesn't lock
/// the user out of their own sandbox.
pub fn validate_oauth_token(token: &str) -> TokenValidation {
    let auth = format!("Authorization: Bearer {}", token);
    let output = std::process::Command::new("curl")
        .args([
            "-s", "-o", "/dev/null", "-m", "5",
            "-w", "%{http_code}",
            "-X", "POST",
            "-H", &auth,
            "-H", "anthropic-version: 2023-06-01",
            "https://api.anthropic.com/v1/messages/count_tokens",
        ])
        .output();
    let output = match output {
        Ok(o) => o,
        Err(e) => return TokenValidation::Unknown { reason: format!("curl invocation failed: {e}") },
    };
    let http_code = String::from_utf8_lossy(&output.stdout);
    parse_validation(output.status.success(), &http_code)
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
