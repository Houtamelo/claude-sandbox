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
    /// Content hash of `.claude-sandbox.toml`. Stored as a container label
    /// so a subsequent `claude-sandbox start` can detect that the config
    /// has changed and trigger an automatic rm+recreate (named home
    /// volume survives). `None` when the project has no toml.
    pub toml_hash: Option<&'a str>,
    /// Content hash of `machine.toml`. Stored as a container label so
    /// changes to host-wide setup (currently just UID) trigger the same
    /// rm+recreate path. Always Some(_) in production (the gate
    /// guarantees machine.toml exists); tests can pass None.
    pub machine_hash: Option<&'a str>,
    /// Content hash of the OAuth token file (or sentinel for absent).
    /// Separate label `cs-oauth-hash` so rotating the token triggers a
    /// recreate (env vars are baked at create time) but NOT an image
    /// rebuild (the token doesn't appear in the Dockerfile).
    pub oauth_hash: Option<&'a str>,
    /// True when the host kernel has SELinux loaded — emits
    /// `--security-opt label=disable` to opt the container out of
    /// SELinux confinement (without mutating host file labels). False
    /// on Ubuntu / Mint / vanilla Arch where the flag is a no-op or
    /// warning-trigger.
    pub selinux: bool,
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
        // Marker label so `claude-sandbox ls` can find every container we own
        // regardless of its derived name (which has no fixed prefix).
        "--label".into(),
        "cs-managed=1".into(),
    ];
    if spec.selinux {
        // Opt the container out of SELinux confinement (the rootless+userns
        // protections remain). Without this, bind-mounted host paths labeled
        // `user_tmp_t` / `user_home_t` are denied to the container's
        // `container_t` context on SELinux-enabled hosts (openSUSE, Fedora,
        // RHEL). `--security-opt label=disable` is per-container and does NOT
        // mutate host file labels (unlike `:z` / `:Z` mount flags).
        v.push("--security-opt".into());
        v.push("label=disable".into());
    }
    if let Some(h) = spec.toml_hash {
        v.push("--label".into());
        v.push(format!("cs-toml-hash={h}"));
    }
    if let Some(h) = spec.machine_hash {
        v.push("--label".into());
        v.push(format!("cs-machine-hash={h}"));
    }
    if let Some(h) = spec.oauth_hash {
        v.push("--label".into());
        v.push(format!("cs-oauth-hash={h}"));
    }
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
    exec_args_as(name, None, interactive, cmd)
}

/// `podman exec` builder with an optional `--user <user>` override.
/// Pass `Some("0")` to run as container root (needed for setup hooks,
/// apt installs, tailscaled, etc.). `None` uses the image's default
/// user (the `claude` user from the Dockerfile).
pub fn exec_args_as(
    name: &str,
    user: Option<&str>,
    interactive: bool,
    cmd: &[&str],
) -> Vec<String> {
    let mut v: Vec<String> = vec!["exec".into()];
    if interactive {
        v.push("-it".into());
    }
    if let Some(u) = user {
        v.push("--user".into());
        v.push(u.into());
    }
    v.push(name.into());
    v.extend(cmd.iter().map(|s| (*s).into()));
    v
}

pub fn inspect_args(name: &str) -> Vec<String> {
    vec!["inspect".into(), "--format".into(), "{{json .}}".into(), name.into()]
}
