use crate::config::ConfigFile;
use crate::paths;

pub fn resolve(cfg: &ConfigFile, project: &std::path::Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    // PulseAudio: point paplay/etc. at the bind-mounted host socket. The
    // socket is mounted unconditionally if it exists on the host (see
    // mounts::default_volumes); setting PULSE_SERVER ensures paplay finds
    // it even when XDG_RUNTIME_DIR isn't set inside.
    let uid = nix::unistd::Uid::current().as_raw();
    let pulse_sock = std::path::PathBuf::from(format!("/run/user/{uid}/pulse/native"));
    if pulse_sock.exists() {
        out.push((
            "PULSE_SERVER".into(),
            format!("unix:{}", pulse_sock.display()),
        ));
    }

    for (k, v) in &cfg.env {
        out.push((k.clone(), paths::expand(v)));
    }
    for k in &cfg.env_passthrough {
        if let Ok(v) = std::env::var(k) {
            out.push((k.clone(), v));
        }
    }
    if let Some(f) = &cfg.env_file {
        let p = project.join(f);
        if let Ok(s) = std::fs::read_to_string(&p) {
            for line in s.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    out.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
        }
    }
    out
}

pub fn ensure_ssh_agent(env: &mut Vec<(String, String)>, volumes: &mut Vec<crate::mounts::Volume>) {
    if let Some(sock) = crate::network::ssh_agent_socket() {
        volumes.push(crate::mounts::Volume::Bind(crate::mounts::Mount {
            host: sock,
            container: std::path::PathBuf::from("/ssh-agent.sock"),
            ro: false,
        }));
        env.push(("SSH_AUTH_SOCK".into(), "/ssh-agent.sock".into()));
    }
}

/// Bind-mount the host's `~/.gnupg/` into the container at the matching
/// path (HOME is already mirrored, so the same path resolves on both
/// sides). gpg-agent's socket lives at `~/.gnupg/S.gpg-agent` and is
/// discovered automatically by the in-container `gpg`; the public + private
/// keyring are also in there, which is necessary for key lookup by ID
/// or fingerprint.
///
/// Unlike the SSH agent forwarding (socket-only — keys never leave the
/// host's agent), this exposes the whole GPG keyring to the container,
/// including private key material on disk. That's a deliberate tradeoff:
/// most GPG operations need keyring metadata, and our security model
/// already treats the container as a permissive read environment for
/// the user's home (`~/.claude`, etc.). Opt-in per project via
/// `gpg_agent = true` (default false).
///
/// No-op when `~/.gnupg/` doesn't exist on the host (machine without
/// GPG configured) — surfacing as a missing-bind-source error would be
/// unhelpful noise.
pub fn ensure_gpg_agent(volumes: &mut Vec<crate::mounts::Volume>) {
    let gnupg = paths::home().join(".gnupg");
    if !gnupg.is_dir() {
        return;
    }
    volumes.push(crate::mounts::Volume::Bind(crate::mounts::Mount {
        host: gnupg.clone(),
        container: gnupg,
        ro: false,
    }));
}
