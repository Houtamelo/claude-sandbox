use std::collections::BTreeMap;

use serde::Deserialize;

pub mod edit;
pub mod parse;

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub name: Option<String>,
    #[serde(default)]
    pub agent_writable: bool,
    pub image: Option<String>,

    #[serde(default)]
    pub mount: Vec<MountSpec>,

    #[serde(default)]
    pub env_passthrough: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub env_file: Option<String>,

    pub ssh_agent: Option<bool>,
    /// Forward the host's GPG home (`~/.gnupg/`) into the container at
    /// the matching path, rw. Default `false` because most projects
    /// don't need GPG and forwarding exposes the private keyring to
    /// the container. Set `gpg_agent = true` to enable for projects
    /// that sign commits / encrypt artefacts.
    pub gpg_agent: Option<bool>,
    /// Override the machine-wide `[claude] flags` from `machine.toml`.
    /// `None` (the default) means "use the machine setting". `Some([..])`
    /// REPLACES the machine setting entirely — if you want the
    /// `--dangerously-skip-permissions` default plus extra flags, list
    /// it explicitly: `claude_flags = ["--dangerously-skip-permissions", "--model", "..."]`.
    pub claude_flags: Option<Vec<String>>,
    pub network: Option<String>,
    #[serde(default)]
    pub ports: Vec<String>,

    #[serde(default)]
    pub gpu: bool,

    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default)]
    pub on_start: Vec<String>,
    #[serde(default)]
    pub on_stop: Vec<String>,
    #[serde(default)]
    pub worktree_setup: Vec<String>,

    #[serde(default)]
    pub limits: LimitsSpec,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountSpec {
    pub host: String,
    pub container: String,
    #[serde(default)]
    pub ro: bool,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsSpec {
    pub memory: Option<String>,
    pub cpus: Option<f32>,
}

impl ConfigFile {
    /// Merge `other` *into* `self`: `other`'s fields override `self`'s.
    /// List-typed fields are concatenated (`self` first, then `other`).
    pub fn merge_in(&mut self, other: ConfigFile) {
        if other.name.is_some() {
            self.name = other.name;
        }
        if other.agent_writable {
            self.agent_writable = true;
        }
        if other.image.is_some() {
            self.image = other.image;
        }
        self.mount.extend(other.mount);
        self.env_passthrough.extend(other.env_passthrough);
        for (k, v) in other.env {
            self.env.insert(k, v);
        }
        if other.env_file.is_some() {
            self.env_file = other.env_file;
        }
        if other.ssh_agent.is_some() {
            self.ssh_agent = other.ssh_agent;
        }
        if other.gpg_agent.is_some() {
            self.gpg_agent = other.gpg_agent;
        }
        // `claude_flags` is full-replace, not append: setting it
        // per-project means "use exactly this list", overriding the
        // machine default. Append semantics would force users to spell
        // out the `--dangerously-skip-permissions` baseline every time
        // they wanted to add one extra flag.
        if other.claude_flags.is_some() {
            self.claude_flags = other.claude_flags;
        }
        if other.network.is_some() {
            self.network = other.network;
        }
        self.ports.extend(other.ports);
        if other.gpu {
            self.gpu = true;
        }
        self.setup.extend(other.setup);
        self.on_start.extend(other.on_start);
        self.on_stop.extend(other.on_stop);
        self.worktree_setup.extend(other.worktree_setup);
        if other.limits.memory.is_some() {
            self.limits.memory = other.limits.memory;
        }
        if other.limits.cpus.is_some() {
            self.limits.cpus = other.limits.cpus;
        }
    }
}

pub fn load_merged(global: Option<&std::path::Path>, local: Option<&std::path::Path>) -> crate::error::Result<ConfigFile> {
    let mut cfg = ConfigFile::default();
    if let Some(p) = global {
        if let Some(g) = parse::load_optional(p)? {
            cfg.merge_in(g);
        }
    }
    if let Some(p) = local {
        if let Some(l) = parse::load_optional(p)? {
            cfg.merge_in(l);
        }
    }
    Ok(cfg)
}
