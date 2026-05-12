//! `ensure_gpg_agent` is the GPG counterpart to `ensure_ssh_agent`,
//! but unlike SSH it bind-mounts the entire `~/.gnupg/` directory.
//! These tests exercise both presence-and-absence paths via a HOME
//! override; they must run serially (the env is process-global).

use claude_sandbox::env::ensure_gpg_agent;
use claude_sandbox::mounts::Volume;

fn with_home<F: FnOnce()>(home: &std::path::Path, f: F) {
    let prev = std::env::var_os("HOME");
    // SAFETY: tests run with --test-threads=1 (see suite default).
    unsafe { std::env::set_var("HOME", home); }
    f();
    unsafe {
        match prev {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }
}

#[test]
fn binds_gnupg_when_host_dir_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let gnupg = tmp.path().join(".gnupg");
    std::fs::create_dir(&gnupg).unwrap();

    with_home(tmp.path(), || {
        let mut volumes: Vec<Volume> = Vec::new();
        ensure_gpg_agent(&mut volumes);
        assert_eq!(volumes.len(), 1, "expected one bind mount");
        match &volumes[0] {
            Volume::Bind(m) => {
                assert_eq!(m.host, gnupg);
                assert_eq!(m.container, gnupg);
                assert!(!m.ro, "bind must be rw — gpg writes state");
            }
            Volume::Named { .. } => panic!("expected Bind mount, got Named"),
        }
    });
}

#[test]
fn noop_when_host_dir_missing() {
    let tmp = tempfile::tempdir().unwrap();
    // Intentionally do NOT create $HOME/.gnupg
    with_home(tmp.path(), || {
        let mut volumes: Vec<Volume> = Vec::new();
        ensure_gpg_agent(&mut volumes);
        assert!(
            volumes.is_empty(),
            "no bind mount should be added when ~/.gnupg/ is absent; \
             got {volumes:?}"
        );
    });
}

#[test]
fn noop_when_gnupg_is_a_file_not_a_dir() {
    let tmp = tempfile::tempdir().unwrap();
    // Defend against the odd edge case where ~/.gnupg is a file. Should
    // not bind (would mount a file at a directory path, confusing gpg).
    std::fs::write(tmp.path().join(".gnupg"), "").unwrap();
    with_home(tmp.path(), || {
        let mut volumes: Vec<Volume> = Vec::new();
        ensure_gpg_agent(&mut volumes);
        assert!(volumes.is_empty(), "got {volumes:?}");
    });
}
