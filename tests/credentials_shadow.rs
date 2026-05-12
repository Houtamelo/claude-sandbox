//! When `CLAUDE_CODE_OAUTH_TOKEN` is configured (via `claude-sandbox cfg`'s
//! OAuth-token step), the container should NOT share `.credentials.json`
//! with the host — otherwise it participates in the OAuth refresh-token
//! race documented at https://github.com/anthropics/claude-code/issues/27933,
//! which causes both host and sandboxed claude sessions to be logged out
//! when their tokens diverge.
//!
//! Fix shape: bind-mount the host's `~/.claude/` directory as before, then
//! add a deeper-path mount that overlays an empty file at
//! `~/.claude/.credentials.json` inside the container. Podman applies
//! deeper mounts on top of shallower ones, so the container sees an
//! empty credentials file and falls through to the
//! `CLAUDE_CODE_OAUTH_TOKEN` env var for inference auth.
//!
//! When no OAuth token is configured, the shadow mount must be absent —
//! otherwise users who haven't run `claude-sandbox cfg`'s token step
//! would lose all auth inside the container.

use std::sync::Mutex;

use claude_sandbox::mounts::{default_volumes, empty_credentials_path, Volume};

static SERIAL: Mutex<()> = Mutex::new(());

struct EnvGuard {
    home_prev: Option<std::ffi::OsString>,
    xdg_prev: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn pin(home: &std::path::Path) -> Self {
        let home_prev = std::env::var_os("HOME");
        let xdg_prev = std::env::var_os("XDG_CONFIG_HOME");
        unsafe {
            std::env::set_var("HOME", home);
            // machine::oauth_token_path() = paths::config_dir() / "oauth_token",
            // and paths::config_dir() reads HOME. Pinning XDG_CONFIG_HOME too
            // is defensive against any libc/dirs version that consults it.
            std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
        }
        EnvGuard { home_prev, xdg_prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.home_prev {
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

fn write_oauth_token(home: &std::path::Path) {
    let p = home.join(".config/claude-sandbox/oauth_token");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "sk-ant-oat01-fake-token-for-tests").unwrap();
}

fn find_shadow_mount<'a>(
    volumes: &'a [Volume],
    home: &std::path::Path,
) -> Option<&'a claude_sandbox::mounts::Mount> {
    let needle = home.join(".claude/.credentials.json");
    volumes.iter().find_map(|v| match v {
        Volume::Bind(m) if m.container == needle => Some(m),
        _ => None,
    })
}

fn find_claude_dir_mount<'a>(
    volumes: &'a [Volume],
    home: &std::path::Path,
) -> Option<&'a claude_sandbox::mounts::Mount> {
    let needle = home.join(".claude");
    volumes.iter().find_map(|v| match v {
        Volume::Bind(m) if m.container == needle => Some(m),
        _ => None,
    })
}

#[test]
fn shadow_mount_present_when_oauth_token_configured() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home/test-user");
    std::fs::create_dir_all(&home).unwrap();
    write_oauth_token(&home);
    let _g = EnvGuard::pin(&home);

    let project = home.join("proj");
    std::fs::create_dir_all(&project).unwrap();
    let vols = default_volumes(&project, "test-container");

    let shadow = find_shadow_mount(&vols, &home).expect(
        "expected a shadow mount at container ~/.claude/.credentials.json when oauth_token is configured",
    );
    assert!(
        shadow.ro,
        "shadow mount must be read-only — the container has no business writing to credentials"
    );
    assert_eq!(
        shadow.host,
        empty_credentials_path(),
        "shadow host must point at the cache-dir empty credentials file"
    );
}

#[test]
fn shadow_mount_omitted_when_oauth_token_absent() {
    // Users who haven't run `claude-sandbox cfg`'s OAuth-token step still
    // rely on the host's .credentials.json for in-container auth. Adding
    // a shadow would lock them out entirely.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home/test-user");
    std::fs::create_dir_all(&home).unwrap();
    // Deliberately do NOT write_oauth_token here.
    let _g = EnvGuard::pin(&home);

    let project = home.join("proj");
    std::fs::create_dir_all(&project).unwrap();
    let vols = default_volumes(&project, "test-container");

    assert!(
        find_shadow_mount(&vols, &home).is_none(),
        "no shadow mount must be added when oauth_token is absent"
    );
}

