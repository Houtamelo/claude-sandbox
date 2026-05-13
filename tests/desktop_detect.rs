//! `desktop::detect` reads `$XDG_CURRENT_DESKTOP` and classifies into
//! the variants the wizard branches on. Must run serially — these
//! tests mutate the process-global env.

use std::sync::Mutex;

use claude_sandbox::desktop::{
    detect, kde_servicemenu_installed, kde_servicemenu_installed_at, kde_servicemenu_system_path,
    kde_servicemenu_user_path, render_servicemenu, Desktop,
};

static SERIAL: Mutex<()> = Mutex::new(());

fn with_xdg<F: FnOnce()>(value: Option<&str>, f: F) {
    let prev = std::env::var_os("XDG_CURRENT_DESKTOP");
    // SAFETY: serial test runs guarantee no concurrent env mutation.
    unsafe {
        match value {
            Some(v) => std::env::set_var("XDG_CURRENT_DESKTOP", v),
            None => std::env::remove_var("XDG_CURRENT_DESKTOP"),
        }
    }
    f();
    unsafe {
        match prev {
            Some(v) => std::env::set_var("XDG_CURRENT_DESKTOP", v),
            None => std::env::remove_var("XDG_CURRENT_DESKTOP"),
        }
    }
}

#[test]
fn kde_plain() {
    with_xdg(Some("KDE"), || assert_eq!(detect(), Desktop::Kde));
}

#[test]
fn kde_wayland_combo() {
    // Some distros set "KDE:wayland" (colon-separated list per the
    // freedesktop spec). Must still match.
    with_xdg(Some("KDE:wayland"), || assert_eq!(detect(), Desktop::Kde));
}

#[test]
fn kde_case_insensitive() {
    with_xdg(Some("kde-plasma"), || assert_eq!(detect(), Desktop::Kde));
    with_xdg(Some("kde"), || assert_eq!(detect(), Desktop::Kde));
}

#[test]
fn gnome_classifies_as_other() {
    with_xdg(Some("GNOME"), || {
        assert_eq!(detect(), Desktop::Other("GNOME".into()));
    });
}

#[test]
fn xfce_classifies_as_other() {
    with_xdg(Some("XFCE"), || {
        assert_eq!(detect(), Desktop::Other("XFCE".into()));
    });
}

#[test]
fn unset_is_unknown() {
    with_xdg(None, || assert_eq!(detect(), Desktop::Unknown));
}

#[test]
fn empty_string_is_unknown() {
    with_xdg(Some(""), || assert_eq!(detect(), Desktop::Unknown));
}

/// Hold HOME + CS_SYSTEM_KIO_SERVICEMENUS_DIR for the duration of one test,
/// restoring afterward. Tests using this take SERIAL so two of them never
/// run in parallel (env is process-global).
struct KioEnvGuard {
    home_prev: Option<std::ffi::OsString>,
    sys_prev: Option<std::ffi::OsString>,
}

impl KioEnvGuard {
    fn pin(home: &std::path::Path, sys_dir: &std::path::Path) -> Self {
        let home_prev = std::env::var_os("HOME");
        let sys_prev = std::env::var_os("CS_SYSTEM_KIO_SERVICEMENUS_DIR");
        unsafe {
            std::env::set_var("HOME", home);
            std::env::set_var("CS_SYSTEM_KIO_SERVICEMENUS_DIR", sys_dir);
        }
        KioEnvGuard { home_prev, sys_prev }
    }
}

impl Drop for KioEnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.home_prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match &self.sys_prev {
                Some(v) => std::env::set_var("CS_SYSTEM_KIO_SERVICEMENUS_DIR", v),
                None => std::env::remove_var("CS_SYSTEM_KIO_SERVICEMENUS_DIR"),
            }
        }
    }
}

#[test]
fn installed_detects_user_local_servicemenu() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys_dir = tmp.path().join("usr-share-kio");
    let user_smdir = home.join(".local/share/kio/servicemenus");
    std::fs::create_dir_all(&user_smdir).unwrap();
    std::fs::create_dir_all(&sys_dir).unwrap();
    std::fs::write(user_smdir.join("open-in-claude-sandbox.desktop"), "x").unwrap();
    let _g = KioEnvGuard::pin(&home, &sys_dir);

    assert!(kde_servicemenu_installed());
    assert_eq!(kde_servicemenu_installed_at(), Some(kde_servicemenu_user_path()));
}

