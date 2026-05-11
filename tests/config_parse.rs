use std::fs;

use tempfile::tempdir;

use claude_sandbox::config::parse::load;
use claude_sandbox::config::ConfigFile;

fn write(content: &str) -> tempfile::TempDir {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("c.toml"), content).unwrap();
    tmp
}

#[test]
fn parses_minimal() {
    let tmp = write("name = \"x\"\n");
    let c = load(&tmp.path().join("c.toml")).unwrap();
    assert_eq!(c.name.as_deref(), Some("x"));
    assert!(!c.agent_writable);
    assert!(c.mount.is_empty());
    assert_eq!(c.tailscale.authkey_env, "TS_AUTHKEY");
}

#[test]
fn parses_full() {
    let tmp = write(r#"
name = "p"
agent_writable = true
image = "claude-sandbox:0.1"
mount = [
  { host = "~/.config/pulumi", container = "/root/.config/pulumi", ro = true },
]
env_passthrough = ["TS_AUTHKEY"]
env = { CARGO_TERM_COLOR = "always" }
env_file = ".env"
ssh_agent = false
network = "bridge"
ports = ["5173:5173", "!8080:8080", ":3000"]
gpu = true
setup = ["apt-get install -y x"]
worktree_setup = ["echo 1"]

[tailscale]
enabled = true
hostname = "h"

[limits]
memory = "16g"
cpus = 4
"#);
    let c = load(&tmp.path().join("c.toml")).unwrap();
    assert!(c.agent_writable);
    assert_eq!(c.mount.len(), 1);
    assert_eq!(c.mount[0].host, "~/.config/pulumi");
    assert_eq!(c.tailscale.enabled, true);
    assert!(c.gpu);
    assert_eq!(c.limits.memory.as_deref(), Some("16g"));
    assert_eq!(c.limits.cpus, Some(4.0));
}

#[test]
fn rejects_unknown_field() {
    let tmp = write("name = \"x\"\nunknown_field = 1\n");
    assert!(load(&tmp.path().join("c.toml")).is_err());
}

#[test]
fn rejects_relative_mount_target() {
    let tmp = write(r#"mount = [{ host = "/x", container = "relative" }]"#);
    let e = load(&tmp.path().join("c.toml")).unwrap_err();
    assert!(format!("{e}").contains("must be absolute"));
}

#[test]
fn rejects_bad_port() {
    let tmp = write("ports = [\"hello:world\"]\n");
    assert!(load(&tmp.path().join("c.toml")).is_err());
}

#[test]
fn rejects_bad_network() {
    let tmp = write("network = \"weird\"\n");
    assert!(load(&tmp.path().join("c.toml")).is_err());
}

#[test]
fn merge_overrides_scalars_and_concats_lists() {
    let mut a = ConfigFile::default();
    a.name = Some("a".into());
    a.setup = vec!["one".into()];

    let mut b = ConfigFile::default();
    b.name = Some("b".into());
    b.setup = vec!["two".into()];

    a.merge_in(b);
    assert_eq!(a.name.as_deref(), Some("b"));
    assert_eq!(a.setup, vec!["one".to_string(), "two".into()]);
}
