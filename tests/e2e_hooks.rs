//! End-to-end tests for lifecycle hooks: setup (once on create),
//! on_start (every start), worktree_setup (per `cs worktree add`).

mod common;

use common::{should_skip, Sandbox};

#[test]
fn setup_hook_runs_once_on_create() {
    if should_skip("setup_hook_runs_once_on_create") {
        return;
    }
    let sb = Sandbox::new();

    // Setup hook writes a marker file inside the container.
    std::fs::write(
        sb.path().join(".claude-sandbox.toml"),
        format!(
            "name = \"{}\"\nsetup = [\"echo first-create > /tmp/setup-marker\"]\n",
            sb.name
        ),
    )
    .expect("write toml");

    create_with_setup(&sb);

    // Marker should exist inside the container.
    let out = sb.podman_exec(&["cat", "/tmp/setup-marker"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "marker file missing inside container.\nstdout: {stdout}\nstderr: {stderr}",
    );
    assert_eq!(stdout.trim(), "first-create");
}

#[test]
fn setup_hook_does_not_rerun_on_subsequent_start() {
    if should_skip("setup_hook_does_not_rerun_on_subsequent_start") {
        return;
    }
    let sb = Sandbox::new();

    // Append a line each time the setup hook fires.
    std::fs::write(
        sb.path().join(".claude-sandbox.toml"),
        format!(
            "name = \"{}\"\nsetup = [\"echo run >> /tmp/setup-log\"]\n",
            sb.name
        ),
    )
    .expect("write toml");

    create_with_setup(&sb);
    // Stop and re-start; the prepare path should not invoke setup again.
    let _ = common::podman(&["stop", &sb.name]);
    create_with_setup(&sb); // ensure_container is now a no-op because container exists

    // Bring it back up so we can exec.
    let _ = common::podman(&["start", &sb.name]);

    let out = sb.podman_exec(&["wc", "-l", "/tmp/setup-log"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let count: i32 = stdout
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(-1);
    assert_eq!(
        count, 1,
        "setup hook should run exactly once across creates; log was: {}",
        stdout
    );
}

// --- helpers ---

fn create_with_setup(sb: &Sandbox) {
    use claude_sandbox::config::load_merged;
    use claude_sandbox::container::create::{ensure_container, run_setup, CreateOptions};
    use claude_sandbox::podman::runner::Podman;

    let toml = sb.path().join(".claude-sandbox.toml");
    let cfg = load_merged(None, Some(&toml)).expect("load merged");
    let podman = Podman::discover().expect("podman");

    let just_created = ensure_container(
        &podman,
        &CreateOptions {
            name: &sb.name,
            image: common::IMAGE,
            project_path: sb.path(),
            config: &cfg,
        },
    )
    .expect("ensure_container");
    if just_created {
        run_setup(&podman, &sb.name, sb.path(), &cfg.setup).expect("setup");
    }
}
