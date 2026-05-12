//! Verifies that the worktree-launch path (`bash -c "cd <wt> && exec <inner>"`)
//! does NOT use a login shell that resets PATH. A login shell on Debian
//! sources /etc/profile which overrides PATH and drops `/root/.local/bin`
//! — that hides the `claude` binary installed there.
//!
//! Catches: bug where `claude-sandbox -w <name>` says "claude: command not found".

mod common;

use std::process::Command;

use common::{should_skip, Sandbox};

#[test]
fn non_login_shell_preserves_claude_local_bin_in_path() {
    if should_skip("non_login_shell_preserves_claude_local_bin_in_path") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);
    let _ = common::podman(&["start", &sb.name]);

    // Mirror what `start_in_worktree` produces: bash -c (no -l).
    let out = sb.podman_exec(&["bash", "-c", "echo $PATH"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let expected = format!(
        "{}/.local/bin",
        claude_sandbox::mounts::container_home().display()
    );
    assert!(
        stdout.contains(&expected),
        "non-login bash dropped {expected} from PATH. \
         If this fails the worktree-launch path will say \
         `claude: command not found`. got PATH: {}",
        stdout.trim()
    );
}

#[test]
fn claude_is_findable_via_the_actual_worktree_wrapper_shape() {
    // Reproduces start_in_worktree's exec argv shape.
    if should_skip("claude_is_findable_via_the_actual_worktree_wrapper_shape") {
        return;
    }
    let sb = Sandbox::new();
    init_git_with_commit(sb.path());
    create_via_lib(&sb);
    let _ = common::podman(&["start", &sb.name]);

    // Create a worktree via cs first.
    let out = sb.podman_exec(&["cs", "worktree", "add", "feat-x"]);
    assert!(
        out.status.success(),
        "cs worktree add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Now run the exact bash wrapper start_in_worktree produces, but with
    // `command -v claude` instead of `claude` (since we can't drive an
    // interactive claude session). The wrapper must NOT use `bash -lc`.
    let wt = sb.path().join(".worktrees/feat-x").display().to_string();
    let wrapper = format!(
        "trap 'rm -f {wt}/.cs-session' EXIT INT TERM; cd {wt} && command -v claude"
    );
    let out = sb.podman_exec(&["bash", "-c", &wrapper]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "wrapper failed.\nstdout: {stdout}\nstderr: {stderr}",
    );
    assert!(
        stdout.trim().ends_with("/claude"),
        "wrapper didn't find claude. stdout: {}",
        stdout.trim()
    );
}

// --- helpers ---

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
    // grant_acls needs a running container.
    let _ = common::podman(&["start", &sb.name]);
    grant_acls(&podman, &sb.name, sb.path(), &[]).expect("grant_acls");
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
