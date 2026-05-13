use crate::config::ConfigFile;
use crate::paths;

/// Build the env-var list for `podman create --env ...`. Everything comes
/// from the merged ConfigFile: literal `env = { K = V }` entries (paths
/// expanded), `env_passthrough = [...]` (host values inherited iff set),
/// and `env_file = "..."` (KEY=VALUE lines from a file relative to the
/// project root).
///
/// Note: there used to be a hardcoded PulseAudio `PULSE_SERVER` assignment
/// here, paired with a hardcoded bind-mount in `mounts::default_volumes`.
/// Both moved into the shipped machine-wide `config.toml` as user-visible
/// recipes — see assets/default-config.toml. Same for SSH-agent forwarding
/// (was `ssh_agent: bool`) and GPG-agent forwarding (was `gpg_agent: bool`).
pub fn resolve(cfg: &ConfigFile, project: &std::path::Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
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
