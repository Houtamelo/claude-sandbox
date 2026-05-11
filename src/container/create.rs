use std::path::Path;

use crate::config::ConfigFile;
use crate::env;
use crate::error::Result;
use crate::mounts::{
    assert_no_target_collisions, default_volumes, extra_volumes, toml_mount,
};
use crate::podman::args::{create_args, CreateSpec};
use crate::podman::runner::Podman;

/// Content-hash of `<project>/.claude-sandbox.toml` as a 16-hex-digit FNV-1a
/// digest, or `None` if the project has no toml. Used to detect "the user
/// edited the config since this container was created" so we can auto-
/// recreate (mounts/env/labels/etc are baked at create time and won't
/// otherwise pick up the change).
pub fn toml_content_hash(project: &Path) -> Option<String> {
    let path = project.join(".claude-sandbox.toml");
    let bytes = std::fs::read(&path).ok()?;
    Some(fnv1a_64_hex(&bytes))
}

fn fnv1a_64_hex(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

/// Read a single label off an existing container via `podman inspect`.
/// Returns `Ok(None)` if the label is absent or the inspect output is
/// shaped unexpectedly (treat unknown labels as "no value" rather than
/// blowing up — worst case we recreate once).
fn container_label(podman: &Podman, name: &str, key: &str) -> Result<Option<String>> {
    let v = podman.run_json(&crate::podman::args::inspect_args(name))?;
    Ok(v.get("Config")
        .and_then(|c| c.get("Labels"))
        .and_then(|l| l.get(key))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string()))
}

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

/// Run the per-project dependency script as container root if it exists.
/// File: `<project>/.claude-sandbox.deps.sh`. Lives alongside the toml.
/// Editable by agents (rw via grant_acls), re-run on every container
/// creation so deps survive `claude-sandbox down` + recreate.
///
/// Script is executed as `sudo bash /work/.claude-sandbox.deps.sh`, so
/// commands inside don't need sudo prefixes. Abort-on-failure so the
/// agent sees the error and can fix the script.
pub fn run_deps_script(podman: &Podman, name: &str, project: &Path) -> Result<()> {
    let script = project.join(".claude-sandbox.deps.sh");
    if !script.exists() {
        return Ok(());
    }
    // Container must be running for exec.
    podman.run(&crate::podman::args::start_args(name))?;
    let script_in_container = script.display().to_string();
    let args = crate::podman::args::exec_args_as(
        name,
        Some("0"),
        false,
        &["bash", &script_in_container],
    );
    podman.run(&args)?;
    Ok(())
}

/// Grant the in-container `claude` user write access to the bind-mounted
/// project dir, `~/.claude`, and any user-declared rw mounts. ACLs are
/// additive (no ownership change); the host inode gains an entry for the
/// container-user's mapped sub-uid — harmless metadata to the host user
/// (still owns the file).
///
/// Why this is needed: in userns rootless mode, container `claude`
/// (UID 1000 inside) maps to host sub-uid 100999. Files owned by the
/// host user (UID 1000) with mode 600 — like `~/.pulumi/credentials.json`
/// — are denied to host UID 100999 without an explicit ACL.
///
/// Safe to call on every start. Best-effort: failures are silently
/// swallowed (`2>/dev/null` per `setfacl` call) so an exotic host path
/// the user mounted doesn't fail the whole bootstrap. Trailing `true`
/// keeps the bash script's exit code at zero.
pub fn grant_acls(
    podman: &Podman,
    name: &str,
    project: &Path,
    user_mounts: &[crate::config::MountSpec],
) -> Result<()> {
    let home = crate::mounts::container_home();
    // Bundled paths (always rw): project dir + Claude Code state dirs.
    let mut cmd = format!(
        "setfacl -R -m u:{user}:rwx -m d:u:{user}:rwx \
            {project} {home}/.claude {home}/.cache/claude-cli-nodejs {home}/.cache/claude 2>/dev/null; \
         setfacl -m u:{user}:rw {home}/.claude.json 2>/dev/null; ",
        user = crate::mounts::CONTAINER_USER,
        home = home.display(),
        project = project.display(),
    );
    // User-declared rw mounts (e.g. `~/.pulumi`, `~/.aws`, `~/.config/gcloud`).
    // Skip ro mounts: agent doesn't need write, and we'd rather not add an
    // ACL entry to a user-protected file unnecessarily.
    for m in user_mounts {
        if m.ro {
            continue;
        }
        let path = crate::paths::expand(&m.container);
        cmd.push_str(&format!(
            "[ -d {path} ] && setfacl -R -m u:{user}:rwx -m d:u:{user}:rwx {path} 2>/dev/null \
             || setfacl -m u:{user}:rw {path} 2>/dev/null; ",
            path = path,
            user = crate::mounts::CONTAINER_USER,
        ));
    }
    cmd.push_str("true");
    let args = crate::podman::args::exec_args_as(name, Some("0"), false, &["bash", "-c", &cmd]);
    let _ = podman.run(&args);
    Ok(())
}

pub fn ensure_container(podman: &Podman, opts: &CreateOptions) -> Result<bool> {
    let current_hash = toml_content_hash(opts.project_path);
    if podman.container_exists(opts.name)? {
        // Compare the toml hash baked into the existing container's
        // `cs-toml-hash` label to what's on disk now. If they match,
        // the config hasn't changed since this container was created
        // and we can keep using it. If they differ — or the label is
        // absent (legacy container from before this feature) — recreate
        // so the new mounts/env/ports take effect. The named home
        // volume (`cs-<name>-home`) is NOT removed by `rm --volumes`,
        // so the in-container `$HOME` survives the recreate.
        let existing_hash = container_label(podman, opts.name, "cs-toml-hash")
            .unwrap_or(None);
        if existing_hash.as_deref() == current_hash.as_deref() {
            return Ok(false);
        }
        crate::step!(
            "Configuration changed — recreating container (named home volume survives)"
        );
        podman.run(&crate::podman::args::rm_args(opts.name))?;
        let _ = crate::registry::remove(opts.name);
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
    // Expose the project path inside so `cs` and other tools can locate
    // the project root without relying on a hardcoded /work.
    env_pairs.push((
        "CS_PROJECT_PATH".into(),
        opts.project_path.display().to_string(),
    ));
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

    crate::step!("Creating container '{}' from image '{}'", opts.name, opts.image);
    let network = opts.config.network.as_deref().unwrap_or("bridge");
    // Workdir is the project's host path (same as the bind-mount target),
    // so claude's session-CWD matches between in- and out-of-container.
    let workdir = opts.project_path.to_path_buf();
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
        toml_hash: current_hash.as_deref(),
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