#[test]
fn shadow_mount_comes_after_claude_dir_mount_in_volume_order() {
    // Podman applies mounts in order; deeper mounts override shallower
    // ones, but only when the deeper mount is declared AFTER the
    // shallower one in the args. If the shadow is emitted before the
    // ~/.claude/ bind-mount, the parent mount will obliterate it.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home/test-user");
    std::fs::create_dir_all(&home).unwrap();
    write_oauth_token(&home);
    let _g = EnvGuard::pin(&home);

    let project = home.join("proj");
    std::fs::create_dir_all(&project).unwrap();
    let vols = default_volumes(&project, "test-container");

    // Locate the .claude/ dir mount and the shadow mount by their positions.
    let claude_dir_idx = vols
        .iter()
        .position(|v| matches!(v, Volume::Bind(m) if m.container == home.join(".claude")))
        .expect("expected the ~/.claude/ dir mount");
    let shadow_idx = vols
        .iter()
        .position(|v| matches!(v, Volume::Bind(m) if m.container == home.join(".claude/.credentials.json")))
        .expect("expected the shadow mount");
    assert!(
        shadow_idx > claude_dir_idx,
        "shadow (idx {shadow_idx}) must appear AFTER ~/.claude dir mount (idx {claude_dir_idx})"
    );
}

#[test]
fn empty_credentials_file_is_created_on_demand_and_contains_safe_payload() {
    // Idempotent ensure. Side-effect of resolving the path is that the
    // file exists on disk so the bind-mount has something to point at.
    // Payload is "{}" rather than zero-bytes so claude-code's JSON
    // parser doesn't error on read.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home/test-user");
    std::fs::create_dir_all(&home).unwrap();
    let _g = EnvGuard::pin(&home);

    let p = empty_credentials_path();
    assert!(p.is_file(), "empty_credentials_path must materialize the file");
    let contents = std::fs::read_to_string(&p).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&contents)
        .expect("shadow file must be valid JSON so claude-code doesn't error on read");
    assert!(
        parsed.is_object(),
        "shadow file must be a JSON object (claude-code expects an object at top level)"
    );
    // Idempotent: second call doesn't overwrite or fail.
    let p2 = empty_credentials_path();
    assert_eq!(p, p2);
    assert!(p2.is_file());
}

#[test]
fn shadow_mount_does_not_contain_refresh_token_payload() {
    // Sanity: the file we point at must not carry the host's actual
    // credentials. Caller's intent is "give the container nothing to
    // refresh"; verifying via the actual disk content removes the risk
    // of a future refactor accidentally pointing the shadow at the
    // host's credentials.json.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home/test-user");
    std::fs::create_dir_all(&home).unwrap();
    let _g = EnvGuard::pin(&home);

    let p = empty_credentials_path();
    let contents = std::fs::read_to_string(&p).unwrap();
    assert!(
        !contents.contains("refreshToken"),
        "shadow file must not contain a refreshToken field"
    );
    assert!(
        !contents.contains("accessToken"),
        "shadow file must not contain an accessToken field"
    );
}

#[test]
fn presence_of_oauth_token_does_not_remove_the_parent_claude_dir_mount() {
    // The shadow OVERLAYS .credentials.json only — everything else
    // under ~/.claude/ (settings.json, agents/, plugins/, sessions/)
    // must still be shared with the host.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home/test-user");
    std::fs::create_dir_all(&home).unwrap();
    write_oauth_token(&home);
    let _g = EnvGuard::pin(&home);

    let project = home.join("proj");
    std::fs::create_dir_all(&project).unwrap();
    let vols = default_volumes(&project, "test-container");

    assert!(
        find_claude_dir_mount(&vols, &home).is_some(),
        "the parent ~/.claude/ mount must still be present alongside the shadow"
    );
}
