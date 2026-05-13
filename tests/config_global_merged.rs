//! `config::load_global_merged` is the glue that lets per-project
//! `.claude-sandbox.toml` merge on top of a global config resolved
//! through the three-tier asset lookup. It's the path every command
//! takes when reading project configuration, so the tier ordering and
//! merge semantics deserve their own coverage.
//!
//! Tests pin `$HOME` and `$CS_SYSTEM_DATA_DIR` so we don't depend on
//! whatever is installed on the host; they hold a file-wide mutex
//! because env mutation is process-global.

use std::sync::Mutex;

use claude_sandbox::config::load_global_merged;

static SERIAL: Mutex<()> = Mutex::new(());

struct EnvGuard {
    home_prev: Option<std::ffi::OsString>,
    sys_prev: Option<std::ffi::OsString>,
    xdg_prev: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn pin(home: &std::path::Path, sys: &std::path::Path) -> Self {
        let home_prev = std::env::var_os("HOME");
        let sys_prev = std::env::var_os("CS_SYSTEM_DATA_DIR");
        let xdg_prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe {
            std::env::set_var("HOME", home);
            std::env::set_var("CS_SYSTEM_DATA_DIR", sys);
            std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
        }
        EnvGuard { home_prev, sys_prev, xdg_prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.home_prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match &self.sys_prev {
                Some(v) => std::env::set_var("CS_SYSTEM_DATA_DIR", v),
                None => std::env::remove_var("CS_SYSTEM_DATA_DIR"),
            }
            match &self.xdg_prev {
                Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }
    }
}

#[test]
fn embedded_default_provides_bridge_network_and_env_passthrough() {
    // Neither override exists -> the embedded default is the source of
    // truth. Asserting on the actual values from assets/default-config.toml
    // makes this test the regression net for "someone changed the
    // embedded default silently". Currently: network=bridge, plus
    // env_passthrough = [SSH_AUTH_SOCK, XDG_RUNTIME_DIR] used by the
    // shipped agent-forwarding recipes.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys-empty");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let cfg = load_global_merged(None).unwrap();
    assert_eq!(
        cfg.network.as_deref(),
        Some("bridge"),
        "embedded default sets network = bridge"
    );
    assert!(
        cfg.env_passthrough.iter().any(|s| s == "SSH_AUTH_SOCK"),
        "embedded default must pass SSH_AUTH_SOCK through for the ssh-agent recipe; got: {:?}",
        cfg.env_passthrough
    );
    assert!(
        cfg.env_passthrough.iter().any(|s| s == "XDG_RUNTIME_DIR"),
        "embedded default must pass XDG_RUNTIME_DIR for the gpg-agent / pulse recipes; got: {:?}",
        cfg.env_passthrough
    );
    // Sanity: agent-forwarding recipes are present as [[mount]] entries
    // (they're optional so they don't break headless hosts).
    let mount_hosts: Vec<&str> = cfg.mount.iter().map(|m| m.host.as_str()).collect();
    assert!(
        mount_hosts.contains(&"$SSH_AUTH_SOCK"),
        "embedded must include $SSH_AUTH_SOCK [[mount]] recipe; got: {mount_hosts:?}"
    );
    assert!(
        mount_hosts.contains(&"~/.gnupg"),
        "embedded must include ~/.gnupg [[mount]] recipe; got: {mount_hosts:?}"
    );
}

#[test]
fn user_override_wins_over_embedded() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys-empty");
    let cfg_dir = home.join(".config/claude-sandbox");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    std::fs::write(
        cfg_dir.join("config.toml"),
        "network = \"host\"\n",
    )
    .unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let cfg = load_global_merged(None).unwrap();
    assert_eq!(
        cfg.network.as_deref(),
        Some("host"),
        "user override must replace embedded network value"
    );
}

#[test]
fn system_install_used_when_user_override_absent() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    std::fs::write(sys.join("config.toml"), "network = \"none\"\n").unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let cfg = load_global_merged(None).unwrap();
    assert_eq!(cfg.network.as_deref(), Some("none"));
}

#[test]
fn local_project_overrides_global() {
    // The merge semantics: global is the base, local applies on top
    // (scalars overwrite, lists concat). Project's own .claude-sandbox.toml
    // must win for fields it sets — that's the entire reason per-project
    // configs exist.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys-empty");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let project_dir = tmp.path().join("proj");
    std::fs::create_dir_all(&project_dir).unwrap();
    let local = project_dir.join(".claude-sandbox.toml");
    std::fs::write(
        &local,
        "name = \"myproj\"\nnetwork = \"host\"\n",
    )
    .unwrap();

    let cfg = load_global_merged(Some(&local)).unwrap();
    // Local fields win:
    assert_eq!(cfg.name.as_deref(), Some("myproj"));
    assert_eq!(cfg.network.as_deref(), Some("host"));
}

#[test]
fn missing_local_path_is_silently_ignored() {
    // Per the project gate flow: `load_cfg` only passes `Some(&path)`
    // when the project toml exists. If a caller does pass a stale path,
    // we don't want to error — there's nothing local to merge, just use
    // the global.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys-empty");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let nonexistent = tmp.path().join("does-not-exist.toml");
    let cfg = load_global_merged(Some(&nonexistent)).unwrap();
    // Falls back to embedded (which sets network = bridge).
    assert_eq!(cfg.network.as_deref(), Some("bridge"));
}

#[test]
fn malformed_system_global_surfaces_source_label_in_error() {
    // If the packaged /usr/share/claude-sandbox/config.toml has been
    // corrupted (bad permissions, mid-upgrade truncation, manual edit
    // gone wrong), the error path must tell the user *where* the bad
    // file lives. Otherwise debugging means guessing which tier produced
    // the global.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    std::fs::write(sys.join("config.toml"), "network = \"completely-invalid\"\n").unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let e = load_global_merged(None).unwrap_err();
    let msg = format!("{e}");
    let sys_path = sys.join("config.toml").display().to_string();
    assert!(
        msg.contains(&sys_path),
        "error must name the failing tier's path; got: {msg}"
    );
}
