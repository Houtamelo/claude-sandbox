//! Runtime SELinux detection.
//!
//! On hosts with SELinux loaded (openSUSE, Fedora, RHEL), bind-mounted
//! paths get labeled `user_tmp_t` / `user_home_t` and are denied to the
//! container's `container_t` context — we pass `--security-opt
//! label=disable` per-container to opt out (without mutating any host
//! labels, unlike `:z` / `:Z` mount flags).
//!
//! On hosts WITHOUT SELinux (Ubuntu, Mint, vanilla Arch, …), the flag
//! is a no-op but adds noise to the podman invocation, and on some
//! older podman versions causes a "unknown security option" warning.
//! Detect once at container-create time and only emit the flag when
//! actually needed.

/// True if the running kernel has SELinux loaded (regardless of whether
/// it's in enforcing or permissive mode — `label=disable` is needed in
/// both, only "fully disabled at boot" doesn't need it).
///
/// Detection: existence of `/sys/fs/selinux/enforce`. This file is
/// present iff the SELinux LSM is loaded by the kernel; absent on
/// kernels built without it (Ubuntu default) and on hosts that booted
/// with `selinux=0`. The `getenforce` command would be more readable
/// but it's not installed by default on every SELinux distro
/// (Tumbleweed ships SELinux without `selinux-utils`).
pub fn enabled() -> bool {
    std::path::Path::new("/sys/fs/selinux/enforce").exists()
}
