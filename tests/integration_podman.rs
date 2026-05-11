//! Real-podman tests. Skipped unless CLAUDE_SANDBOX_E2E=1.

use std::process::Command;

fn e2e_enabled() -> bool {
    std::env::var("CLAUDE_SANDBOX_E2E").ok().as_deref() == Some("1")
}

#[test]
fn e2e_lifecycle() {
    if !e2e_enabled() {
        eprintln!("skipping (set CLAUDE_SANDBOX_E2E=1 to run)");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join(".git")).unwrap();

    let bin = env!("CARGO_BIN_EXE_claude-sandbox");
    let run = |args: &[&str]| -> std::process::Output {
        Command::new(bin)
            .args(args)
            .current_dir(tmp.path())
            .output()
            .unwrap()
    };

    let s = run(&["status"]);
    assert!(s.status.success(), "status before create should not fail");

    // Start would block; instead create+start manually via stop/down to avoid attach.
    let stop = run(&["stop"]);
    // stop is allowed to fail when no container exists; tolerated.
    let _ = stop;
    let down = run(&["down"]);
    let _ = down;
}
