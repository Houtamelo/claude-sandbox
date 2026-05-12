//! Three-tier asset lookup: `~/.config/claude-sandbox/<name>` (user
//! override) -> `$CS_SYSTEM_DATA_DIR/<name>` (package install) ->
//! `include_str!` embedded fallback.
//!
//! - `$HOME` pins where the user-override tier resolves (via
//!   `paths::config_dir` which delegates to `dirs::home_dir`).
//! - `$CS_SYSTEM_DATA_DIR` overrides the system tier (default
//!   `/usr/share/claude-sandbox`) so tests can pin it to a tempdir.
//!
//! Tests mutate process-global env, so they hold a file-wide mutex.

use std::sync::Mutex;

use claude_sandbox::assets::{
    self, populate_user_config, resolve_default_config, resolve_dockerfile, AssetSource,
    EMBEDDED_DEFAULT_CONFIG, EMBEDDED_DOCKERFILE,
};

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
            // Some dirs versions consult XDG_CONFIG_HOME before HOME; force it
            // to track our HOME pin so paths::config_dir is deterministic.
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
fn user_override_wins_over_system_and_embedded() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys");
    std::fs::create_dir_all(home.join(".config/claude-sandbox")).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    std::fs::write(home.join(".config/claude-sandbox/Dockerfile"), "USER_DOCKERFILE").unwrap();
    std::fs::write(sys.join("Dockerfile"), "SYS_DOCKERFILE").unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let r = resolve_dockerfile().unwrap();
    assert_eq!(r.contents, "USER_DOCKERFILE");
    assert!(matches!(r.source, AssetSource::UserOverride(_)));
}

#[test]
fn system_used_when_user_override_absent() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys");
    std::fs::create_dir_all(home.join(".config/claude-sandbox")).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    std::fs::write(sys.join("Dockerfile"), "SYS_DOCKERFILE").unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let r = resolve_dockerfile().unwrap();
    assert_eq!(r.contents, "SYS_DOCKERFILE");
    assert!(matches!(r.source, AssetSource::System(_)));
}

#[test]
fn embedded_used_when_both_absent() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys-does-not-exist");
    std::fs::create_dir_all(&home).unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let r = resolve_dockerfile().unwrap();
    assert_eq!(r.contents, EMBEDDED_DOCKERFILE);
    assert!(matches!(r.source, AssetSource::Embedded));
}

#[test]
fn default_config_uses_same_three_tier_order() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys");
    std::fs::create_dir_all(home.join(".config/claude-sandbox")).unwrap();
    std::fs::create_dir_all(&sys).unwrap();
    std::fs::write(sys.join("config.toml"), "name = \"sys-cfg\"").unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let r = resolve_default_config().unwrap();
    assert_eq!(r.contents, "name = \"sys-cfg\"");
    assert!(matches!(r.source, AssetSource::System(_)));
}

#[test]
fn embedded_constants_are_nonempty() {
    // Compile-time include_str! would silently embed an empty string if
    // the path is wrong; this guards the wiring.
    assert!(!EMBEDDED_DOCKERFILE.trim().is_empty(), "Dockerfile must be embedded");
    assert!(
        !EMBEDDED_DEFAULT_CONFIG.trim().is_empty(),
        "default-config.toml must be embedded"
    );
}

#[test]
fn populate_user_config_writes_embedded_into_home_config() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys-irrelevant");
    let _g = EnvGuard::pin(&home, &sys);

    let written = populate_user_config(false).unwrap();
    assert_eq!(written.len(), 2, "both Dockerfile and config.toml should be written");

    let dockerfile = std::fs::read_to_string(home.join(".config/claude-sandbox/Dockerfile")).unwrap();
    assert_eq!(dockerfile, EMBEDDED_DOCKERFILE);

    let config = std::fs::read_to_string(home.join(".config/claude-sandbox/config.toml")).unwrap();
    assert_eq!(config, EMBEDDED_DEFAULT_CONFIG);
}

#[test]
fn populate_user_config_does_not_clobber_existing_files() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys-irrelevant");
    let cfg_dir = home.join(".config/claude-sandbox");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(cfg_dir.join("Dockerfile"), "USER_EDIT").unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let written = populate_user_config(false).unwrap();

    let dockerfile = std::fs::read_to_string(cfg_dir.join("Dockerfile")).unwrap();
    assert_eq!(dockerfile, "USER_EDIT", "must not clobber user-edited Dockerfile");
    // Only config.toml gets written; Dockerfile already existed.
    assert_eq!(written.len(), 1);
    assert!(written[0].ends_with("config.toml"));
}

#[test]
fn populate_user_config_force_overwrites_existing() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys = tmp.path().join("sys-irrelevant");
    let cfg_dir = home.join(".config/claude-sandbox");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(cfg_dir.join("Dockerfile"), "USER_EDIT").unwrap();
    let _g = EnvGuard::pin(&home, &sys);

    let written = populate_user_config(true).unwrap();

    let dockerfile = std::fs::read_to_string(cfg_dir.join("Dockerfile")).unwrap();
    assert_eq!(dockerfile, EMBEDDED_DOCKERFILE, "force=true must replace existing file");
    assert_eq!(written.len(), 2);
}

#[test]
fn system_data_dir_defaults_to_usr_share() {
    let _lock = SERIAL.lock().unwrap();
    let prev = std::env::var_os("CS_SYSTEM_DATA_DIR");
    unsafe {
        std::env::remove_var("CS_SYSTEM_DATA_DIR");
    }
    assert_eq!(
        assets::system_data_dir(),
        std::path::PathBuf::from("/usr/share/claude-sandbox")
    );
    unsafe {
        match prev {
            Some(v) => std::env::set_var("CS_SYSTEM_DATA_DIR", v),
            None => std::env::remove_var("CS_SYSTEM_DATA_DIR"),
        }
    }
}
