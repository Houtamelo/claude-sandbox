use std::path::{Path, PathBuf};

use crate::config::{ConfigFile, MountSpec};
use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub host: PathBuf,
    pub container: PathBuf,
    pub ro: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Volume {
    Bind(Mount),
    Named { name: String, container: PathBuf, ro: bool },
}

/// The non-root user inside the container that Claude runs as.
/// Must match the user created in the image's Dockerfile.
pub const CONTAINER_USER: &str = "claude";

/// Path of the empty-credentials shadow file used to take the in-container
/// claude out of the OAuth refresh-token race pool (issue #27933). Calling
/// this function also materializes the file on disk so the bind-mount has
/// a target; idempotent across calls. Lives in the cache dir alongside
/// other transient claude-sandbox state.
///
/// Payload is the JSON object `{}` rather than zero bytes — claude-code
/// parses this file with a JSON parser and errors on empty input on some
/// versions. An empty object has no `claudeAiOauth` key, so claude-code
/// finds no refresh token and falls back to `CLAUDE_CODE_OAUTH_TOKEN`
/// for inference auth.
pub fn empty_credentials_path() -> PathBuf {
    let p = paths::cache_dir().join("empty-credentials.json");
    if !p.exists() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&p, "{}");
    }
    p
}

/// In-container HOME path. Matches the host user's HOME so Claude Code's
/// HOME-keyed setup-state cache (`~/.cache/claude-cli-nodejs/-<HOME->/`)
/// can be located via a simple bind-mount.
pub fn container_home() -> PathBuf {
    paths::home()
}

pub fn default_volumes(project_path: &Path, container_name: &str) -> Vec<Volume> {
    let home = paths::home();
    let chome = container_home();
    let mut v = vec![
        // Bind the project at its host path. Claude Code records the
        // session's CWD as an absolute path and refuses `--resume` when
        // the current CWD differs. Mounting host_path -> host_path keeps
        // paths identical inside and outside, so resume works.
        Volume::Bind(Mount {
            host: project_path.to_path_buf(),
            container: project_path.to_path_buf(),
            ro: false,
        }),
        // Persistent claude state (settings, agents, plugins, sessions).
        // Note: `.credentials.json` underneath this directory is
        // selectively shadowed below when `CLAUDE_CODE_OAUTH_TOKEN` is
        // configured — see the shadow-mount block after this vec.
        Volume::Bind(Mount {
            host: home.join(".claude"),
            container: chome.join(".claude"),
            ro: false,
        }),
        // Top-level state file — NOT inside ~/.claude/. Holds
        // hasCompletedOnboarding, userID, oauthAccount, tipsHistory, etc.
        // Without this, claude treats every session as first-run even
        // with all the directory state above bind-mounted.
        Volume::Bind(Mount {
            host: ensure_file(home.join(".claude.json")),
            container: chome.join(".claude.json"),
            ro: false,
        }),
        // Setup-state / onboarding cache; without this, claude treats every
        // container session as first-run and re-prompts for theme + login.
        // Create on host if absent so the bind-mount has something to point at.
        Volume::Bind(Mount {
            host: ensure_dir(home.join(".cache/claude-cli-nodejs")),
            container: chome.join(".cache/claude-cli-nodejs"),
            ro: false,
        }),
        Volume::Bind(Mount {
            host: ensure_dir(home.join(".cache/claude")),
            container: chome.join(".cache/claude"),
            ro: false,
        }),
        // Named volume for everything else under HOME (apt installs at user
        // level, shell history, claude binary install location, etc.).
        Volume::Named {
            name: format!("cs-{}-home", container_name),
            container: chome.clone(),
            ro: false,
        },
    ];
    // When the user has configured a long-lived `CLAUDE_CODE_OAUTH_TOKEN`
    // (via `claude-sandbox cfg`), shadow the bind-mounted `.credentials.json`
    // with an empty file so the in-container claude doesn't read the host's
    // refresh token. Without this, every container start can trigger a
    // refresh — racing other concurrent claude processes (host + other
    // sandboxes) and invalidating the entire OAuth refresh-token family
    // server-side per the well-known race condition in claude-code
    // (https://github.com/anthropics/claude-code/issues/27933).
    //
    // The container falls back to `CLAUDE_CODE_OAUTH_TOKEN` (injected as
    // an env var at create time) for inference auth. The shadow is read-
    // only: the container has no business writing to credentials.
    //
    // Skipped entirely when no OAuth token is configured — those users
    // still rely on the bind-mounted `.credentials.json` for in-container
    // auth, and shadowing would lock them out. Order matters: this mount
    // must come AFTER the `~/.claude/` directory mount above so podman
    // applies it on top.
    if crate::machine::oauth_token_exists() {
        v.push(Volume::Bind(Mount {
            host: empty_credentials_path(),
            container: chome.join(".claude/.credentials.json"),
            ro: true,
        }));
    }
    let gitconfig = home.join(".gitconfig");
    if gitconfig.exists() {
        v.push(Volume::Bind(Mount {
            host: gitconfig,
            container: chome.join(".gitconfig"),
            ro: true,
        }));
    }
    // PulseAudio / SSH-agent / GPG-agent forwarding used to be hardcoded
    // here. Now those are shipped as user-visible `[[mount]]` recipes in
    // the machine-wide config.toml (assets/default-config.toml), gated by
    // the new `optional = true` semantics so hosts that don't run those
    // services don't see parse errors or podman-create failures.
    v
}

