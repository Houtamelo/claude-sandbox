//! Regression: when the picker sends the user to an existing worktree
//! BUT no container exists yet, the worktree launch must still create
//! the container (not error with "no container for this project").
//!
//! Reproduces the user-reported bug: right-click → "Open in claude-sandbox"
//! on a fresh project that has worktrees → picker shows worktrees →
//! pick one → "error: no container for this project".

mod common;

use std::process::Command;

use common::{should_skip, Sandbox};

#[test]
fn picker_existing_worktree_creates_container_if_missing() {
    if should_skip("picker_existing_worktree_creates_container_if_missing") {
        return;
    }
    let sb = Sandbox::new();
    init_git_with_commit(sb.path());

    // Pre-create the worktree on host (mirrors `cs worktree add` from a
    // prior session — the worktree dir exists, but the container does not).
    Command::new("git")
        .args(["worktree", "add", "-b", "feat", ".worktrees/feat"])
        .current_dir(sb.path())
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .output().expect("git worktree add");
    assert!(sb.path().join(".worktrees/feat").exists(), "wt not created");
    assert!(!sb.container_exists(), "precondition: no container yet");

    // Simulate the picker's Choice::Existing path. We can't drive the
    // interactive picker, but we can call the same code path the picker
    // dispatches to: `claude-sandbox -w feat`. That uses targeted_start →
    // start_in_worktree → prepare_container.
    //
    // start_in_worktree's final exec runs `claude --dangerously-skip-permissions`
    // which would block on a TTY. Substitute `--shell` so we can drive it
    // non-interactively; the bug is in container creation, not the inner exec.
    //
    // Actually, the bug-triggering path is purely the container-prep part.
    // We assert via inspection: invoke `-w feat` with a short timeout and
    // verify the container ends up created. If the bug were present,
    // ensure_running_if_exists would error before any container existed.
    let _out = std::process::Command::new(Sandbox::bin())
        .args(["-w", "feat", "shell"])
        .current_dir(sb.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn claude-sandbox -w feat shell");
    // Give it a moment to create the container, then check.
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(
        sb.container_exists(),
        "container should have been created by `-w feat shell` even though \
         it didn't pre-exist (was: no container for this project)"
    );
}

fn init_git_with_commit(path: &std::path::Path) {
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(path)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test")
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let _ = std::fs::remove_dir_all(path.join(".git"));
    run(&["init", "-q", "-b", "main"]);
    std::fs::write(path.join("README.md"), "# test\n").expect("write readme");
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
}
