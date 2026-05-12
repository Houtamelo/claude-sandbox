//! `assets::user_override_state` reports whether the user's `~/.config`
//! override of an asset is absent, an identical copy of the binary's
//! embedded default, or a meaningful divergence. Drives:
//! - the rebuild-time warning ("you're overriding the Dockerfile, did
//!   you mean to?") so stale auto-deployed cruft is visible.
//! - the cfg wizard's refresh/delete prompt so users can clean up.

use std::sync::Mutex;

use claude_sandbox::assets::{
    self, user_override_state, OverrideState, DOCKERFILE_NAME, EMBEDDED_DOCKERFILE,
};

static SERIAL: Mutex<()> = Mutex::new(());

struct HomeGuard {
    prev: Option<std::ffi::OsString>,
    xdg_prev: Option<std::ffi::OsString>,
}

impl HomeGuard {
    fn pin(home: &std::path::Path) -> Self {
        let prev = std::env::var_os("HOME");
        let xdg_prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe {
            std::env::set_var("HOME", home);
            std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
        }
        HomeGuard { prev, xdg_prev }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match &self.xdg_prev {
                Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
    }
}

#[test]
fn absent_when_user_override_file_missing() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let _g = HomeGuard::pin(tmp.path());
    assert_eq!(
        user_override_state(DOCKERFILE_NAME, EMBEDDED_DOCKERFILE),
        OverrideState::Absent
    );
}

#[test]
fn matches_embedded_when_user_copy_is_byte_for_byte_identical() {
    // Old `make install` deployed an unchanged copy of the shipped
    // Dockerfile. Users never edited it. The cfg wizard offers to
    // delete the no-op override; this state is the green-light.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join(".config/claude-sandbox");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(cfg.join(DOCKERFILE_NAME), EMBEDDED_DOCKERFILE).unwrap();
    let _g = HomeGuard::pin(tmp.path());

    assert_eq!(
        user_override_state(DOCKERFILE_NAME, EMBEDDED_DOCKERFILE),
        OverrideState::MatchesEmbedded
    );
}

#[test]
fn differs_from_embedded_when_user_copy_was_modified() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join(".config/claude-sandbox");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(cfg.join(DOCKERFILE_NAME), "FROM ubuntu:24.04\n# my custom edit\n").unwrap();
    let _g = HomeGuard::pin(tmp.path());

    assert_eq!(
        user_override_state(DOCKERFILE_NAME, EMBEDDED_DOCKERFILE),
        OverrideState::DiffersFromEmbedded
    );
}

#[test]
fn differs_when_user_copy_is_from_an_older_shipped_version() {
    // This is the stale-cruft case: the old `make install` deployed a
    // pre-flexibility-pass Dockerfile (with tailscale). It's not what
    // the user would write themselves, but byte-for-byte differs from
    // the current embedded copy. Caller treats it the same as a manual
    // edit (warn, ask user); we don't try to distinguish "stale auto-
    // deployed" from "intentional user edit" because both have the
    // same remediation: refresh or delete.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join(".config/claude-sandbox");
    std::fs::create_dir_all(&cfg).unwrap();
    let stale = "FROM debian:bookworm-slim\n# tailscale install removed\n";
    std::fs::write(cfg.join(DOCKERFILE_NAME), stale).unwrap();
    let _g = HomeGuard::pin(tmp.path());

    assert_eq!(
        user_override_state(DOCKERFILE_NAME, EMBEDDED_DOCKERFILE),
        OverrideState::DiffersFromEmbedded
    );
}

#[test]
fn convenience_helpers_consult_the_right_files() {
    // `dockerfile_override_state` and `default_config_override_state`
    // wire the embedded constants — guards against a typo silently
    // making them resolve the wrong file.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join(".config/claude-sandbox");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(cfg.join(DOCKERFILE_NAME), "different\n").unwrap();
    let _g = HomeGuard::pin(tmp.path());

    assert_eq!(
        assets::dockerfile_override_state(),
        OverrideState::DiffersFromEmbedded
    );
    // config.toml was never written → absent for that helper.
    assert_eq!(
        assets::default_config_override_state(),
        OverrideState::Absent
    );
}
