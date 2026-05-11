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

pub fn default_volumes(project_path: &Path, container_name: &str) -> Vec<Volume> {
    let home = paths::home();
    let mut v = vec![
        Volume::Bind(Mount {
            host: project_path.to_path_buf(),
            container: PathBuf::from("/work"),
            ro: false,
        }),
        Volume::Bind(Mount {
            host: home.join(".claude"),
            container: PathBuf::from("/root/.claude"),
            ro: false,
        }),
        Volume::Named {
            name: format!("cs-{}-home", container_name),
            container: PathBuf::from("/root"),
            ro: false,
        },
    ];
    let gitconfig = home.join(".gitconfig");
    if gitconfig.exists() {
        v.push(Volume::Bind(Mount {
            host: gitconfig,
            container: PathBuf::from("/root/.gitconfig"),
            ro: true,
        }));
    }
    v
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
