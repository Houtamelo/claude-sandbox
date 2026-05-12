use assert_cmd::Command;
use clap::Parser;
use predicates::str::contains;
use tempfile;

use claude_sandbox::cli::{Cmd, CsCli, CsCmd, HostCli};

#[test]
fn help_works() {
    Command::cargo_bin("claude-sandbox").unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("start"))
        .stdout(contains("shell"))
        .stdout(contains("stop"))
        .stdout(contains("down"))
        .stdout(contains("rename"))
        .stdout(contains("goal"));
}

/// `claude-sandbox goal "<sentence>"` parses as a single Goal command
/// whose condition is the joined trailing args. Guards against accidental
/// regressions to the `trailing_var_arg`/`required=true` settings.
#[test]
fn host_goal_parses_multiword_condition() {
    let cli = HostCli::try_parse_from([
        "claude-sandbox",
        "goal",
        "spec.md",
        "is",
        "implemented",
        "and",
        "all",
        "tests",
        "pass",
    ])
    .expect("parse");
    match cli.command {
        Some(Cmd::Goal { condition }) => {
            assert_eq!(
                condition.join(" "),
                "spec.md is implemented and all tests pass"
            );
        }
        other => panic!("expected Goal, got {other:?}"),
    }
}

/// A bare `claude-sandbox goal` with no condition must error — the goal
/// has to have a target. (Otherwise clap would happily hand us an empty
/// Vec and we'd run `claude -p /goal `, which silently degrades to a
/// no-op session.)
#[test]
fn host_goal_requires_condition() {
    let err = HostCli::try_parse_from(["claude-sandbox", "goal"])
        .err()
        .expect("should fail");
    let s = err.to_string();
    assert!(
        s.contains("required") || s.contains("CONDITION") || s.contains("condition"),
        "unexpected error: {s}"
    );
}

/// When `machine.toml` doesn't exist, every command except `cfg`/`init`/
/// `--help`/`--version` must fail fast with a descriptive error that
/// names the missing file and points at `claude-sandbox cfg`.
#[test]
fn missing_machine_toml_errors_with_cfg_hint() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = Command::cargo_bin("claude-sandbox")
        .unwrap()
        .arg("status")
        .env("HOME", tmp.path())
        // Force a clean XDG_CONFIG_HOME path under the tempdir so a stray
        // ~/.config/claude-sandbox/machine.toml on the test host doesn't
        // satisfy the gate.
        .env("XDG_CONFIG_HOME", tmp.path().join(".config"))
        // current_dir doesn't matter for this — the gate fires before
        // project lookup.
        .current_dir(tmp.path())
        .output()
        .expect("spawn");
    assert!(
        !out.status.success(),
        "expected non-zero exit when machine.toml is missing, got {}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("claude-sandbox cfg"),
        "stderr should hint at the cfg subcommand; got: {stderr}"
    );
}

#[test]
fn cs_goal_parses_multiword_condition() {
    let cli = CsCli::try_parse_from([
        "cs",
        "goal",
        "all",
        "tests",
        "green",
    ])
    .expect("parse");
    match cli.command {
        CsCmd::Goal { condition } => {
            assert_eq!(condition.join(" "), "all tests green");
        }
        other => panic!("expected Goal, got {other:?}"),
    }
}
