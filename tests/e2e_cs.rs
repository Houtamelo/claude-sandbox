//! End-to-end tests for the in-container `cs` companion: status,
//! worktree add/ls/rm/current. These require `cs` to actually run
//! inside the image (no GLIBC mismatch).

mod common;

use common::{should_skip, Sandbox};

#[test]
fn cs_status_works_inside_container() {
    if should_skip("cs_status_works_inside_container") {
        return;
    }
    let sb = Sandbox::new();
    create_and_start(&sb);

    let out = sb.podman_exec(&["cs", "status"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "`cs status` failed inside container.\nstdout: {stdout}\nstderr: {stderr}",
    );
    let expected = format!("project: {}", sb.path().display());
    assert!(
        stdout.contains(&expected),
        "expected '{expected}' in cs status; got: {stdout}"
    );
    assert!(
        stdout.contains("worktree: main"),
        "expected 'worktree: main' on main checkout; got: {stdout}"
    );
}

#[test]
fn cs_worktree_add_creates_directory() {
    if should_skip("cs_worktree_add_creates_directory") {
        return;
    }
    let sb = Sandbox::new();
    // git needs at least one commit before `worktree add` works.
    init_git_with_commit(sb.path());
    create_and_start(&sb);

    let out = sb.podman_exec(&["cs", "worktree", "add", "feat-a"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "`cs worktree add feat-a` failed.\nstdout: {stdout}\nstderr: {stderr}",
    );

    // The worktree dir should now exist on the HOST (we bind-mount $PWD).
    let wt = sb.path().join(".worktrees/feat-a");
    assert!(
        wt.exists() && wt.is_dir(),
        "worktree dir not visible on host at {}",
        wt.display()
    );

    // And `cs worktree ls` should list it.
    let out = sb.podman_exec(&["cs", "worktree", "ls"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("feat-a"),
        "feat-a missing from `cs worktree ls`: {stdout}"
    );
}

#[test]
fn cs_worktree_rm_removes_the_worktree() {
    if should_skip("cs_worktree_rm_removes_the_worktree") {
        return;
    }
    let sb = Sandbox::new();
    init_git_with_commit(sb.path());
    create_and_start(&sb);

    let out = sb.podman_exec(&["cs", "worktree", "add", "feat-b"]);
    assert!(out.status.success(), "add should succeed");

    let out = sb.podman_exec(&["cs", "worktree", "rm", "feat-b"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "rm should succeed; stderr: {stderr}"
    );

    let wt = sb.path().join(".worktrees/feat-b");
    assert!(
        !wt.exists(),
        "worktree dir should be gone after rm"
    );
}

// --- helpers ---

fn create_and_start(sb: &Sandbox) {
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
        },
    )
    .expect("ensure_container");

    let out = common::podman(&["start", &sb.name]);
    assert!(
        out.status.success(),
        "start failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Mirror start_or_shell: grant the non-root claude user access to /work.
    grant_acls(&podman, &sb.name, sb.path(), &[]).expect("grant_acls");
}

fn init_git_with_commit(path: &std::path::Path) {
    use std::process::Command;
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
    // Sandbox::new already created `.git` as a plain dir. Re-init properly.
    let _ = std::fs::remove_dir_all(path.join(".git"));
    run(&["init", "-q", "-b", "main"]);
    std::fs::write(path.join("README.md"), "# test\n").expect("write readme");
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
}
