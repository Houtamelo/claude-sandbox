use std::path::Path;

use crate::mounts::Volume;

#[derive(Debug, Clone)]
pub struct CreateSpec<'a> {
    pub name: &'a str,
    pub image: &'a str,
    pub volumes: &'a [Volume],
    pub env: &'a [(String, String)],
    pub network: &'a str,
    pub ports: &'a [PortMapping],
    pub workdir: &'a Path,
    pub extra: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortMapping {
    pub host: u16,
    pub container: u16,
}

pub fn create_args(spec: &CreateSpec) -> Vec<String> {
    let mut v: Vec<String> = vec![
        "create".into(),
        "--name".into(),
        spec.name.into(),
        "--workdir".into(),
        spec.workdir.display().to_string(),
        "--network".into(),
        spec.network.into(),
        "--init".into(),
        // Opt the container out of SELinux confinement (the rootless+userns
        // protections remain). Without this, bind-mounted host paths labeled
        // `user_tmp_t` / `user_home_t` are denied to the container's
        // `container_t` context on SELinux-enabled hosts (openSUSE, Fedora,
        // RHEL). `--security-opt label=disable` is per-container and does NOT
        // mutate host file labels (unlike `:z` / `:Z` mount flags).
        "--security-opt".into(),
        "label=disable".into(),
    ];
    for vol in spec.volumes {
        v.push("--volume".into());
        v.push(volume_arg(vol));
    }
    for (k, val) in spec.env {
        v.push("--env".into());
        v.push(format!("{}={}", k, val));
    }
    for p in spec.ports {
        v.push("--publish".into());
        v.push(format!("{}:{}", p.host, p.container));
    }
    v.extend(spec.extra.iter().cloned());
    v.push(spec.image.into());
    v.push("sleep".into());
    v.push("infinity".into());
    v
}

fn volume_arg(vol: &Volume) -> String {
    match vol {
        Volume::Bind(m) => format!(
            "{}:{}{}",
            m.host.display(),
            m.container.display(),
            if m.ro { ":ro" } else { "" }
        ),
        Volume::Named { name, container, ro } => format!(
            "{}:{}{}",
            name,
            container.display(),
            if *ro { ":ro" } else { "" }
        ),
    }
}

pub fn start_args(name: &str) -> Vec<String> {
    vec!["start".into(), name.into()]
}

pub fn stop_args(name: &str) -> Vec<String> {
    vec!["stop".into(), name.into()]
}

pub fn rm_args(name: &str) -> Vec<String> {
    vec!["rm".into(), "--force".into(), "--volumes".into(), name.into()]
}

pub fn exec_args(name: &str, interactive: bool, cmd: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = vec!["exec".into()];
    if interactive {
        v.push("-it".into());
    }
    v.push(name.into());
    v.extend(cmd.iter().map(|s| (*s).into()));
    v
}

pub fn inspect_args(name: &str) -> Vec<String> {
    vec!["inspect".into(), "--format".into(), "{{json .}}".into(), name.into()]
}
