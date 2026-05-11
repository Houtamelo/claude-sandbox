use std::path::Path;

use crate::config::ConfigFile;
use crate::env;
use crate::error::Result;
use crate::mounts::{
    assert_no_target_collisions, default_volumes, extra_volumes, toml_mount,
};
use crate::podman::args::{create_args, CreateSpec};
use crate::podman::runner::Podman;

pub struct CreateOptions<'a> {
    pub name: &'a str,
    pub image: &'a str,
    pub project_path: &'a Path,
    pub config: &'a ConfigFile,
}

pub fn run_setup(
    podman: &Podman,
    name: &str,
    project_path: &Path,
    setup: &[String],
) -> Result<()> {
    if setup.is_empty() {
        return Ok(());
    }
    // Container must be running for exec.
    podman.run(&crate::podman::args::start_args(name))?;
    // setup hooks run as root: they typically apt-install, modify /etc, etc.
    crate::hooks::run(
        podman,
        name,
        setup,
        &crate::hooks::HookEnv {
            project_name: name.to_string(),
            project_path: project_path.to_path_buf(),
            worktree_name: None,
        },
        true,
        crate::hooks::HookUser::Root,
    )?;
    Ok(())
}

/// Grant the in-container `claude` user write access to the bind-mounted
/// project dir and `~/.claude` so a non-root agent can edit existing
/// host-owned files. ACLs are additive (no ownership change) and propagate
/// to the host as entries for the userns sub-uid — harmless metadata.
///
/// Safe to call on every start. Best-effort: failures are logged but
/// non-fatal so a stale image without `acl` installed doesn't lock the
/// user out — they can `claude-sandbox rebuild` to fix.
pub fn grant_acls(podman: &Podman, name: &str) -> Result<()> {
    let cmd = format!(
        "setfacl -R -m u:{user}:rwx -m d:u:{user}:rwx /work {home}/.claude 2>/dev/null || true",
        user = crate::mounts::CONTAINER_USER,
        home = crate::mounts::CONTAINER_HOME,
    );
    let args = crate::podman::args::exec_args_as(name, Some("0"), false, &["bash", "-c", &cmd]);
    let _ = podman.run(&args);
    Ok(())
}

pub fn ensure_container(podman: &Podman, opts: &CreateOptions) -> Result<bool> {
    if podman.container_exists(opts.name)? {
        return Ok(false);
    }
    let mut volumes = default_volumes(opts.project_path, opts.name);
    volumes.extend(extra_volumes(opts.config, opts.project_path));
    if opts
        .project_path
        .join(".claude-sandbox.toml")
        .exists()
    {
        volumes.push(toml_mount(opts.project_path, opts.config.agent_writable));
    }

    let mut env_pairs = env::resolve(opts.config, opts.project_path);
    for k in crate::features::tailscale::passthrough_env(&opts.config.tailscale) {
        if let Ok(v) = std::env::var(&k) {
            env_pairs.push((k, v));
        }
    }
    if opts.config.ssh_agent.unwrap_or(true) {
        env::ensure_ssh_agent(&mut env_pairs, &mut volumes);
    }

    assert_no_target_collisions(&volumes)?;

    let port_requests: Vec<crate::network::PortRequest> = opts
        .config
        .ports
        .iter()
        .map(|s| crate::network::parse(s))
        .collect::<Result<Vec<_>>>()?;
    let ports = crate::network::resolve(&port_requests)?;

    let network = opts.config.network.as_deref().unwrap_or("bridge");
    let workdir = std::path::PathBuf::from("/work");
    let gpu_extras = crate::features::gpu::extra_args(opts.config.gpu);
    let spec = CreateSpec {
        name: opts.name,
        image: opts.image,
        volumes: &volumes,
        env: &env_pairs,
        network,
        ports: &ports,
        workdir: &workdir,
        extra: &gpu_extras,
    };
    podman.run(&create_args(&spec))?;
    let _ = crate::registry::upsert(opts.name, opts.project_path);

    for (req, mapping) in port_requests.iter().zip(ports.iter()) {
        match req.host {
            Some(p) if mapping.host != p => {
                eprintln!(
                    "port {} on host: requested {}, got {}",
                    mapping.container, p, mapping.host
                );
            }
            None => {
                eprintln!(
                    "port {} ephemeral: got {}",
                    mapping.container, mapping.host
                );
            }
            _ => {}
        }
    }

    Ok(true)
}
