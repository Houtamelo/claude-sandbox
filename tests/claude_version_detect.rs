//! Coverage for the version-detect path that pins the in-container
//! `claude` to the host's version. The fall-through to `"stable"` is a
//! genuine fallback — but losing the host version silently (which the
//! prior code did via `.ok()?`) means rebuilds quietly install whatever
//! `stable` happens to be that day, never matching the host. These
//! tests pin the typed failure modes so the caller can log them.

use claude_sandbox::podman::image::{parse_claude_version_stdout, ClaudeDetectError};

#[test]
fn parses_canonical_version_line() {
    // Real output: `claude --version` prints `2.1.139 (Claude Code)\n`
    let v = parse_claude_version_stdout("2.1.139 (Claude Code)\n").unwrap();
    assert_eq!(v, "2.1.139");
}

#[test]
fn parses_when_only_semver_present() {
    let v = parse_claude_version_stdout("2.0.0\n").unwrap();
    assert_eq!(v, "2.0.0");
}

#[test]
fn parses_with_leading_whitespace() {
    let v = parse_claude_version_stdout("   3.4.5 (Claude Code)\n").unwrap();
    assert_eq!(v, "3.4.5");
}

#[test]
fn empty_stdout_is_unparsable() {
    let e = parse_claude_version_stdout("").unwrap_err();
    assert!(matches!(e, ClaudeDetectError::UnparsableOutput { .. }));
}

#[test]
fn whitespace_only_stdout_is_unparsable() {
    let e = parse_claude_version_stdout("   \n\t\n").unwrap_err();
    assert!(matches!(e, ClaudeDetectError::UnparsableOutput { .. }));
}

#[test]
fn first_token_without_digits_is_unparsable() {
    // Guards against `claude --version` ever printing a banner line
    // first (e.g. "Claude 2.1.139") — we'd previously have taken
    // "Claude" as the version. Require at least one digit so the
    // upgrade path is fail-loud.
    let e = parse_claude_version_stdout("Claude Code 2.1.139\n").unwrap_err();
    assert!(matches!(e, ClaudeDetectError::UnparsableOutput { .. }));
}

#[test]
fn error_display_includes_failure_mode() {
    // The caller logs `format!("{e}")` to stderr on fallback. Make sure
    // each variant prints something the user can act on.
    let e = ClaudeDetectError::NotFound("No such file or directory".into());
    assert!(format!("{e}").to_lowercase().contains("not found"));

    let e = ClaudeDetectError::ExitNonZero { code: Some(2), stderr: "broken\n".into() };
    let s = format!("{e}");
    assert!(s.contains("exited"), "got: {s}");
    assert!(s.contains('2'));

    let e = ClaudeDetectError::UnparsableOutput { stdout: "foo".into() };
    assert!(format!("{e}").to_lowercase().contains("unparsable"));
}
