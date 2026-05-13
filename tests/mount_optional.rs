//! `MountSpec.optional` lets the shipped machine-wide config.toml ship
//! `[[mount]]` blocks for ssh-agent / gpg-agent / pulseaudio sockets
//! without breaking on hosts that don't run those services. Required
//! mounts still behave as before (parse-error / hard fail).

use std::sync::Mutex;

use claude_sandbox::config::parse::load_from_str;
use claude_sandbox::mounts::{spec_to_volume_optional, Volume};

static SERIAL: Mutex<()> = Mutex::new(());

struct EnvGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }
        EnvGuard { key, prev }
    }
    fn unset(key: &'static str) -> Self {
        let prev = std::env::var_os(key);
        unsafe {
            std::env::remove_var(key);
        }
        EnvGuard { key, prev: prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.prev {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

#[test]
fn optional_field_defaults_to_false() {
    let cfg = load_from_str(
        r#"[[mount]]
host = "/etc/hostname"
container = "/host-hostname"
"#,
        "<test>",
    )
    .unwrap();
    assert_eq!(cfg.mount.len(), 1);
    assert!(!cfg.mount[0].optional, "default must be false");
}

#[test]
fn optional_field_parses_when_set_true() {
    let cfg = load_from_str(
        r#"[[mount]]
host = "$SOME_VAR"
container = "$SOME_VAR"
optional = true
"#,
        "<test>",
    )
    .unwrap();
    assert!(cfg.mount[0].optional);
}

#[test]
fn required_mount_with_unresolved_env_var_fails_validation() {
    // Same behavior as today: if the user writes a `[[mount]]` without
    // optional = true, an unresolved $VAR leaves the path non-absolute
    // and validate errors. Catches typos.
    let _lock = SERIAL.lock().unwrap();
    let _g = EnvGuard::unset("CS_TEST_UNSET_VAR");
    let e = load_from_str(
        r#"[[mount]]
host = "/etc/hostname"
container = "$CS_TEST_UNSET_VAR/foo"
"#,
        "<test>",
    )
    .unwrap_err();
    let msg = format!("{e}");
    assert!(
        msg.contains("must be absolute"),
        "expected absolute-path validation error; got: {msg}"
    );
}

#[test]
fn optional_mount_with_unresolved_env_var_parses_cleanly() {
    // Critical: the shipped config.toml uses $SSH_AUTH_SOCK,
    // $XDG_RUNTIME_DIR/gnupg, etc. Hosts without those env vars must
    // not parse-error — they get an empty mount list instead.
    let _lock = SERIAL.lock().unwrap();
    let _g = EnvGuard::unset("CS_TEST_UNSET_VAR");
    let cfg = load_from_str(
        r#"[[mount]]
host = "$CS_TEST_UNSET_VAR"
container = "$CS_TEST_UNSET_VAR"
optional = true
"#,
        "<test>",
    )
    .expect("optional mount with unresolved env var must not parse-error");
    assert_eq!(cfg.mount.len(), 1);
}

#[test]
fn optional_mount_skipped_when_env_var_unset() {
    let _lock = SERIAL.lock().unwrap();
    let _g = EnvGuard::unset("CS_TEST_UNSET_VAR");
    let cfg = load_from_str(
        r#"[[mount]]
host = "$CS_TEST_UNSET_VAR/sub"
container = "$CS_TEST_UNSET_VAR/sub"
optional = true
"#,
        "<test>",
    )
    .unwrap();
    let project = std::path::PathBuf::from("/tmp/fake-project");
    assert!(
        spec_to_volume_optional(&cfg.mount[0], &project).is_none(),
        "unresolved $VAR in optional mount must produce None"
    );
}

#[test]
fn optional_mount_skipped_when_host_path_does_not_exist() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");
    let _g = EnvGuard::set("CS_TEST_MISSING", missing.to_str().unwrap());
    let cfg = load_from_str(
        r#"[[mount]]
host = "$CS_TEST_MISSING"
container = "$CS_TEST_MISSING"
optional = true
"#,
        "<test>",
    )
    .unwrap();
    let project = std::path::PathBuf::from("/tmp/fake-project");
    assert!(
        spec_to_volume_optional(&cfg.mount[0], &project).is_none(),
        "missing host path in optional mount must produce None"
    );
}

#[test]
fn optional_mount_included_when_host_path_exists() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("exists");
    std::fs::write(&real, b"x").unwrap();
    let _g = EnvGuard::set("CS_TEST_EXISTS", real.to_str().unwrap());
    let cfg = load_from_str(
        r#"[[mount]]
host = "$CS_TEST_EXISTS"
container = "/inside/path"
optional = true
"#,
        "<test>",
    )
    .unwrap();
    let project = std::path::PathBuf::from("/tmp/fake-project");
    let vol = spec_to_volume_optional(&cfg.mount[0], &project)
        .expect("present host path must produce Some");
    match vol {
        Volume::Bind(m) => {
            assert_eq!(m.host, real);
            assert_eq!(m.container, std::path::PathBuf::from("/inside/path"));
        }
        _ => panic!("expected Bind"),
    }
}

#[test]
fn required_mount_always_includes_even_when_host_missing() {
    // Required mounts pass through unconditionally; the failure surfaces
    // at podman-create time rather than here. Preserves existing semantics
    // for user-specified mounts where "the host path should exist" is a
    // load-bearing invariant.
    let cfg = load_from_str(
        r#"[[mount]]
host = "/path/that/does/not/exist/anywhere"
container = "/inside"
"#,
        "<test>",
    )
    .unwrap();
    let project = std::path::PathBuf::from("/tmp/fake-project");
    assert!(
        spec_to_volume_optional(&cfg.mount[0], &project).is_some(),
        "required mount must always produce Some, even when host is missing"
    );
}
