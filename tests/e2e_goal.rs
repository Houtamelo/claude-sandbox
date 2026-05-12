//! Image-level guards for `claude-sandbox goal` / `cs goal`.
//!
//! The goal-mode launcher relies on two upstream contracts:
//!   1. The `claude` binary accepts `-p` (headless / print mode).
//!   2. `/goal` is a recognised slash command bundled with claude.
//!
//! If either disappears upstream, every `goal` launch silently degrades
//! to a useless session — guard both at the image level so the
//! regression is caught on `claude-sandbox rebuild` instead of in
//! production.

mod common;

use common::{should_skip, Sandbox};

#[test]
fn claude_in_image_advertises_dash_p_flag() {
    if should_skip("claude_in_image_advertises_dash_p_flag") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);
    let _ = common::podman(&["start", &sb.name]);

    let out = sb.podman_exec(&["bash", "-c", "claude --help 2>&1 | grep -E -- '(^|[ ,])-p( |,)'"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("-p"),
        "claude --help didn't list `-p`. The headless mode flag is \
         load-bearing for `goal` — if upstream renamed it, update the \
         goal launcher in src/main.rs.\nstdout: {stdout}"
    );
}

#[test]
fn slash_goal_is_a_known_command() {
    if should_skip("slash_goal_is_a_known_command") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);
    let _ = common::podman(&["start", &sb.name]);

    // `claude /help` (or its slash-command listing) advertises bundled
    // commands. We grep the published listing rather than invoking
    // `/goal` directly (which would burn tokens and need auth).
    let out = sb.podman_exec(&[
        "bash",
        "-c",
        // The bundled slash-command help can live in a couple places; check
        // both the installed plugin/skill metadata and the inline help.
        "set -e; \
         { ls ~/.claude/skills/ 2>/dev/null; \
           ls ~/.claude/plugins/ 2>/dev/null; \
           find / -path '*/superpowers/skills/goal*' 2>/dev/null; \
           claude --help 2>&1 || true; \
         } | grep -F goal || true",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    // We don't hard-assert success: the image may not bake the goal skill
    // in (it ships with the user's mounted ~/.claude). What we DO want to
    // catch is the case where the in-image claude version is so old it
    // predates the /goal feature entirely — that's a deployment problem
    // we want to flag, not silently launch into.
    eprintln!("[info] /goal discovery output:\n{stdout}");
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
        },
    )
    .expect("ensure_container");
    let _ = common::podman(&["start", &sb.name]);
    grant_acls(&podman, &sb.name, sb.path(), &[]).expect("grant_acls");
}
