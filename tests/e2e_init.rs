//! `claude-sandbox init` writes a minimal `.claude-sandbox.toml` in cwd
//! so the directory can be used as a sandbox project without `git init`.

mod common;

use std::path::PathBuf;

use common::Sandbox;

fn run_cli_in(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new(Sandbox::bin())
        .args(args)
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("spawn claude-sandbox")
}

/// `init` requires no podman / no image, so it's safe to run without E2E gating.
#[test]
fn init_creates_toml_in_fresh_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let toml = tmp.path().join(".claude-sandbox.toml");
    assert!(!toml.exists(), "precondition: no toml");

    let out = run_cli_in(tmp.path(), &["init"]);
    assert!(
        out.status.success(),
        "init exited non-zero. stdout: {} stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("initialized"),
        "stdout missing 'initialized': {stdout}"
    );
    assert!(toml.exists(), "toml not created");
    let body = std::fs::read_to_string(&toml).unwrap();
    assert!(body.contains("name = "), "toml missing name field: {body}");
}

#[test]
fn init_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let toml = tmp.path().join(".claude-sandbox.toml");
    std::fs::write(&toml, "# custom\nname = \"keep-me\"\n").unwrap();

    let out = run_cli_in(tmp.path(), &["init"]);
    assert!(out.status.success(), "init should succeed when toml exists");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("already initialized"),
        "stdout should say already-initialized: {stdout}"
    );
    let body = std::fs::read_to_string(&toml).unwrap();
    assert!(
        body.contains("\"keep-me\"") && body.contains("# custom"),
        "existing toml was clobbered: {body}"
    );
}

#[test]
fn error_when_no_marker_hints_at_init() {
    // A subdir of a non-project dir → should error AND hint at `init`.
    let tmp = tempfile::tempdir().unwrap();
    // tempdir is under /tmp which is not a git repo or marker dir,
    // and tmp.path() itself is empty, so any subcommand needing project
    // context will hit ProjectNotFound.
    let out = run_cli_in(tmp.path(), &["status"]);
    assert!(!out.status.success(), "status should fail with no project");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("project not found"),
        "stderr missing 'project not found': {stderr}"
    );
    assert!(
        stderr.contains("claude-sandbox init"),
        "stderr missing init hint: {stderr}"
    );
    assert!(
        stderr.contains("git init"),
        "stderr missing git init hint: {stderr}"
    );
}

#[test]
fn after_init_main_path_works() {
    // After `init`, the project should be usable for `status`.
    let tmp = tempfile::tempdir().unwrap();

    let out = run_cli_in(tmp.path(), &["init"]);
    assert!(out.status.success());

    let out = run_cli_in(tmp.path(), &["status"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "status after init failed.\nstdout: {stdout}\nstderr: {stderr}",
    );
    assert!(
        stdout.contains("absent"),
        "expected 'absent' (no container yet) in status: {stdout}"
    );
    // Don't leave the toml around (tempdir cleans up the dir itself,
    // but we want the test order-independent).
    let _ = std::fs::remove_file(tmp.path().join(".claude-sandbox.toml"));
    // suppress unused warning
    let _: PathBuf = tmp.path().to_path_buf();
}
