//! Desktop-environment detection + KDE Dolphin context-menu install.
//!
//! Different DEs expose "right-click context-menu actions on folders"
//! via incompatible mechanisms — there's no portable ABI. KDE uses
//! KIO ServiceMenus (`.desktop` files in
//! `~/.local/share/kio/servicemenus/`); GNOME uses Nautilus scripts
//! or extensions; XFCE uses Thunar custom actions XML; etc. We only
//! auto-install the KDE one; other DEs are documented for manual
//! setup at `docs/recipes/context-menu.md`.

use std::path::PathBuf;

use crate::paths;

/// What we detected about the user's desktop session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desktop {
    /// KDE Plasma — we can install the Dolphin servicemenu automatically.
    Kde,
    /// Some other DE that we don't have an artefact for. The string is
    /// the raw `$XDG_CURRENT_DESKTOP` value so the wizard can echo it.
    Other(String),
    /// `$XDG_CURRENT_DESKTOP` is unset or empty (e.g. running over SSH
    /// without a graphical session, or a minimal WM that doesn't set it).
    Unknown,
}

/// Read `$XDG_CURRENT_DESKTOP` and classify. Per the freedesktop spec
/// the value is a colon-separated list (e.g. `KDE:wayland` on some
/// distros) — substring match for our purposes.
pub fn detect() -> Desktop {
    match std::env::var("XDG_CURRENT_DESKTOP") {
        Ok(s) if s.is_empty() => Desktop::Unknown,
        Ok(s) if s.to_ascii_uppercase().contains("KDE") => Desktop::Kde,
        Ok(s) => Desktop::Other(s),
        Err(_) => Desktop::Unknown,
    }
}

/// Where the Dolphin servicemenu .desktop file lives in the user-local
/// XDG data dir. KF6 (Plasma 6) reads from this path; older KF5 used
/// `~/.local/share/kservices5/ServiceMenus/` but Plasma 6 has been out
/// long enough that we target the newer path only.
pub fn kde_servicemenu_user_path() -> PathBuf {
    paths::home()
        .join(".local/share/kio/servicemenus/open-in-claude-sandbox.desktop")
}

/// Where a distro package drops the servicemenu system-wide. Override
/// via `CS_SYSTEM_KIO_SERVICEMENUS_DIR` for tests; production default is
/// the FHS-canonical KIO path.
pub fn kde_servicemenu_system_path() -> PathBuf {
    let dir = match std::env::var_os("CS_SYSTEM_KIO_SERVICEMENUS_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from("/usr/share/kio/servicemenus"),
    };
    dir.join("open-in-claude-sandbox.desktop")
}

/// `Some(path)` of the actual install when the servicemenu is present
/// either user-locally or system-wide, `None` otherwise. User-local wins
/// when both exist (user overrides system per KDE precedence).
pub fn kde_servicemenu_installed_at() -> Option<PathBuf> {
    let user = kde_servicemenu_user_path();
    if user.exists() {
        return Some(user);
    }
    let sys = kde_servicemenu_system_path();
    if sys.exists() {
        return Some(sys);
    }
    None
}

pub fn kde_servicemenu_installed() -> bool {
    kde_servicemenu_installed_at().is_some()
}

/// Install the bundled .desktop entry into the KDE servicemenu dir.
/// Embedded via `include_str!` so the runtime install doesn't depend
/// on the source repo being present on disk. Mode 755 because KF6
/// requires servicemenu entries to be executable.
pub fn install_kde_servicemenu() -> std::io::Result<PathBuf> {
    const DESKTOP_ENTRY: &str =
        include_str!("../assets/dolphin/open-in-claude-sandbox.desktop");
    let dest = kde_servicemenu_user_path();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&dest, DESKTOP_ENTRY)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(dest)
}
