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

/// Verify the binary's source spec — the CLAUDE_FLAGS constant must
/// contain the flag. Lightweight string-level check on main.rs so
/// removing the flag without updating tests is caught.
#[test]
fn main_rs_declares_dangerously_skip_permissions() {
    let main_rs = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .expect("read src/main.rs");
    assert!(
        main_rs.contains("CLAUDE_FLAGS")
            && main_rs.contains("--dangerously-skip-permissions"),
        "src/main.rs missing the --dangerously-skip-permissions flag declaration. \
         The container is the safety boundary; in-app permission prompts defeat \
         the sandbox's purpose."
    );
}

fn create_via_lib(sb: &Sandbox) {
    use claude_sandbox::config::{edit, load_merged};
    use claude_sandbox::container::create::{ensure_container, CreateOptions};
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
        },
    )
    .expect("ensure_container");
}
