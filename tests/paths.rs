//! `paths::*` helpers translate `$HOME` into the three XDG-shaped dirs
//! claude-sandbox cares about. These tests are the regression net for
//! "someone silently changed where config / cache / data lands."

use std::sync::Mutex;

use claude_sandbox::paths;

static SERIAL: Mutex<()> = Mutex::new(());

struct HomeGuard {
    prev: Option<std::ffi::OsString>,
}

impl HomeGuard {
    fn pin(home: &std::path::Path) -> Self {
        let prev = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", home);
        }
        HomeGuard { prev }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }
}

#[test]
fn config_dir_is_dot_config_claude_sandbox_under_home() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let _g = HomeGuard::pin(tmp.path());
    assert_eq!(paths::config_dir(), tmp.path().join(".config/claude-sandbox"));
}

#[test]
fn data_dir_is_dot_local_share_claude_sandbox_under_home() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let _g = HomeGuard::pin(tmp.path());
    assert_eq!(
        paths::data_dir(),
        tmp.path().join(".local/share/claude-sandbox")
    );
}

#[test]
fn cache_dir_is_dot_cache_claude_sandbox_under_home() {
    // image::rebuild writes its build context under cache_dir; the cfg
    // wizard reads embedded defaults out of config_dir; recreating a
    // container relies on the registry under data_dir. Moving any of
    // these silently breaks deployed installs, so the path itself is
    // load-bearing.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let _g = HomeGuard::pin(tmp.path());
    assert_eq!(paths::cache_dir(), tmp.path().join(".cache/claude-sandbox"));
}

#[test]
fn all_three_share_the_same_home() {
    // Sanity: if HOME changes, every helper tracks it. Catches a
    // hypothetical regression where one helper caches HOME at process
    // start and the others read it live.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let _g = HomeGuard::pin(tmp.path());
    let home = paths::home();
    assert!(paths::config_dir().starts_with(&home));
    assert!(paths::data_dir().starts_with(&home));
    assert!(paths::cache_dir().starts_with(&home));
}
