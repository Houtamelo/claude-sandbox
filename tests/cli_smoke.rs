use assert_cmd::Command;
use predicates::str::contains;

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
        .stdout(contains("rename"));
}
