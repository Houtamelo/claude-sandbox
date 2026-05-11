//! Image-level sanity tests: does the base image contain the expected
//! binaries / PATH wiring / entrypoint behavior?
//!
//! Run with: `CLAUDE_SANDBOX_E2E=1 cargo test --test e2e_image`

mod common;

use common::{run_in_image, should_skip};

#[test]
fn image_has_cs_and_claude_sandbox_binaries() {
    if should_skip("image_has_cs_and_claude_sandbox_binaries") {
        return;
    }
    let out = run_in_image(&[
        "test -x /usr/local/bin/claude-sandbox && \
         test -L /usr/local/bin/cs && \
         readlink /usr/local/bin/cs",
    ]);
    assert!(
        out.status.success(),
        "binaries missing or wrong perms.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "claude-sandbox"
    );
}

#[test]
fn cs_binary_can_actually_run_inside_image() {
    // This is the GLIBC-mismatch canary. If `cs` was cross-compiled against
    // a newer glibc than the image carries, this will fail with a clear
    // "GLIBC_X.Y not found" message.
    if should_skip("cs_binary_can_actually_run_inside_image") {
        return;
    }
    let out = run_in_image(&["cs --help 2>&1; echo EXIT=$?"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("GLIBC_") && !stdout.contains("cannot execute"),
        "cs is incompatible with the image's libc.\nfull output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("EXIT=0"),
        "cs --help failed inside the image.\nfull output:\n{}",
        stdout
    );
}

#[test]
fn claude_is_on_path_inside_image() {
    if should_skip("claude_is_on_path_inside_image") {
        return;
    }
    let out = run_in_image(&["command -v claude || echo MISSING"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("MISSING"),
        "claude binary is not on PATH inside the image. \
         The Anthropic installer placed it at /home/claude/.local/bin/ \
         but PATH doesn't include that.\nstdout: {}",
        stdout
    );
}

#[test]
fn default_entrypoint_accepts_appended_commands() {
    // Catches the `ENTRYPOINT ["/bin/bash", "-l"]` + appended-command bug:
    // `podman run image cmd args` becomes `bash -l cmd args` and bash
    // treats `cmd` as a script filename — "cannot execute binary file".
    //
    // Image default user is `claude` (non-root) — required for
    // --dangerously-skip-permissions to actually work.
    if should_skip("default_entrypoint_accepts_appended_commands") {
        return;
    }
    let out = common::podman(&["run", "--rm", common::IMAGE, "whoami"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "appended `whoami` command failed via default entrypoint.\nstdout: {stdout}\nstderr: {stderr}",
    );
    assert_eq!(stdout.trim(), "claude");
}

#[test]
fn home_local_bin_in_path() {
    // Either claude lives somewhere global, or /home/claude/.local/bin is in PATH.
    // We verify by echoing PATH and asserting it appears (which is the
    // canonical fix for `claude_is_on_path_inside_image`).
    if should_skip("home_local_bin_in_path") {
        return;
    }
    let out = run_in_image(&["echo \"$PATH\""]);
    let path = String::from_utf8_lossy(&out.stdout);
    assert!(
        path.contains("/home/claude/.local/bin"),
        "/home/claude/.local/bin not in image PATH (would break `claude` lookup). \
         got PATH: {}",
        path.trim()
    );
}

#[test]
fn claude_user_has_passwordless_sudo() {
    // The image's `claude` user must have NOPASSWD sudo so apt installs,
    // tailscaled setup, and other root-needing operations stay frictionless
    // inside the sandbox.
    if should_skip("claude_user_has_passwordless_sudo") {
        return;
    }
    let out = run_in_image(&["sudo -n whoami 2>&1"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.trim() == "root",
        "claude user lacks passwordless sudo (sandbox can't apt-install). \
         output: {stdout}"
    );
}
