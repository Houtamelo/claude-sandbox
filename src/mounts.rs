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
        Volume::Bind(Mount {
            host: project_path.to_path_buf(),
            container: PathBuf::from("/work"),
            ro: false,
        }),
        // Persistent claude state (credentials, settings, projects, sessions).
        Volume::Bind(Mount {
            host: home.join(".claude"),
            container: chome.join(".claude"),
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
    let gitconfig = home.join(".gitconfig");
    if gitconfig.exists() {
        v.push(Volume::Bind(Mount {
            host: gitconfig,
            container: chome.join(".gitconfig"),
            ro: true,
        }));
    }
    v
}

fn ensure_dir(p: PathBuf) -> PathBuf {
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn extra_volumes(cfg: &ConfigFile, project: &Path) -> Vec<Volume> {
    cfg.mount
        .iter()
        .map(|m| spec_to_volume(m, project))
        .collect()
}

fn spec_to_volume(m: &MountSpec, project: &Path) -> Volume {
    let host = if m.host.starts_with('/') || m.host.starts_with('~') || m.host.starts_with('$') {
        std::path::PathBuf::from(paths::expand(&m.host))
    } else {
        project.join(&m.host)
    };
    Volume::Bind(Mount {
        host,
        container: PathBuf::from(&m.container),
        ro: m.ro,
    })
}

pub fn toml_mount(project: &Path, agent_writable: bool) -> Volume {
    Volume::Bind(Mount {
        host: project.join(".claude-sandbox.toml"),
        container: PathBuf::from("/work/.claude-sandbox.toml"),
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
