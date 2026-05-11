//! Tests for the CLI's name-resolution and ls/rename paths — the
//! ones that should consult `.claude-sandbox.toml`'s `name` field
//! after a rename rather than the path-derived name.
//!
//! Catches bugs like: after `claude-sandbox rename`, `status` reporting
//! the wrong container; or `ls` missing containers because of a stale
//! name-pattern filter.

mod common;

use common::{should_skip, Sandbox};

#[test]
fn status_reports_running_after_create() {
    if should_skip("status_reports_running_after_create") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);
    let _ = common::podman(&["start", &sb.name]);

    let out = sb.cli(&["status"]);
    assert!(out.status.success(), "status exited non-zero");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("running"),
        "expected 'running' in status; got: {}",
        stdout
    );
    assert!(
        stdout.contains(&sb.name),
        "expected container name '{}' in status; got: {}",
        sb.name,
        stdout
    );
}

#[test]
fn ls_finds_managed_containers_via_label() {
    // Container names don't have a fixed prefix; `ls` must find them
    // via the `cs-managed=1` label that create_args adds.
    if should_skip("ls_finds_managed_containers_via_label") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);

    let out = sb.cli(&["ls"]);
    assert!(
        out.status.success(),
        "ls exited non-zero: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&sb.name),
        "ls did not list our container '{}'.\nstdout: {}",
        sb.name,
        stdout
    );
}

// --- helper ---

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
