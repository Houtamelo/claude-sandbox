//! Verifies that `grant_acls` puts a symlink at `$HOME/CLAUDE.md` -> `/CLAUDE.md`.
//!
//! Why this matters: Claude Code's parent-directory CLAUDE.md walk stops at
//! `$HOME`, not `/`. The baked sandbox-awareness doc at `/CLAUDE.md` would
//! otherwise never be loaded by the in-container agent. The symlink puts it
//! on the walk path for any project under `$HOME`.
//!
//! Pure structural check (does the symlink exist + point to `/CLAUDE.md`).
//! We don't drive `claude -p` here because that needs auth tokens, costs
//! API budget, and is flaky; the merge-behavior verification belongs in
//! manual probes (see commit log for the test harness used during dev).

mod common;

use common::{should_skip, Sandbox};

#[test]
fn grant_acls_links_root_claude_md_into_home() {
    if should_skip("grant_acls_links_root_claude_md_into_home") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);

    let home = claude_sandbox::mounts::container_home();
    let link = format!("{}/CLAUDE.md", home.display());

    // The symlink must exist...
    let out = sb.podman_exec(&["test", "-L", &link]);
    assert!(
        out.status.success(),
        "expected symlink at {link}, got status {:?}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    // ...and point at the baked sandbox doc.
    let out = sb.podman_exec(&["readlink", &link]);
    let target = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(
        target, "/CLAUDE.md",
        "symlink at {link} points to '{target}', expected '/CLAUDE.md'"
    );

    // Sanity-check that the symlink dereferences to readable content.
    let out = sb.podman_exec(&["bash", "-c", &format!("cat {link} | head -1")]);
    let first = String::from_utf8_lossy(&out.stdout);
    assert!(
        !first.trim().is_empty(),
        "symlink target unreadable or empty. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
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
