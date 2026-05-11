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
         The Anthropic installer placed it at /root/.local/bin/ but PATH \
         doesn't include that.\nstdout: {}",
        stdout
    );
}

#[test]
fn default_entrypoint_accepts_appended_commands() {
    // Catches the `ENTRYPOINT ["/bin/bash", "-l"]` + appended-command bug:
    // `podman run image cmd args` becomes `bash -l cmd args` and bash
    // treats `cmd` as a script filename — "cannot execute binary file".
    //
    // We expect a working image to let us `podman run image whoami`
    // and get `root` back.
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
    assert_eq!(stdout.trim(), "root");
}

#[test]
fn home_local_bin_in_path() {
    // Either claude lives somewhere global, or /root/.local/bin is in PATH.
    // We verify by echoing PATH and asserting /root/.local/bin appears
    // (which is the canonical fix for `claude_is_on_path_inside_image`).
    if should_skip("home_local_bin_in_path") {
        return;
    }
    let out = run_in_image(&["echo \"$PATH\""]);
    let path = String::from_utf8_lossy(&out.stdout);
    assert!(
        path.contains("/root/.local/bin"),
        "/root/.local/bin not in image PATH (would break `claude` lookup). \
         got PATH: {}",
        path.trim()
    );
}