fn ensure_dir(p: PathBuf) -> PathBuf {
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Touch a file so a bind-mount has a target to bind to. If the file
/// already exists (the common case for ~/.claude.json on real systems)
/// this is a no-op. Without this, podman would create a *directory*
/// at the bind-mount target and Claude would write to an empty file
/// inside it instead of the expected location.
fn ensure_file(p: PathBuf) -> PathBuf {
    if !p.exists() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::File::create(&p);
    }
    p
}

pub fn extra_volumes(cfg: &ConfigFile, project: &Path) -> Vec<Volume> {
    cfg.mount
        .iter()
        .filter_map(|m| spec_to_volume_optional(m, project))
        .collect()
}

/// Build a podman volume from a MountSpec. Required mounts (default)
/// always return Some — failure modes surface at podman-create time.
///
/// Optional mounts (`optional = true`) are filtered out when the host
/// path can't be made concrete: an unresolved `$VAR` leaves `$` in the
/// expansion, and a missing file/dir on disk means there's nothing to
/// bind. This lets the shipped machine-wide `config.toml` ship recipes
/// like `host = "$SSH_AUTH_SOCK"` without breaking on hosts that don't
/// run an SSH agent.
pub fn spec_to_volume_optional(m: &MountSpec, project: &Path) -> Option<Volume> {
    let host_raw = paths::expand(&m.host);
    if m.optional {
        // Unresolved $VAR — paths::expand leaves the literal `$NAME`
        // in place when the env var isn't set. Treat as "skip silently".
        if host_raw.contains('$') {
            return None;
        }
        // Resolved path that doesn't exist on disk — skip too.
        let resolved_for_check = if host_raw.starts_with('/') || host_raw.starts_with('~') {
            std::path::PathBuf::from(&host_raw)
        } else {
            project.join(&host_raw)
        };
        if !resolved_for_check.exists() {
            return None;
        }
    }
    let host = if m.host.starts_with('/') || m.host.starts_with('~') || m.host.starts_with('$') {
        std::path::PathBuf::from(host_raw)
    } else {
        project.join(&m.host)
    };
    Some(Volume::Bind(Mount {
        host,
        container: PathBuf::from(paths::expand(&m.container)),
        ro: m.ro,
    }))
}

pub fn toml_mount(project: &Path, agent_writable: bool) -> Volume {
    Volume::Bind(Mount {
        host: project.join(".claude-sandbox.toml"),
        container: project.join(".claude-sandbox.toml"),
        ro: !agent_writable,
    })
}

pub fn assert_no_target_collisions(volumes: &[Volume]) -> crate::error::Result<()> {
    use std::collections::HashMap;
    let mut seen: HashMap<&Path, ()> = HashMap::new();
    for v in volumes {
        let target = match v {
            Volume::Bind(m) => m.container.as_path(),
            Volume::Named { container, .. } => container.as_path(),
        };
        if seen.insert(target, ()).is_some() {
            return Err(crate::error::Error::Config(format!(
                "mount collision at {}",
                target.display()
            )));
        }
    }
    Ok(())
}