#[test]
fn installed_detects_system_wide_servicemenu_from_package() {
    // Distro packages drop the servicemenu into /usr/share/kio/servicemenus/.
    // The cfg wizard must NOT then redundantly install a user-local copy.
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys_dir = tmp.path().join("usr-share-kio");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&sys_dir).unwrap();
    std::fs::write(sys_dir.join("open-in-claude-sandbox.desktop"), "x").unwrap();
    let _g = KioEnvGuard::pin(&home, &sys_dir);

    assert!(kde_servicemenu_installed());
    assert_eq!(kde_servicemenu_installed_at(), Some(kde_servicemenu_system_path()));
}

#[test]
fn installed_returns_false_when_absent_from_both() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys_dir = tmp.path().join("usr-share-kio");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&sys_dir).unwrap();
    let _g = KioEnvGuard::pin(&home, &sys_dir);

    assert!(!kde_servicemenu_installed());
    assert_eq!(kde_servicemenu_installed_at(), None);
}

#[test]
fn system_path_defaults_to_usr_share_kio_servicemenus() {
    let _lock = SERIAL.lock().unwrap();
    let prev = std::env::var_os("CS_SYSTEM_KIO_SERVICEMENUS_DIR");
    unsafe {
        std::env::remove_var("CS_SYSTEM_KIO_SERVICEMENUS_DIR");
    }
    assert_eq!(
        kde_servicemenu_system_path(),
        std::path::PathBuf::from("/usr/share/kio/servicemenus/open-in-claude-sandbox.desktop")
    );
    unsafe {
        match prev {
            Some(v) => std::env::set_var("CS_SYSTEM_KIO_SERVICEMENUS_DIR", v),
            None => std::env::remove_var("CS_SYSTEM_KIO_SERVICEMENUS_DIR"),
        }
    }
}

#[test]
fn render_substitutes_binary_path_into_exec_line() {
    // KDE Plasma's systemd-user session doesn't put ~/.cargo/bin or
    // similar on PATH, and `bash -lc` doesn't reliably source the
    // user's profile in the konsole-spawned chain. So the .desktop's
    // Exec line embeds an absolute binary path resolved at install
    // time via std::env::current_exe.
    let rendered = render_servicemenu(std::path::Path::new("/opt/claude-sandbox/bin/claude-sandbox"));
    assert!(
        rendered.contains("/opt/claude-sandbox/bin/claude-sandbox"),
        "rendered .desktop must contain the literal binary path; got:\n{rendered}"
    );
    assert!(
        !rendered.contains("{{BINARY}}"),
        "{{{{BINARY}}}} placeholder must be substituted, none left in output:\n{rendered}"
    );
}

#[test]
fn render_preserves_login_shell_invocation() {
    // Regression net for the earlier `bash -c` -> `bash -lc` fix. A
    // future refactor shouldn't drop the login-shell flag.
    let rendered = render_servicemenu(std::path::Path::new("/usr/bin/claude-sandbox"));
    assert!(
        rendered.contains("bash -lc"),
        "Exec must use login shell so /etc/profile.d is sourced; got:\n{rendered}"
    );
}

#[test]
fn render_preserves_konsole_invocation() {
    // Sanity: still a konsole launch. Catches a refactor that swaps
    // konsole for something else without updating the asset.
    let rendered = render_servicemenu(std::path::Path::new("/usr/bin/claude-sandbox"));
    assert!(
        rendered.contains("konsole --workdir %f"),
        "Exec must launch konsole with %f workdir; got:\n{rendered}"
    );
}

#[test]
fn render_keeps_dual_exec_for_kde_action_machinery() {
    // KF6 servicemenus need an `Exec=true` line in the main entry
    // (KDE uses it as a no-op when the menu is constructed) AND a
    // real Exec line inside the [Desktop Action ...] block. Dropping
    // either silently breaks the menu.
    let rendered = render_servicemenu(std::path::Path::new("/usr/bin/claude-sandbox"));
    let exec_lines: Vec<&str> = rendered.lines().filter(|l| l.starts_with("Exec=")).collect();
    assert_eq!(
        exec_lines.len(),
        2,
        "expected two Exec= lines (top-level no-op + action's real); got: {exec_lines:?}"
    );
    assert_eq!(exec_lines[0], "Exec=true", "top-level Exec must remain the no-op");
}

#[test]
fn user_path_under_home_xdg_data_dir() {
    let _lock = SERIAL.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let sys_dir = tmp.path().join("sys");
    let _g = KioEnvGuard::pin(&home, &sys_dir);

    assert_eq!(
        kde_servicemenu_user_path(),
        home.join(".local/share/kio/servicemenus/open-in-claude-sandbox.desktop")
    );
}
