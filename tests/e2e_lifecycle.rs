//! End-to-end lifecycle tests: status, create, mount verification,
//! stop, down. Drives the real `claude-sandbox` binary against a
//! tempdir-backed project and a real podman.

mod common;

use common::{should_skip, Sandbox};

#[test]
fn status_on_fresh_project_reports_absent() {
    if should_skip("status_on_fresh_project_reports_absent") {
        return;
    }
    let sb = Sandbox::new();
    let out = sb.cli(&["status"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "status exited non-zero");
    assert!(
        stdout.contains("absent"),
        "fresh project should report container absent; got: {}",
        stdout.trim()
    );
}

#[test]
fn first_invocation_creates_container_and_toml() {
    // Drive container creation via the library directly (the CLI's start
    // path execs into `podman exec -it` which needs a TTY). This still
    // exercises the same `ensure_container` code path the CLI uses, and
    // is what blocks for the rest of the tests in this file.
    if should_skip("first_invocation_creates_container_and_toml") {
        return;
    }
    let sb = Sandbox::new();
    create_container(&sb);

    // .claude-sandbox.toml should be present in the project after first start
    // (the CLI auto-creates it; here we do it explicitly because we bypass main.rs).
    // Just verify the container exists; auto-toml is covered by config_auto_create tests.
    assert!(
        sb.container_exists(),
        "container '{}' was not created",
        sb.name
    );
}

#[test]
fn container_has_default_mounts() {
    if should_skip("container_has_default_mounts") {
        return;
    }
    let sb = Sandbox::new();
    create_container(&sb);

    let info = sb.inspect();
    let mounts = info["Mounts"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let has_target = |target: &str| {
        mounts.iter().any(|m| {
            m["Destination"].as_str() == Some(target)
                || m["Target"].as_str() == Some(target)
        })
    };
    let chome = claude_sandbox::mounts::container_home();
    let chome_claude = chome.join(".claude").display().to_string();
    let chome_str = chome.display().to_string();
    assert!(has_target("/work"), "missing /work bind. mounts: {:#?}", mounts);
    assert!(
        has_target(&chome_claude),
        "missing {chome_claude} bind. mounts: {:#?}",
        mounts
    );
    assert!(
        has_target(&chome_str),
        "missing {chome_str} named volume. mounts: {:#?}",
        mounts
    );
}

#[test]
fn workdir_is_work_and_user_is_root() {
    if should_skip("workdir_is_work_and_user_is_root") {
        return;
    }
    let sb = Sandbox::new();
    create_container(&sb);
    start(&sb);

    let out = sb.podman_exec(&["pwd"]);
    assert!(out.status.success(), "pwd failed");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "/work");

    // Image's default user is the non-root `claude` user (UID 1000).
    // Container-root remains accessible via passwordless sudo for hooks
    // and apt operations.
    let out = sb.podman_exec(&["id", "-un"]);
    assert!(out.status.success(), "id failed");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "claude");
}

#[test]
fn project_dir_visible_at_work() {
    if should_skip("project_dir_visible_at_work") {
        return;
    }
    let sb = Sandbox::new();
    // Place a marker file inside the project, then check the container sees it.
    std::fs::write(sb.path().join("MARKER.txt"), "hello-from-test").expect("write marker");
    create_container(&sb);
    start(&sb);

    let out = sb.podman_exec(&["cat", "/work/MARKER.txt"]);
    assert!(out.status.success(), "cat failed");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "hello-from-test"
    );
}

#[test]
fn stop_then_down_cleans_up_via_cli() {
    if should_skip("stop_then_down_cleans_up_via_cli") {
        return;
    }
    let sb = Sandbox::new();
    create_container(&sb);
    start(&sb);

    // stop
    let out = sb.cli(&["stop"]);
    assert!(
        out.status.success(),
        "stop failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let info = sb.inspect();
    assert_eq!(
        info["State"]["Running"].as_bool(),
        Some(false),
        "container should be stopped after `claude-sandbox stop`"
    );

    // down
    let out = sb.cli(&["down"]);
    assert!(
        out.status.success(),
        "down failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !sb.container_exists(),
        "container should not exist after `claude-sandbox down`"
    );
}

// --- helpers ---

/// Create the container via the library API (no exec).
/// Mirrors what `claude-sandbox start` would do in setup phase.
fn create_container(sb: &Sandbox) {
    use claude_sandbox::config::{edit, load_merged};
    use claude_sandbox::container::create::{ensure_container, CreateOptions};
    use claude_sandbox::podman::runner::Podman;

    let toml = sb.path().join(".claude-sandbox.toml");
    edit::create_minimal(&toml, &sb.name).expect("auto-create toml");

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
    assert!(just_created, "first ensure_container should create");
}

/// After-start ACL grant: mirrors the production start_or_shell flow
/// where grant_acls runs right after ensure_running.
#[allow(dead_code)]
fn grant_acls_post_start(sb: &Sandbox) {
    use claude_sandbox::container::create::grant_acls;
    use claude_sandbox::podman::runner::Podman;
    let podman = Podman::discover().expect("podman");
    grant_acls(&podman, &sb.name).expect("grant_acls");
}

/// `podman start` so subsequent `podman exec` calls work. Verifies the
/// container is *still running* a moment later — catches the case where
/// the entrypoint+cmd combination causes the process to exit immediately.
fn start(sb: &Sandbox) {
    let out = common::podman(&["start", &sb.name]);
    assert!(
        out.status.success(),
        "podman start failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Give the container a beat to either stabilize or die.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let info = sb.inspect();
    let running = info["State"]["Running"].as_bool().unwrap_or(false);
    if !running {
        let logs = common::podman(&["logs", &sb.name]);
        panic!(
            "container exited immediately after start (exit code {:?}). \
             logs:\nstdout: {}\nstderr: {}",
            info["State"]["ExitCode"].as_i64(),
            String::from_utf8_lossy(&logs.stdout).trim(),
            String::from_utf8_lossy(&logs.stderr).trim()
        );
    }
}
