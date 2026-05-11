use tempfile::tempdir;

use claude_sandbox::worktree::claim::{clear, evaluate, read, write, ClaimState};

#[test]
fn write_then_read_roundtrips() {
    let tmp = tempdir().unwrap();
    let c = write(tmp.path()).unwrap();
    let read_back = read(tmp.path()).unwrap().unwrap();
    assert_eq!(read_back.host_pid, c.host_pid);
}

#[test]
fn clear_removes_file() {
    let tmp = tempdir().unwrap();
    write(tmp.path()).unwrap();
    clear(tmp.path()).unwrap();
    assert!(read(tmp.path()).unwrap().is_none());
}

#[test]
fn active_when_pid_is_self() {
    let tmp = tempdir().unwrap();
    write(tmp.path()).unwrap();
    match evaluate(tmp.path()).unwrap() {
        ClaimState::Active(_) => {}
        _ => panic!("expected Active"),
    }
}

#[test]
fn stale_when_pid_does_not_exist() {
    use std::fs;
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join(".cs-session"),
        r#"{"host_pid":2147483640,"started_at":0,"container_exec_id":null}"#,
    ).unwrap();
    match evaluate(tmp.path()).unwrap() {
        ClaimState::Stale(_) => {}
        _ => panic!("expected Stale"),
    }
}

#[test]
fn available_when_no_file() {
    let tmp = tempdir().unwrap();
    match evaluate(tmp.path()).unwrap() {
        ClaimState::Available => {}
        _ => panic!("expected Available"),
    }
}
