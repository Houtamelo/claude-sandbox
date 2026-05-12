//! `desktop::detect` reads `$XDG_CURRENT_DESKTOP` and classifies into
//! the variants the wizard branches on. Must run serially — these
//! tests mutate the process-global env.

use claude_sandbox::desktop::{detect, Desktop};

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
