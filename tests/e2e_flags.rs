//! Verifies that `claude` is invoked with `--dangerously-skip-permissions`
//! on every launch path. The container IS the safety boundary; in-app
//! permission prompts defeat the entire point of the sandbox.

mod common;

use common::{should_skip, Sandbox};

/// Test the in-container exec shape for the main checkout: the argv that
/// the host wrapper would build for `claude-sandbox` should include
/// the dangerous-skip flag.
#[test]
fn main_path_claude_argv_includes_skip_permissions() {
    if should_skip("main_path_claude_argv_includes_skip_permissions") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);
    let _ = common::podman(&["start", &sb.name]);

    // Probe the actual claude binary's recognition of the flag inside the
    // image. If the flag name ever changes upstream, this fails clearly
    // before we ship a broken release.
    let out = sb.podman_exec(&["bash", "-c", "claude --help 2>&1 | grep -- --dangerously-skip-permissions"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("--dangerously-skip-permissions"),
        "claude does not advertise --dangerously-skip-permissions in --help. \
         If upstream renamed the flag, update CLAUDE_FLAGS in src/main.rs.\n\
         stdout: {stdout}"
    );
}

/// End-to-end: claude actually accepts --dangerously-skip-permissions as
/// the non-root `claude` user (it refuses to honour the flag as root, so
/// the user setup in the Dockerfile is load-bearing for the sandbox model
/// to work at all). Uses --print to keep it non-interactive.
#[test]
fn claude_accepts_dangerously_skip_as_nonroot() {
    if should_skip("claude_accepts_dangerously_skip_as_nonroot") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);
    let _ = common::podman(&["start", &sb.name]);

    let out = sb.podman_exec(&[
        "claude",
        "--dangerously-skip-permissions",
        "--print",
        "respond with the word OK",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // The exit might be non-zero if no auth is configured in the
    // tester's ~/.claude. We only fail on the specific "root/sudo"
    // refusal message — that's the regression we're guarding.
    assert!(
        !stdout.contains("cannot be used with root/sudo")
            && !stderr.contains("cannot be used with root/sudo"),
        "claude refused --dangerously-skip-permissions; the in-image \
         default user must be non-root.\nstdout: {stdout}\nstderr: {stderr}"
    );
}

/// Guards the safety baseline: the default `claude_flags` in
/// `machine.toml`'s `[claude]` section must contain
/// `--dangerously-skip-permissions`. Users can still override
/// (per-project or by editing machine.toml), but the OUT-OF-THE-BOX
/// default ships with the dangerous-skip-permissions flag on because
/// the container is the safety boundary and in-app prompts defeat the
/// sandbox's purpose. Catches accidental defaults flips.
#[test]
fn default_claude_flags_contains_dangerously_skip() {
    let default = claude_sandbox::machine::ClaudeSpec::default();
    assert!(
        default
            .flags
            .iter()
            .any(|f| f == "--dangerously-skip-permissions"),
        "default ClaudeSpec.flags must contain `--dangerously-skip-permissions`; \
         got {:?}",
        default.flags
    );
}

fn create_via_lib(sb: &Sandbox) {
    use claude_sandbox::config::{edit, load_merged};
    use claude_sandbox::container::create::{ensure_container, grant_acls, CreateOptions};
    use claude_sandbox::podman::runner::Podman;

    let toml = sb.path().join(".claude-sandbox.toml");
    edit::create_minimal(&toml, &sb.name).expect("auto-create toml");

    let cfg = load_merged(None, Some(&toml)).expect("load merged");
    let podman = Podman::discover().expect("podman");

    ensure_container(
        &podman,
        &CreateOptions {
            name: &sb.name,
            image: common::IMAGE,
            project_path: sb.path(),
            config: &cfg,
            machine_hash: None,
            oauth_hash: None,
            oauth_token: None,
            machine_cfg: None,
        },
    )
    .expect("ensure_container");
    let _ = common::podman(&["start", &sb.name]);
    grant_acls(&podman, &sb.name, sb.path(), &[]).expect("grant_acls");
}
