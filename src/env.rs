use crate::config::ConfigFile;
use crate::paths;

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
