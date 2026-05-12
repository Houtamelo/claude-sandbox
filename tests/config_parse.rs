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
env_passthrough = ["PULUMI_ACCESS_TOKEN"]
env = { CARGO_TERM_COLOR = "always" }
env_file = ".env"
ssh_agent = false
network = "bridge"
ports = ["5173:5173", "!8080:8080", ":3000"]
gpu = true
setup = ["apt-get install -y x"]
worktree_setup = ["echo 1"]

[limits]
memory = "16g"
cpus = 4
"#);
    let c = load(&tmp.path().join("c.toml")).unwrap();
    assert!(c.agent_writable);
    assert_eq!(c.mount.len(), 1);
    assert_eq!(c.mount[0].host, "~/.config/pulumi");
    assert!(c.gpu);
    assert_eq!(c.limits.memory.as_deref(), Some("16g"));
    assert_eq!(c.limits.cpus, Some(4.0));
}

#[test]
fn gpg_agent_defaults_to_none_meaning_off() {
    // No `gpg_agent` field at all → None → callers `.unwrap_or(false)`
    // → off. Explicit opt-in only.
    let tmp = write("name = \"x\"\n");
    let c = load(&tmp.path().join("c.toml")).unwrap();
    assert_eq!(c.gpg_agent, None);
}

#[test]
fn claude_flags_default_to_none_meaning_inherit_machine() {
    let tmp = write("name = \"x\"\n");
    let c = load(&tmp.path().join("c.toml")).unwrap();
    assert_eq!(c.claude_flags, None);
}

#[test]
fn claude_flags_per_project_full_override() {
    // Per-project list REPLACES the machine-wide setting wholesale —
    // append semantics would force users to repeat the dangerous-skip
    // baseline every time they wanted a project-specific extra flag.
    // Confirmed via merge_in's `is_some()` override pattern.
    let tmp = write(r#"
name = "x"
claude_flags = ["--allowedTools", "Bash,Read,Edit"]
"#);
    let c = load(&tmp.path().join("c.toml")).unwrap();
    assert_eq!(
        c.claude_flags,
        Some(vec!["--allowedTools".into(), "Bash,Read,Edit".into()])
    );
}

#[test]
fn claude_flags_can_be_explicit_empty() {
    // Empty list ≠ None. None means "fall through to machine.toml";
    // Some([]) means "no flags at all" (e.g. user wants the in-app
    // permission UX back for this specific project).
    let tmp = write("name = \"x\"\nclaude_flags = []\n");
    let c = load(&tmp.path().join("c.toml")).unwrap();
    assert_eq!(c.claude_flags, Some(vec![]));
}

#[test]
fn gpg_agent_can_be_explicit() {
    let tmp = write("name = \"x\"\ngpg_agent = true\n");
    let c = load(&tmp.path().join("c.toml")).unwrap();
    assert_eq!(c.gpg_agent, Some(true));
}

/// Guards the clean-break removal of the built-in Tailscale feature.
/// Existing tomls with `[tailscale]` are intentionally broken; the
/// recipe at docs/recipes/tailscale.md shows how to install it via
/// .claude-sandbox.deps.sh + on_start hooks instead.
#[test]
fn rejects_legacy_tailscale_section() {
    let tmp = write(r#"
name = "p"

[tailscale]
enabled = true
"#);
    let e = load(&tmp.path().join("c.toml")).unwrap_err();
    let msg = format!("{e}");
    assert!(
        msg.contains("tailscale") || msg.contains("unknown"),
        "expected an unknown-field error mentioning `tailscale`; got: {msg}"
    );
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
fn accepts_tilde_in_container_target() {
    // Project bind-mounts at host absolute path, so HOME inside == HOME
    // outside — `~/.foo` is unambiguous on both sides of a mount.
    let tmp = write(r#"mount = [{ host = "~/.pulumi", container = "~/.pulumi" }]"#);
    let c = load(&tmp.path().join("c.toml")).expect("should accept ~ in container");
    assert_eq!(c.mount[0].container, "~/.pulumi");
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
